# Codex Voice Architecture Redesign

## Executive Summary

The current implementation is fundamentally backwards. We're fighting against Codex's design instead of working with it. Every voice capture spawns new processes, loses context, and requires increasingly complex workarounds. This document outlines the correct architecture.

## Current Problems (Your Analysis is 100% Correct)

### 1. Process Lifecycle Disaster
**Problem**: Every request spawns fresh codex, ffmpeg, and whisper processes
- Codex never maintains session state
- Lost context after each interaction
- Lost approvals and tool access
- Forces PTY juggling and exec fallbacks

**Impact**: The Enter key bug is a SYMPTOM of this - we're corrupting state because we keep reinitializing everything.

### 2. Environment Assumptions
**Problem**: Hardcoded paths and binaries
- Assumes whisper in `.venv/bin/`
- Assumes codex on PATH
- No discovery or verification
- User has to manually fix paths

**Impact**: "It doesn't work" issues are mostly path problems we shouldn't have.

### 3. Output Handling
**Problem**: Buffering destroys interactivity
- Hide Codex's real-time output
- Make approvals impossible
- Force --skip-git-repo-check workarounds
- Silent waits with no feedback

**Impact**: Users can't see what Codex is doing, can't approve operations.

### 4. Packaging Nightmare
**Problem**: No real installation process
- Scripts scattered in repo
- Manual path configuration
- Can't drop into other projects
- No pip/homebrew/cargo package

**Impact**: Users copy commands by hand, nothing "just works".

## The Correct Architecture (Full Rust Implementation)

### Core Principle
**Codex is the application, we are a thin input layer**. We should:
1. Start Codex once
2. Keep it alive
3. Stream its output
4. Inject voice-transcribed prompts
5. Never alter its behavior

### Why Rust for Everything
- **Speed**: 10-100x faster than Python for audio processing
- **Memory Safety**: No segfaults, guaranteed thread safety
- **Zero-Cost Abstractions**: High-level code with no runtime overhead
- **Native Async**: Tokio for concurrent operations without blocking
- **Single Binary**: Ship one executable, no Python/Node dependencies

### Architecture Components

```
┌─────────────────────────────────────────────────────────┐
│                     User Interface                       │
│  (TUI with streaming output, input field, status bar)    │
└─────────────────┬───────────────────┬───────────────────┘
                  │                   │
         ┌────────▼────────┐  ┌──────▼──────┐
         │ Voice Pipeline  │  │   Keyboard  │
         │    Service      │  │    Input    │
         └────────┬────────┘  └──────┬──────┘
                  │                   │
         ┌────────▼───────────────────▼──────┐
         │     Session Manager               │
         │  - Maintains single Codex PTY     │
         │  - Streams output to UI           │
         │  - Injects prompts from any source│
         │  - Preserves all Codex state      │
         └────────────────┬──────────────────┘
                          │
         ┌────────────────▼──────────────────┐
         │        Codex Process              │
         │  (Long-lived, stable session)     │
         └───────────────────────────────────┘
```

### 1. Stable Codex Session

```rust
struct CodexSession {
    pty: PtyProcess,
    state: SessionState,
    output_stream: mpsc::Sender<String>,
}

impl CodexSession {
    fn start() -> Result<Self> {
        // Start Codex ONCE with proper PTY
        let pty = PtyProcess::spawn("codex", &["--interactive"])?;

        // Stream output continuously
        let output_stream = spawn_output_reader(pty.stdout);

        Ok(Self { pty, state: Active, output_stream })
    }

    fn send_prompt(&mut self, text: &str) -> Result<()> {
        // Just write to PTY stdin
        self.pty.stdin.write_all(text.as_bytes())?;
        Ok(())
    }
}
```

### 2. Voice Pipeline as Service (Pure Rust)

```rust
use whisper_rs::{WhisperContext, FullParams};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

struct VoiceService {
    whisper_ctx: Arc<WhisperContext>,  // Model loaded once, shared
    audio_device: cpal::Device,
    sample_rate: u32,
}

impl VoiceService {
    fn new() -> Result<Self> {
        // Load model once at startup
        let whisper_ctx = WhisperContext::new("models/ggml-base.bin")?;

        // Initialize audio device once
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or("No input device")?;

        Ok(Self {
            whisper_ctx: Arc::new(whisper_ctx),
            audio_device: device,
            sample_rate: 16000,
        })
    }

    async fn capture_and_transcribe(&self, duration: Duration) -> Result<String> {
        // Direct audio capture in Rust (no FFmpeg)
        let audio = self.capture_audio(duration).await?;

        // Transcribe with whisper.cpp bindings (no subprocess)
        let params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        self.whisper_ctx.full(params, &audio)
    }

    async fn capture_audio(&self, duration: Duration) -> Result<Vec<f32>> {
        let (tx, rx) = mpsc::channel();

        // Stream audio directly from device
        let stream = self.audio_device.build_input_stream(
            &self.config(),
            move |data: &[f32], _: &_| {
                tx.send(data.to_vec()).unwrap();
            },
            |err| eprintln!("Audio error: {}", err),
        )?;

        stream.play()?;
        tokio::time::sleep(duration).await;
        stream.pause()?;

        // Collect samples
        let samples: Vec<f32> = rx.try_iter().flatten().collect();
        Ok(samples)
    }
}
```

