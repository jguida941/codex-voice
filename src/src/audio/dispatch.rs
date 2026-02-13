//! Bounded audio-frame dispatch that isolates callback pressure from processing loops.

use crossbeam_channel::{Sender, TrySendError};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Downmix multi-channel input to mono while applying the provided converter so
/// Whisper receives a single channel regardless of the microphone layout.
pub(super) fn append_downmixed_samples<T, F>(
    buf: &mut Vec<f32>,
    data: &[T],
    channels: usize,
    mut convert: F,
) where
    T: Copy,
    F: FnMut(T) -> f32,
{
    if channels <= 1 {
        buf.extend(data.iter().copied().map(&mut convert));
        return;
    }

    // Average each interleaved frame so VAD/STT see one stable mono stream
    // regardless of hardware channel layout.
    let mut acc = 0.0f32;
    let mut count = 0usize;
    for sample in data.iter().copied() {
        acc += convert(sample);
        count += 1;
        if count == channels {
            buf.push(acc / channels as f32);
            acc = 0.0;
            count = 0;
        }
    }
    if count > 0 {
        buf.push(acc / count as f32);
    }
}

pub(super) struct FrameDispatcher {
    frame_samples: usize,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    sender: Sender<Vec<f32>>,
    dropped: Arc<AtomicUsize>,
}

impl FrameDispatcher {
    pub(super) fn new(
        frame_samples: usize,
        sender: Sender<Vec<f32>>,
        dropped: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            frame_samples: frame_samples.max(1),
            pending: Vec::with_capacity(frame_samples),
            scratch: Vec::new(),
            sender,
            dropped,
        }
    }

    pub(super) fn push<T, F>(&mut self, data: &[T], channels: usize, convert: F)
    where
        T: Copy,
        F: FnMut(T) -> f32,
    {
        self.scratch.clear();
        append_downmixed_samples(&mut self.scratch, data, channels, convert);
        self.pending.extend_from_slice(&self.scratch);

        while self.pending.len() >= self.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.frame_samples).collect();
            if let Err(err) = self.sender.try_send(frame) {
                match err {
                    TrySendError::Full(_) => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    TrySendError::Disconnected(_) => break,
                }
            }
        }
    }
}
