# ADR 0016: Modular Visual Styling System

Status: Accepted
Date: 2026-01-30

## Context

The overlay status line was originally plain text with no colors or visual
differentiation. This made it difficult to:
- Quickly identify current state (recording, processing, error)
- Scan status messages at a glance
- Match the polish of modern CLI tools (Claude CLI, etc.)

Users expected visual feedback similar to professional terminal applications.

## Decision

Implement a modular visual styling system with these components:

### Core Modules

1. **`status_style.rs`** - Status message categorization
   - `StatusType` enum: Recording, Processing, Success, Warning, Error, Info
   - Auto-detection from message content via `from_message()`
   - Unicode prefixes: `● REC`, `◐`, `✓`, `⚠`, `✗`, `ℹ`

2. **`theme/`** - Color theme system
   - `Theme` enum: ChatGpt, Claude, Codex, Coral, Catppuccin, Dracula, Nord, Ansi, None
   - Backend-default themes: Claude backend → `claude`, Codex backend → `codex`, others → `coral`
   - `ThemeColors` struct with semantic colors (recording, success, error, etc.)
   - 24-bit RGB colors for modern terminals

3. **`color_mode.rs`** - Terminal capability detection
   - `ColorMode` enum: TrueColor, Color256, Ansi16, None
   - Auto-detection from `COLORTERM`, `TERM` environment
   - Respects `NO_COLOR` standard (https://no-color.org/)

4. **`status_line.rs`** - Enhanced status line layout
   - Structured format: `◉ AUTO │ -35dB │ Ready   Ctrl+R rec`
   - Mode indicator, pipeline tag (during recording), sensitivity, message, shortcuts

### CLI Flags

- `--theme <name>` - Select color theme
- `--no-color` - Disable all colors

### Design Principles

- **Modular**: Each concern in its own module
- **Backward compatible**: Simple `Status { text }` still works
- **Graceful degradation**: Falls back to plain text if colors unsupported
- **Standards compliant**: Respects NO_COLOR, detects terminal capabilities

## Consequences

**Positive:**
- Professional, polished appearance
- Quick visual identification of states
- User-configurable themes
- Accessible (NO_COLOR support)
- Testable (each module has unit tests)
- Extensible (easy to add new themes)

**Negative:**
- More code to maintain (8 new modules)
- ANSI escape codes add complexity to output parsing
- Theme colors may not look good in all terminals
- Slight increase in binary size

**Trade-offs:**
- Modularity over simplicity (worth it for maintainability)
- Rich features over minimal footprint (acceptable for a TUI app)

## Modules Created

| Module | Purpose | Lines |
|--------|---------|-------|
| `status_style.rs` | Status categorization and prefixes | ~180 |
| `theme/` | Color themes (Coral, Catppuccin, Dracula, Nord) | ~170 |
| `color_mode.rs` | Terminal capability detection | ~165 |
| `status_line.rs` | Enhanced status line layout | ~250 |
| `help.rs` | Help overlay with shortcuts | ~175 |
| `banner.rs` | Startup banner | ~95 |
| `session_stats.rs` | Exit statistics | ~175 |
| `audio_meter/` | Visual audio level meters | ~270 |
| `progress.rs` | Progress bars and spinners | ~250 |

## Alternatives Considered

- **Single monolithic styling module**: Rejected; harder to test and maintain
- **External crate (colored, owo-colors)**: Rejected; adds dependency for simple task
- **CSS-like styling system**: Over-engineered for terminal output
- **No themes, just hardcoded colors**: Less flexible; users can't customize

## Links

- [NO_COLOR Standard](https://no-color.org/)
- [Catppuccin Theme](https://github.com/catppuccin/catppuccin)
- `src/src/bin/voiceterm/` - Implementation location