### 3. Advanced Features (Rust-Powered)

#### Real-time Voice Activity Detection (VAD)
```rust
struct SmartVoiceCapture {
    vad: VoiceActivityDetector,

    async fn capture_until_silence(&self) -> Result<Vec<f32>> {
        let mut buffer = Vec::new();
        let mut silence_duration = Duration::ZERO;

        while silence_duration < Duration::from_secs(2) {
            let chunk = self.capture_chunk().await?;

            if self.vad.is_speech(&chunk) {
                buffer.extend(chunk);
                silence_duration = Duration::ZERO;
            } else {
                silence_duration += CHUNK_DURATION;
            }
        }

        Ok(buffer)
    }
}
```

#### Wake Word Detection
```rust
struct WakeWordDetector {
    model: PicovoiceRust,

    async fn listen_for_wake_word(&self) -> Result<()> {
        loop {
            let audio = self.capture_chunk().await?;
            if self.model.process(&audio)? {
                return Ok(()); // Wake word detected
            }
        }
    }
}

// Usage: "Hey Codex" activates voice capture
```

#### Multi-language Support
```rust
enum Language {
    English, Spanish, French, German, Japanese, Chinese
}

impl VoiceService {
    fn with_language(&mut self, lang: Language) -> &mut Self {
        self.whisper_params.language = lang.to_whisper_code();
        self
    }
}
```

#### Streaming Transcription
```rust
impl VoiceService {
    async fn stream_transcription(&self) -> impl Stream<Item = String> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let mut buffer = RingBuffer::new(16000); // 1 second

            loop {
                let chunk = self.capture_chunk().await?;
                buffer.push(chunk);

                // Transcribe every 500ms for near real-time
                if buffer.len() >= 8000 {
                    let text = self.whisper_ctx.transcribe(&buffer)?;
                    tx.send(text).await?;
                }
            }
        });

        ReceiverStream::new(rx)
    }
}

### 3. Streaming Output

```rust
fn stream_codex_output(pty: &mut PtyProcess, ui: &mut UI) {
    let reader = BufReader::new(&pty.stdout);
    for line in reader.lines() {
        let line = line?;

        // Stream to UI immediately
        ui.append_output(&line);

        // Also log for debugging
        log::debug!("Codex: {}", line);

        // Let UI handle any needed responses
        if line.contains("Approve?") {
            ui.show_approval_prompt();
        }
    }
}
```

### 4. Proper Configuration

```toml
# ~/.config/codex_voice/config.toml

[codex]
command = "codex"
args = ["--interactive"]
working_dir = "."

[whisper]
command = "/opt/homebrew/bin/whisper"
model = "base"
# OR use server mode
server_url = "http://localhost:8080"

[audio]
device = ":0"  # macOS default
duration = 5
format = "wav"

[ui]
theme = "dark"
show_status_bar = true
```

### 5. Installation That Works

```bash
# Method 1: Homebrew
brew tap codex-voice/tap
brew install codex-voice

# Method 2: Cargo
cargo install codex-voice

# Method 3: pip
pip install codex-voice

# First run auto-configures
codex-voice init
# > Found ffmpeg at /usr/local/bin/ffmpeg ✓
# > Found whisper at /opt/homebrew/bin/whisper ✓
# > Found codex at /usr/local/bin/codex ✓
# > Configuration saved to ~/.config/codex_voice/config.toml

# Then from ANY directory
codex-voice
# Just works. No path juggling.
```

## Performance Optimizations (Rust-Specific)

### Zero-Copy Audio Pipeline
```rust
// Avoid allocations with ring buffers and memory pools
struct ZeroCopyAudioPipeline {
    ring_buffer: Arc<Mutex<RingBuffer<f32>>>,
    memory_pool: MemoryPool<AudioChunk>,
}

impl ZeroCopyAudioPipeline {
    fn process_audio(&self) -> Result<()> {
        // Reuse buffers, no allocations in hot path
        let chunk = self.memory_pool.acquire();
        self.ring_buffer.read_into(&mut chunk)?;
        self.whisper.process_borrowed(&chunk)?;
        self.memory_pool.release(chunk);
        Ok(())
    }
}
```

### SIMD Optimizations
```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

