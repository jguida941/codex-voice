//! Whisper speech-to-text integration so capture results become local transcripts.
//!
//! Wraps `whisper_rs` to provide a simple transcription API. The model is loaded
//! once and reused across captures to avoid repeated initialization overhead.

#[cfg(unix)]
mod platform {
    use crate::config::AppConfig;
    use crate::log_debug;
    use anyhow::{anyhow, Context, Result};
    use std::io;
    use std::os::raw::{c_char, c_uint, c_void};
    use std::os::unix::io::AsRawFd;
    use std::sync::Once;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    /// Whisper model context for speech-to-text transcription.
    ///
    /// Holds the loaded GGML model in memory. Create once at startup and reuse
    /// for all transcription requests to avoid repeated model loading.
    pub struct Transcriber {
        ctx: WhisperContext,
    }

    impl Transcriber {
        /// Loads the Whisper model from disk.
        ///
        /// Temporarily redirects stderr to `/dev/null` during loading because
        /// whisper.cpp emits verbose initialization messages.
        ///
        /// Note: this is a process-wide redirect; we keep it brief and expect
        /// model loading to happen during startup before other threads log.
        ///
        /// # Errors
        ///
        /// Returns an error if the model file cannot be loaded or stderr
        /// redirection fails.
        pub fn new(model_path: &str) -> Result<Self> {
            install_whisper_log_silencer();

            let null = std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .context("failed to open /dev/null")?;
            let null_fd = null.as_raw_fd();

            // SAFETY: dup(2) duplicates the stderr file descriptor. We restore it
            // after model loading completes. This is safe because we hold the only
            // reference and restore before returning.
            let orig_stderr = unsafe { libc::dup(2) };
            if orig_stderr < 0 {
                return Err(anyhow!(
                    "failed to dup stderr: {}",
                    io::Error::last_os_error()
                ));
            }

            // Redirect stderr to /dev/null temporarily
            // SAFETY: dup2 replaces stderr with /dev/null; both fds are valid.
            let dup_result = unsafe { libc::dup2(null_fd, 2) };
            if dup_result < 0 {
                // SAFETY: orig_stderr is a valid fd from dup(2).
                unsafe {
                    libc::close(orig_stderr);
                }
                return Err(anyhow!(
                    "failed to redirect stderr: {}",
                    io::Error::last_os_error()
                ));
            }

            // Load model (output will be suppressed)
            let ctx_result =
                WhisperContext::new_with_params(model_path, WhisperContextParameters::default());

            // Restore original stderr
            // SAFETY: restore stderr using the saved fd from dup(2).
            let restore_result = unsafe { libc::dup2(orig_stderr, 2) };
            // SAFETY: orig_stderr is a valid fd returned by dup(2).
            unsafe {
                libc::close(orig_stderr);
            }
            if restore_result < 0 {
                return Err(anyhow!(
                    "failed to restore stderr: {}",
                    io::Error::last_os_error()
                ));
            }

            let ctx = ctx_result.context("failed to load whisper model")?;
            Ok(Self { ctx })
        }

        /// Run transcription for the captured PCM samples and return the concatenated text.
        pub fn transcribe(&self, samples: &[f32], config: &AppConfig) -> Result<String> {
            let mut state = self
                .ctx
                .create_state()
                .context("failed to create whisper state")?;
            let mut params = if config.whisper_beam_size > 1 {
                FullParams::new(SamplingStrategy::BeamSearch {
                    beam_size: config.whisper_beam_size as i32,
                    patience: -1.0,
                })
            } else {
                FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
            };
            if config.lang.eq_ignore_ascii_case("auto") {
                params.set_language(None);
                params.set_detect_language(true);
            } else {
                params.set_language(Some(&config.lang));
                params.set_detect_language(false);
            }
            params.set_temperature(config.whisper_temperature);
            // Limit CPU usage so laptops don't max out all cores.
            params.set_n_threads(num_cpus::get().min(8) as i32);
            params.set_print_progress(false);
            params.set_print_timestamps(false);
            params.set_print_special(false);
            params.set_print_realtime(false);
            params.set_translate(false);
            params.set_token_timestamps(false);
            state.full(params, samples)?;
            let mut transcript = String::new();
            let num_segments = match state.full_n_segments() {
                Ok(count) => count,
                Err(err) => {
                    log_debug(&format!("Whisper failed to read segment count: {err}"));
                    return Ok(transcript);
                }
            };
            if num_segments < 0 {
                log_debug("Whisper returned a negative segment count");
                return Ok(transcript);
            }
            // Whisper splits output into small segments; stitch them together.
            for i in 0..num_segments {
                match state.full_get_segment_text_lossy(i) {
                    Ok(text) => transcript.push_str(&text),
                    Err(err) => log_debug(&format!("Failed to read whisper segment {i}: {err}")),
                }
            }
            // Filter out Whisper's [BLANK_AUDIO] token
            let filtered = transcript.replace("[BLANK_AUDIO]", "");
            Ok(filtered)
        }
    }

    fn install_whisper_log_silencer() {
        static INSTALL_LOG_CALLBACK: Once = Once::new();
        INSTALL_LOG_CALLBACK.call_once(|| unsafe {
            // SAFETY: whisper_rs expects a valid callback pointer; we pass a function
            // that ignores its inputs and never dereferences raw pointers.
            whisper_rs::set_log_callback(Some(whisper_log_callback), std::ptr::null_mut());
        });
    }

    #[allow(unused_variables)]
    unsafe extern "C" fn whisper_log_callback(
        _level: c_uint,
        _text: *const c_char,
        _user_data: *mut c_void,
    ) {
        // Silence the default whisper.cpp logger so it does not corrupt the TUI.
        // SAFETY: We do not dereference any incoming pointers.
    }
}

#[cfg(unix)]
pub use platform::Transcriber;

#[cfg(not(unix))]
mod platform {
    use anyhow::{anyhow, Result};

    /// Stub implementation for unsupported targets such as Windows.
    pub struct Transcriber;

    impl Transcriber {
        pub fn new(_: &str) -> Result<Self> {
            Err(anyhow!(
                "Whisper transcription is currently supported only on Unix-like platforms"
            ))
        }

        pub fn transcribe(&self, _: &[f32], _: &AppConfig) -> Result<String> {
            Err(anyhow!(
                "Whisper transcription is currently supported only on Unix-like platforms"
            ))
        }
    }
}

#[cfg(not(unix))]
pub use platform::Transcriber;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn transcriber_rejects_missing_model() {
        let result = Transcriber::new("/no/such/model.bin");
        assert!(result.is_err());
    }
}
