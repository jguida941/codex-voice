//! Transcript queue/delivery wiring so capture output reaches the CLI in order.

mod delivery;
mod idle;
mod queue;
mod session;

pub(crate) use delivery::{deliver_transcript, send_transcript, try_flush_pending, TranscriptIo};
pub(crate) use idle::transcript_ready;
pub(crate) use queue::{push_pending_transcript, PendingTranscript};
pub(crate) use session::TranscriptSession;