fn downsample_audio_simd(input: &[f32], output: &mut [f32]) {
    unsafe {
        // Use AVX2 for 8x parallel processing
        for chunk in input.chunks_exact(8) {
            let vec = _mm256_loadu_ps(chunk.as_ptr());
            let downsampled = _mm256_mul_ps(vec, _mm256_set1_ps(0.5));
            _mm256_storeu_ps(output.as_mut_ptr(), downsampled);
        }
    }
}
```

### GPU Acceleration (Optional)
```rust
#[cfg(feature = "cuda")]
struct GpuWhisper {
    cuda_ctx: CudaContext,

    fn transcribe_gpu(&self, audio: &[f32]) -> Result<String> {
        // Offload to GPU for 10x speedup
        let gpu_buffer = self.cuda_ctx.alloc(audio.len())?;
        self.cuda_ctx.copy_to_device(audio, &gpu_buffer)?;
        self.cuda_ctx.run_whisper_kernel(&gpu_buffer)?;
        self.cuda_ctx.copy_from_device(&gpu_buffer)
    }
}
```

## Additional Features

### 1. Voice Commands
```rust
enum VoiceCommand {
    StartCapture,
    StopCapture,
    ClearInput,
    SendMessage,
    ScrollUp,
    ScrollDown,
    Exit,
}

impl VoiceCommand {
    fn from_text(text: &str) -> Option<Self> {
        match text.to_lowercase().as_str() {
            "start recording" => Some(StartCapture),
            "stop recording" => Some(StopCapture),
            "clear" | "clear input" => Some(ClearInput),
            "send" | "send message" => Some(SendMessage),
            "scroll up" => Some(ScrollUp),
            "scroll down" => Some(ScrollDown),
            "exit" | "quit" => Some(Exit),
            _ => None,
        }
    }
}
```

### 2. Noise Cancellation
```rust
use nnnoiseless::DenoiseState;

struct NoiseCancellation {
    denoiser: DenoiseState,

    fn process(&mut self, audio: &mut [f32]) -> Result<()> {
        // Real-time noise removal
        self.denoiser.process_frame(audio)?;
        Ok(())
    }
}
```

### 3. Speaker Diarization
```rust
struct SpeakerDiarization {
    embeddings: HashMap<SpeakerId, Vec<f32>>,

    fn identify_speaker(&self, audio: &[f32]) -> SpeakerId {
        // Identify who is speaking
        let embedding = self.extract_embedding(audio);
        self.find_closest_speaker(embedding)
    }
}
```

### 4. Context-Aware Completions
```rust
struct ContextAwareTranscription {
    project_context: ProjectContext,

    fn transcribe_with_context(&self, audio: &[f32]) -> String {
        // Use project-specific vocabulary
        let hints = self.project_context.get_vocabulary();
        self.whisper.transcribe_with_hints(audio, hints)
    }
}
```

## Migration Path (Rust-First)

### Phase 1: Core Rust Implementation (1 week)
- [x] Basic TUI in Rust (done)
- [ ] Replace FFmpeg with cpal for audio
- [ ] Integrate whisper.cpp via rust bindings
- [ ] Implement persistent Codex session

### Phase 2: Performance Optimizations (3-4 days)
- [ ] Add ring buffers for zero-copy audio
- [ ] Implement SIMD optimizations
- [ ] Add voice activity detection
- [ ] Profile and optimize hot paths

### Phase 3: Advanced Features (1 week)
- [ ] Wake word detection ("Hey Codex")
- [ ] Streaming transcription
- [ ] Voice commands
- [ ] Multi-language support

### Phase 4: Distribution (2-3 days)
- [ ] Single binary with embedded models
- [ ] Cross-compilation for Linux/Windows
- [ ] Homebrew formula
- [ ] GitHub releases with CI/CD

## Why This Fixes Everything

1. **Enter key bug**: Goes away because we're not corrupting state between captures
2. **5-6 second delays**: Gone, Codex stays warm
3. **Lost approvals**: Fixed, session persists
4. **Path issues**: Config layer handles discovery
5. **Can't see output**: Streaming shows everything
6. **Hard to install**: Package manager handles it

## Immediate Next Steps

1. **Agree on this design** - This is the correct path
2. **Start with Phase 1** - Quick win, fixes current bugs
3. **Document requirements** - Lock down expectations
4. **Begin refactor** - SessionManager first

## Conclusion

Your diagnosis is perfect. We've been building workarounds instead of a proper integration. The current bugs (Enter key, F2/Alt+R not working, delays) are all symptoms of fighting against Codex's design instead of embracing it.

Let's stop hacking and build this correctly.