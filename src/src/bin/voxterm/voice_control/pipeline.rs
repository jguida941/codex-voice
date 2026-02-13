//! Voice-pipeline capability logic so UI labels match the active capture path.

use voxterm::VoiceCaptureSource;

pub(super) fn using_native_pipeline(has_transcriber: bool, has_recorder: bool) -> bool {
    has_transcriber && has_recorder
}

pub(super) fn pipeline_status_label(source: VoiceCaptureSource) -> &'static str {
    match source {
        VoiceCaptureSource::Native => "Rust",
        VoiceCaptureSource::Python => "Python",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn using_native_pipeline_requires_both_components() {
        assert!(!using_native_pipeline(false, false));
        assert!(!using_native_pipeline(true, false));
        assert!(!using_native_pipeline(false, true));
        assert!(using_native_pipeline(true, true));
    }
}
