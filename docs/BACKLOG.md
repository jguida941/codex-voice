# Backlog

## UX
- [ ] Auto-voice status: while a capture is active in auto mode, the status line should keep showing "Listening..." even after toggling send mode or other settings.
- [ ] Add a mic-meter hotkey so users can calibrate VAD without restarting the app.
- [ ] Optional HUD input preview while Codex is thinking (Phase 3).
- [ ] Processing delay/freeze after sending input while Codex is thinking (audit after the queue fix).
- [ ] Status spam: repeated "Transcript ready (Rust pipeline)" lines appear many times in a row. (needs repro after status dedupe/redraw gating)

## Bugs / Reliability
- [ ] Verify CSI-u filtering fix (overlay input now drops CSI-u sequences; confirm in real sessions).
- [ ] Verify transcript queueing while Codex is busy (queue now waits for next prompt; confirm in real sessions).
- [ ] Unexpected command hint appears in output: "Use /skills to list available sk ..." shows up in the UI. (likely provider output; not emitted by overlay)
