//! Transcript queue management so auto-send/order guarantees remain predictable.

use std::collections::VecDeque;
use voiceterm::{log_debug, VoiceCaptureSource};

use crate::config::VoiceSendMode;

pub(crate) const MAX_PENDING_TRANSCRIPTS: usize = 5;

/// Transcript queued while the CLI is busy.
pub(crate) struct PendingTranscript {
    /// Raw transcript text.
    pub(crate) text: String,
    /// Pipeline that produced the transcript.
    pub(crate) source: VoiceCaptureSource,
    /// Send mode to apply when flushing.
    pub(crate) mode: VoiceSendMode,
}

pub(crate) fn push_pending_transcript(
    pending: &mut VecDeque<PendingTranscript>,
    transcript: PendingTranscript,
) -> bool {
    if pending.len() >= MAX_PENDING_TRANSCRIPTS {
        pending.pop_front();
        log_debug("pending transcript queue full; dropping oldest transcript");
        pending.push_back(transcript);
        return true;
    }
    pending.push_back(transcript);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pending_transcript_drops_oldest_when_full() {
        let mut pending = VecDeque::new();
        for i in 0..MAX_PENDING_TRANSCRIPTS {
            let dropped = push_pending_transcript(
                &mut pending,
                PendingTranscript {
                    text: format!("t{i}"),
                    source: VoiceCaptureSource::Native,
                    mode: VoiceSendMode::Auto,
                },
            );
            assert!(!dropped);
        }
        let dropped = push_pending_transcript(
            &mut pending,
            PendingTranscript {
                text: "last".to_string(),
                source: VoiceCaptureSource::Native,
                mode: VoiceSendMode::Auto,
            },
        );
        assert!(dropped);
        assert_eq!(pending.len(), MAX_PENDING_TRANSCRIPTS);
        assert_eq!(pending.front().unwrap().text, "t1");
        assert_eq!(pending.back().unwrap().text, "last");
    }
}
