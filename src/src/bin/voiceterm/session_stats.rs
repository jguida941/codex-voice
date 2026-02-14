//! Session statistics tracking so end-of-run summaries reflect real voice activity.
//!
//! Tracks voice capture statistics during a session and formats them for display.

use std::time::{Duration, Instant};

use crate::theme::{Theme, ThemeColors};

/// Statistics for a voice capture session.
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Number of successful transcripts
    pub transcripts: u32,
    /// Number of empty captures (no speech detected)
    pub empty_captures: u32,
    /// Number of errors
    pub errors: u32,
    /// Total speech duration in seconds
    pub total_speech_secs: f32,
    /// Session start time
    start_time: Option<Instant>,
}

impl SessionStats {
    pub fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Record a successful transcript.
    pub fn record_transcript(&mut self, duration_secs: f32) {
        self.transcripts += 1;
        self.total_speech_secs += duration_secs;
    }

    /// Record an empty capture (no speech detected).
    pub fn record_empty(&mut self) {
        self.empty_captures += 1;
    }

    /// Record an error.
    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    /// Get session duration.
    pub fn session_duration(&self) -> Duration {
        self.start_time
            .map(|start| start.elapsed())
            .unwrap_or_default()
    }

    /// Check if any activity occurred.
    pub fn has_activity(&self) -> bool {
        self.transcripts > 0 || self.empty_captures > 0 || self.errors > 0
    }

    /// Calculate average transcript duration.
    pub fn avg_transcript_duration(&self) -> f32 {
        if self.transcripts > 0 {
            self.total_speech_secs / self.transcripts as f32
        } else {
            0.0
        }
    }
}

/// Format session stats for display on exit.
pub fn format_session_stats(stats: &SessionStats, theme: Theme) -> String {
    if !stats.has_activity() {
        return String::new();
    }

    let colors = theme.colors();
    let mut lines = vec![
        String::new(), // Empty line before
        format_header(&colors),
        format_separator(),
        format_stat_line(
            &colors,
            "Transcripts",
            &stats.transcripts.to_string(),
            colors.success,
        ),
    ];

    // Total speech time
    let speech_time = format_duration(stats.total_speech_secs);
    lines.push(format_stat_line(&colors, "Total speech", &speech_time, ""));

    // Average duration
    if stats.transcripts > 0 {
        let avg = format!("{:.1}s", stats.avg_transcript_duration());
        lines.push(format_stat_line(&colors, "Avg duration", &avg, ""));
    }

    // Empty captures (if any)
    if stats.empty_captures > 0 {
        lines.push(format_stat_line(
            &colors,
            "No speech",
            &stats.empty_captures.to_string(),
            colors.warning,
        ));
    }

    // Errors (if any)
    if stats.errors > 0 {
        lines.push(format_stat_line(
            &colors,
            "Errors",
            &stats.errors.to_string(),
            colors.error,
        ));
    }

    // Session duration
    let session_dur = format_duration(stats.session_duration().as_secs_f32());
    lines.push(format_stat_line(&colors, "Session", &session_dur, ""));

    lines.push(String::new()); // Empty line after

    lines.join("\n")
}

fn format_header(colors: &ThemeColors) -> String {
    format!("{}Session Summary{}", colors.info, colors.reset)
}

fn format_separator() -> String {
    "───────────────".to_string()
}

fn format_stat_line(colors: &ThemeColors, label: &str, value: &str, value_color: &str) -> String {
    let value_display = if value_color.is_empty() {
        value.to_string()
    } else {
        format!("{}{}{}", value_color, value, colors.reset)
    };
    format!("{:<12} {}", label, value_display)
}

fn format_duration(secs: f32) -> String {
    if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        let mins = (secs / 60.0).floor();
        let remaining_secs = secs % 60.0;
        format!("{}m {:.0}s", mins as u32, remaining_secs)
    } else {
        let hours = (secs / 3600.0).floor();
        let remaining_mins = ((secs % 3600.0) / 60.0).floor();
        format!("{}h {}m", hours as u32, remaining_mins as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_stats_new() {
        let stats = SessionStats::new();
        assert_eq!(stats.transcripts, 0);
        assert!(!stats.has_activity());
    }

    #[test]
    fn session_stats_record_transcript() {
        let mut stats = SessionStats::new();
        stats.record_transcript(2.5);
        stats.record_transcript(3.0);
        assert_eq!(stats.transcripts, 2);
        assert_eq!(stats.total_speech_secs, 5.5);
        assert!(stats.has_activity());
    }

    #[test]
    fn session_stats_avg_duration() {
        let mut stats = SessionStats::new();
        stats.record_transcript(2.0);
        stats.record_transcript(4.0);
        assert_eq!(stats.avg_transcript_duration(), 3.0);
    }

    #[test]
    fn session_stats_record_empty_and_error() {
        let mut stats = SessionStats::new();
        stats.record_empty();
        stats.record_error();
        assert_eq!(stats.empty_captures, 1);
        assert_eq!(stats.errors, 1);
        assert!(stats.has_activity());
    }

    #[test]
    fn session_duration_reflects_started_session() {
        let mut stats = SessionStats::new();
        stats.start_time = Some(Instant::now() - Duration::from_millis(25));
        assert!(stats.session_duration() >= Duration::from_millis(25));
    }

    #[test]
    fn has_activity_is_true_for_each_counter_individually() {
        let mut only_transcript = SessionStats::new();
        only_transcript.transcripts = 1;
        assert!(only_transcript.has_activity());

        let mut only_empty = SessionStats::new();
        only_empty.empty_captures = 1;
        assert!(only_empty.has_activity());

        let mut only_error = SessionStats::new();
        only_error.errors = 1;
        assert!(only_error.has_activity());
    }

    #[test]
    fn avg_transcript_duration_zero_when_no_transcripts() {
        let stats = SessionStats::new();
        assert_eq!(stats.avg_transcript_duration(), 0.0);
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(30.5), "30.5s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(125.0), "2m 5s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3725.0), "1h 2m");
    }

    #[test]
    fn format_duration_uses_minute_and_hour_boundaries() {
        assert_eq!(format_duration(60.0), "1m 0s");
        assert_eq!(format_duration(3600.0), "1h 0m");
    }

    #[test]
    fn format_session_stats_empty() {
        let stats = SessionStats::new();
        let output = format_session_stats(&stats, Theme::Coral);
        assert!(output.is_empty());
    }

    #[test]
    fn format_session_stats_with_activity() {
        let mut stats = SessionStats::new();
        stats.record_transcript(5.0);
        let output = format_session_stats(&stats, Theme::Coral);
        assert!(output.contains("Session Summary"));
        assert!(output.contains("Transcripts"));
        assert!(output.contains("1"));
        assert!(!output.contains("No speech"));
    }

    #[test]
    fn format_session_stats_conditional_rows_follow_zero_vs_nonzero_counts() {
        let mut stats = SessionStats::new();
        stats.transcripts = 0;
        stats.total_speech_secs = 12.0;
        stats.empty_captures = 1;
        stats.errors = 0;
        let output = format_session_stats(&stats, Theme::None);
        assert!(!output.contains("Avg duration"));
        assert!(output.contains("No speech"));
        assert!(!output.contains("Errors"));

        stats.transcripts = 1;
        stats.errors = 1;
        let output = format_session_stats(&stats, Theme::None);
        assert!(output.contains("Avg duration"));
        assert!(output.contains("No speech"));
        assert!(output.contains("Errors"));
    }

    #[test]
    fn format_separator_is_stable() {
        assert_eq!(format_separator(), "───────────────");
    }
}
