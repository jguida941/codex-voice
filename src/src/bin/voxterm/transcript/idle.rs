//! Idle/readiness checks so auto-send triggers only when prompt timing is safe.

use std::time::{Duration, Instant};

use crate::prompt::PromptTracker;

fn prompt_ready(prompt_tracker: &PromptTracker, last_enter_at: Option<Instant>) -> bool {
    match (prompt_tracker.last_prompt_seen_at(), last_enter_at) {
        (Some(prompt_at), Some(enter_at)) => prompt_at > enter_at,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(crate) fn transcript_ready(
    prompt_tracker: &PromptTracker,
    last_enter_at: Option<Instant>,
    now: Instant,
    transcript_idle_timeout: Duration,
) -> bool {
    if prompt_ready(prompt_tracker, last_enter_at) {
        return true;
    }
    let idle_ready = if let Some(last_output_at) = prompt_tracker.last_pty_output_at() {
        now.duration_since(last_output_at) >= transcript_idle_timeout
    } else {
        prompt_tracker.idle_ready(now, transcript_idle_timeout)
    };
    if prompt_tracker.last_prompt_seen_at().is_none() {
        return idle_ready;
    }
    if let (Some(enter_at), Some(last_output_at)) =
        (last_enter_at, prompt_tracker.last_pty_output_at())
    {
        if last_output_at >= enter_at && idle_ready {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::PromptLogger;
    use regex::Regex;

    #[test]
    fn transcript_ready_falls_back_after_output_idle_since_enter() {
        let logger = PromptLogger::new(None);
        let regex = Regex::new(r"^> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), false, logger);

        tracker.feed_output(b"> \n");
        let last_enter_at = Some(Instant::now());
        tracker.feed_output(b"working...\n");

        let idle_timeout = Duration::from_millis(10);
        let now = Instant::now() + idle_timeout + Duration::from_millis(1);
        assert!(transcript_ready(&tracker, last_enter_at, now, idle_timeout));
    }
}
