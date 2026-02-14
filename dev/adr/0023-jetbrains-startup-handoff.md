# ADR 0023: JetBrains Startup Handoff and Resize Ghost-Frame Cleanup

Status: Accepted
Date: 2026-02-14

## Context

JetBrains IDE terminals (PyCharm/IntelliJ/CLion/WebStorm) showed two recurring startup/layout failures that did not reproduce in Cursor:

1. Alternate-screen splash handoff could leave visual artifacts before the live PTY/HUD view.
2. Early/late terminal geometry transitions could leave stale HUD borders in old row positions (ghost frames).

These failures were high-impact for first-run trust because they visually overlapped backend output and the input area.

## Decision

Adopt a JetBrains-specific startup/render hardening strategy:

1. Auto-skip startup splash in JetBrains terminal environments by detecting IDE-specific env markers (`PYCHARM_HOSTED`, `JETBRAINS_IDE`, `IDEA_*`, `CLION_IDE`, `WEBSTORM_IDE`) and JetBrains-like terminal identifiers.
2. On writer resize events, clear previously rendered HUD/overlay rows at the old geometry before drawing at the new geometry.

`VOICETERM_NO_STARTUP_BANNER=1` remains available as a global override for all terminals.

## Consequences

**Positive:**
- Removes JetBrains-specific splash handoff artifacts.
- Prevents stale/ghost HUD borders after startup or runtime resize transitions.
- Preserves behavior in non-JetBrains terminals where splash rendering is stable.

**Negative:**
- Startup behavior is intentionally environment-dependent (JetBrains terminals skip splash by default).
- Detection relies on environment markers that may vary by IDE versions/plugins.

**Trade-offs:**
- Favor reliable first-paint UX over strict cross-terminal splash parity.
- Keep manual env override so users can enforce one behavior globally.

## Alternatives Considered

- **Keep splash everywhere and tune timing only**: reduced but did not eliminate JetBrains artifacts.
- **Force-disable splash for all terminals**: too broad; removes a useful startup cue where rendering is stable.
- **Use scroll-region control to guard splash transition**: added complexity and prior regressions; not needed for this issue class.

## Links

- `src/src/bin/voiceterm/banner.rs`
- `src/src/bin/voiceterm/writer/state.rs`
- `guides/TROUBLESHOOTING.md`
