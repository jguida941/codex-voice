# Voice HUD UI Enhancement Plan

Comprehensive visual/UI design document for the Voice HUD project (working title). This consolidates all research, completed work, and future plans.

> **Note**: This project is transitioning from "Codex Voice" to a universal voice HUD for AI CLI tools. See [Project Identity](#project-identity--positioning) for naming discussion.

## Contents

- [**Project Identity & Positioning**](#project-identity--positioning) (NEW)
- [**Competitive Landscape**](#competitive-landscape) (NEW)
- [**Widget Library Stack**](#widget-library-stack) (NEW)
- [**Icon Vocabulary**](#icon-vocabulary) (NEW)
- [**HUD Module System**](#hud-module-system) (NEW)
- [Research Summary](#research-summary-jan-2026)
- [Current State](#current-state)
- [Completed Implementations](#completed-implementations)
- [Professional Polish Guidelines](#professional-polish-guidelines)
- [Future Phases](#future-phases) (Phases -1 to 9.5)
- [Micro-Interactions](#micro-interactions)
- [Accessibility & Compatibility](#accessibility--compatibility)
- [Testing Strategy](#testing-strategy)
- [Implementation Roadmap](#implementation-roadmap)
- [Phase Execution Checklist (Claude)](#phase-execution-checklist-claude)
- [**Future Vision: Beyond Whisper**](#future-vision-beyond-whisper) (Phases 10-20)
  - [Visual Features](#visual-features-from-industry-leaders) (Markdown, Syntax, Graphics, Multi-Panel)
  - [AI & LLM Extensions](#ai--llm-extensions) (MCP, Multi-Model, Local-First)
  - [Ambient HUD Features](#ambient-hud-features) (Notifications, Analytics, Macros, TTS)
  - [Integration Ecosystem](#integration-ecosystem) (IDE, Warp-Style)
- [Research & Resources](#research--resources)
- [File Locations](#file-locations)

---

## Project Identity & Positioning

### The Problem with "Codex Voice"

The current name ties the project to OpenAI's Codex CLI specifically. The AI CLI landscape has exploded:

| Tool | Vendor | GitHub Stars |
|------|--------|--------------|
| [Claude Code](https://github.com/anthropics/claude-code) | Anthropic | Official CLI |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli) | Google | Open source |
| [OpenCode](https://github.com/opencode-ai/opencode) | Community | 70K+ (fast growth) |
| [Aider](https://aider.chat/) | Community | Popular, has voice |
| Codex CLI | OpenAI | Original target |

**The project should be AI-agnostic from Day 1.**

### Name Candidates

| Name | Rationale | Domain/Availability |
|------|-----------|---------------------|
| **VoxDev** | Voice + Dev, short, memorable | TBD |
| **VoiceHUD** | Describes exactly what it is | TBD |
| **SpeakCode** | Direct, action-oriented | TBD |
| **Murmur** | Subtle ambient overlay vibe | TBD |
| **Utterance** | Technical term for speech unit | TBD |
| **VoxTerm** | Voice + Terminal | TBD |
| **DevWhisper** | Plays on Whisper STT | TBD |
| **Speakr** | Modern, short | TBD |

**Decision**: TBD (requires team input)

### Core Value Proposition

> **[Project Name]**: The open-source voice HUD for AI coding assistants.
>
> Talk to Claude Code, Gemini CLI, Aider, or any AI CLI. See your tokens, costs, and transcript queue in a beautiful terminal overlay. 100% local Whisper STT — your voice never leaves your machine.

### Multi-Backend Architecture

The application should support multiple AI CLI backends:

```rust
/// Supported AI CLI backends
pub enum AiBackend {
    ClaudeCode,      // anthropic/claude-code
    GeminiCli,       // google-gemini/gemini-cli
    Aider,           // paul-gauthier/aider
    OpenCode,        // opencode-ai/opencode
    CodexCli,        // openai/codex (legacy)
    Custom(String),  // Any command: "my-custom-ai-tool"
}

impl AiBackend {
    /// Command to spawn for this backend
    pub fn command(&self) -> &str {
        match self {
            Self::ClaudeCode => "claude",
            Self::GeminiCli => "gemini",
            Self::Aider => "aider",
            Self::OpenCode => "opencode",
            Self::CodexCli => "codex",
            Self::Custom(cmd) => cmd,
        }
    }

    /// Prompt detection pattern (regex for "ready for input")
    pub fn prompt_pattern(&self) -> &str {
        match self {
            Self::ClaudeCode => r"^>",
            Self::GeminiCli => r"^>",
            Self::Aider => r"^>",
            // ... backend-specific patterns
            _ => r"^[>$#]",
        }
    }
}
```

**CLI Flag**:
```bash
voice-hud --backend codex          # Default
voice-hud --backend gemini
voice-hud --backend aider
voice-hud --backend "my-custom-tool --flag"
```

---

## Competitive Landscape

### Voice Input Tools for AI CLIs

| Tool | Approach | Strengths | Gaps We Fill |
|------|----------|-----------|--------------|
| [VoiceMode MCP](https://getvoicemode.com/) | MCP server for Claude | Natural conversations, TTS | No HUD, no visual feedback, Claude-only |
| [Listen-Claude-Code](https://github.com/gmoqa/listen-claude-code) | Simple CLI wrapper | Lightweight | No overlay, minimal UX, Claude-only |
| [Wispr Flow](https://wisprflow.ai/) | Premium dictation | Polish, context-aware | Paid ($$$), not open source |
| [Super Whisper](https://superwhisper.com/) | Mac app | Fast, local | Mac-only, paid, no HUD |
| [ccstatusline](https://github.com/sirmalloc/ccstatusline) | Status bar for Claude | Tokens, cost, themes | No voice, status-only |
| [OpenWhispr](https://github.com/HeroTools/open-whispr) | Desktop dictation | Open source, local | No AI CLI integration |

### Our Differentiators

1. **Full Visual HUD** — not just a status line, a complete overlay system
2. **Multi-Backend** — works with any AI CLI, not locked to one vendor
3. **Local-First** — Whisper runs locally, voice never leaves machine
4. **Transcript Queue** — speak while AI is busy, queue manages flow
5. **Open Source** — free, extensible, community-driven
6. **Rust Performance** — ~250ms transcription latency

---

## Widget Library Stack

### Recommended Libraries

| Library | Purpose | Priority |
|---------|---------|----------|
| **[rat-widget](https://crates.io/crates/rat-widget)** | Buttons, toggles, checkboxes, sliders, text-input, menubar, status-bar. **Built-in focus + event handling.** | Primary |
| **[ratatui-interact](https://docs.rs/ratatui-interact)** | Buttons with icons, block style, toggles, checkboxes. **Focus + mouse click support.** | Secondary |
| **[tui-widgets](https://github.com/ratatui/tui-widgets)** | Official ratatui collection — popups, prompts, scrollviews, dialog boxes | Supplemental |
| **[tui-textarea](https://crates.io/crates/tui-textarea)** | Multi-line text editor with undo/redo, search | For transcript editing |
| **[tui-input](https://crates.io/crates/tui-input)** | Single-line text input | For filters, search |
| **[tui-slider](https://crates.io/crates/tui-slider)** | Horizontal/vertical sliders | For sensitivity |

### Widget Specifications

#### Buttons

```
Standard:     ┌──────────┐
              │  Apply   │
              └──────────┘

Focused:      ┌──────────┐
              │ ▸ Apply  │  (or inverse video)
              └──────────┘

Disabled:     ┌──────────┐
              │  Apply   │  (dimmed color)
              └──────────┘
```

#### Toggles / Checkboxes

```
Checkbox (Unicode):    [●] Enabled     [ ] Disabled
Checkbox (ASCII):      [x] Enabled     [ ] Disabled

Toggle (pill style):   ━━●━━  ON       ●━━━━  OFF
```

#### Radio Buttons

```
( ) Manual Mode    (●) Auto Mode    ( ) Confirm Mode
```

#### Sliders

```
Horizontal:   ━━━━━━●━━━━━━━━━━━━  -35dB
              ├────────────────────┤
              -60               -20

Compact:      ────●──────  -35dB
```

#### Settings Overlay Example

```
╭─ Settings ───────────────────────────────────────────────╮
│                                                          │
│   [●] Auto-voice              ( ) Manual  (●) Auto-send  │
│                                                          │
│   Sensitivity   ━━━━━━●━━━━━━━━━━━━━━  -35dB             │
│                                                          │
│   [●] Animations              [ ] Unicode                │
│                                                          │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│   │  Apply   │  │  Reset   │  │  Cancel  │              │
│   └──────────┘  └──────────┘  └──────────┘              │
│                                                          │
╰─ Tab next  ↑↓ navigate  ←→ adjust  Enter select ────────╯
```

---

## Icon Vocabulary

**Rule**: No emojis. Unicode geometric shapes and box drawing only. All icons must render consistently across terminals (iTerm2, Kitty, WezTerm, Terminal.app, Windows Terminal, VS Code integrated terminal).

### State Indicators

| State | Unicode | ASCII Fallback | Color |
|-------|---------|----------------|-------|
| Recording | `●` | `*` | Red |
| Idle | `○` | `-` | Dim |
| Processing | `◐ ◑ ◒ ◓` (animated) | `[...]` | Yellow |
| Success | `✓` | `ok` | Green |
| Error | `✗` | `err` | Red |
| Warning | `⚠` | `!` | Yellow |
| Info | `ℹ` | `i` | Blue |

### HUD Elements

| Element | Unicode | ASCII Fallback | Notes |
|---------|---------|----------------|-------|
| Audio level | `▸` or `◂` | `>` | Direction indicator |
| Time/Latency | `◷` | `t:` | Clock face |
| Tokens/Context | `≡` or `⧗` | `ctx:` | Stack/hourglass |
| Cost | `$` | `$` | Universal |
| Queue depth | `▤` | `Q:` | Stacked lines |
| Network OK | `◆` | `+` | Filled diamond |
| Network down | `◇` | `x` | Empty diamond |
| Git branch | `⎇` | text only | Branch symbol |
| Meter bars | `▁▂▃▄▅▆▇█` | `[====  ]` | Block elements |
| Separator | `│` | `|` | Box drawing |
| Selection marker | `▶` or `▸` | `>` | Triangle |

### Animation Frames

```rust
// Processing spinner (Braille)
const SPINNER_BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// Processing spinner (Quarter circle)
const SPINNER_CIRCLE: &[char] = &['◐', '◓', '◑', '◒'];

// Processing spinner (ASCII fallback)
const SPINNER_ASCII: &[&str] = &["[.  ]", "[.. ]", "[...]", "[ ..]", "[  .]", "[   ]"];
```

### Theme-Specific Indicators

Each theme can override default indicators while maintaining semantic meaning:

| Theme | Recording | Processing | Success | Idle |
|-------|-----------|------------|---------|------|
| Coral | `●` | `◐` | `✓` | `○` |
| Catppuccin | `◉` | `◈` | `✓` | `◇` |
| Dracula | `⬤` | `⏺` | `✓` | `○` |
| Nord | `◆` | `▸` | `✓` | `◇` |
| High-Contrast | `●` (inverse) | `◐` (inverse) | `✓` | `○` |
| ANSI | `*` | `@` | `ok` | `-` |

---

## HUD Module System

The HUD is composed of pluggable modules. Each module renders a fixed-width segment and can be enabled/disabled via config.

### Module Architecture

```rust
/// A HUD module renders a fixed-width segment
pub trait HudModule {
    /// Unique identifier
    fn id(&self) -> &'static str;

    /// Render the module content (fixed width)
    fn render(&self, state: &AppState, width: usize) -> String;

    /// Minimum width required
    fn min_width(&self) -> usize;

    /// Update frequency (Some = tick-based, None = event-based)
    fn tick_interval(&self) -> Option<Duration>;
}
```

### Standard Modules

| Module | ID | Width | Content | Example | Default |
|--------|-----|-------|---------|---------|---------|
| **Mode** | `mode` | 8 | Recording state + mode | `● AUTO` | On |
| **Audio Meter** | `meter` | 12 | dB level + waveform | `-40dB ▁▂▃▅▆` | On |
| **Model** | `model` | 12 | Active AI backend | `claude` | On |
| **Latency** | `latency` | 8 | Last response time | `◷ 1.2s` | On |
| **Queue** | `queue` | 6 | Pending transcripts | `Q: 2` | On |
| **Duration** | `duration` | 8 | Recording duration | `2.5s` | On |
| **Network** | `network` | 3 | Connection status | `◆` or `◇` | Off |
| **Git** | `git` | 12 | Branch + status | `main ↑2` | Off |
| **Tokens** | `tokens` | 12 | Context usage (API) | `≡ 12K/100K` | Off |
| **Cost** | `cost` | 8 | Session cost (API) | `$ 0.23` | Off |

> **Note**: Token and cost modules require API integration and are disabled by default. Enable them if you're using API-based backends that expose usage metrics.

### Module Display Rules (Professional UX)

- Render nothing when data is unavailable (no placeholder `--` noise).
- Degrade gracefully at narrow widths (indicator-only before disappearing).
- Never emit ANSI or newlines from a module; rendering stays pure and width-bounded.

### HUD Layout Zones

```
╭─── [Project Name] ─ [Model] ─────────────────────────────────────────╮
│ [LEFT ZONE]           │ [CENTER ZONE]          │ [RIGHT ZONE]        │
│ Mode + Status         │ Message / Pipeline     │ Metrics             │
╰──────────────────────────────────────────────────────────────────────╯
```

**Zone Assignments**:
- **Left**: `mode`, `network`, `queue`
- **Center**: Status message, pipeline stage, transcript preview
- **Right**: `meter`, `latency`, `tokens`, `cost`, `duration`

### Full HUD Example (80+ cols)

```
╭─── VoxDev ─ claude ─────────────────────────────────────────────────╮
│ ● AUTO │ Transcribing [2/4]...             │ -40dB ▁▂▃▅▆  ◷ 1.2s Q:0│
│ ^R rec  ^V auto  : cmd  ? help                                      │
╰─────────────────────────────────────────────────────────────────────╯
```

### Compact HUD (60 cols)

```
╭─── VoxDev ─ claude ────────────────────────╮
│ ● AUTO │ Transcribing...   │ -40dB  ◷ 1.2s│
│ ^R rec  ^V auto  ? help                    │
╰────────────────────────────────────────────╯
```

### Minimal HUD (40 cols)

```
╭─ VoxDev ──────────────────────╮
│ ● AUTO  -40dB ▁▂▃▅  ◷ 1.2s   │
│ ^R  ^V  ? help                │
╰───────────────────────────────╯
```

### Module Configuration

```toml
# ~/.config/voice-hud/preferences.toml

[hud.modules]
# Core modules (on by default)
mode = true
meter = true
model = true
latency = true
queue = true
duration = true

# Optional modules (off by default)
network = false   # Connection status indicator
git = false       # Git branch + status
tokens = false    # API token usage (requires API integration)
cost = false      # Session cost (requires API integration)

[hud.layout]
left = ["mode", "queue"]
center = "message"  # Status message / pipeline stage
right = ["meter", "latency", "duration"]
```

---

## Research Summary (Jan 2026)

Key findings from industry research on TUI best practices:

### TUI Design Patterns
- **Model-View-Update (MVU)** pattern for state management (used by Bubbletea, Textual)
- Reactive attributes for dynamic state changes
- CSS-like styling systems for theming
- Rich widget libraries with focus management built-in

### Ratatui Best Practices

#### Core Libraries
- `ratatui-widgets`: official split-out of built-in widgets (Ratatui re-exports them; most apps don't need a direct dependency)
- Component pattern (like GoBang, EDMA) for inter-component communication

#### Interactive Widget Libraries (Recommended Stack)

| Library | Components | Event Handling | Focus | Mouse |
|---------|------------|----------------|-------|-------|
| **[rat-widget](https://crates.io/crates/rat-widget)** | Buttons, toggles, checkboxes, radio, sliders, text-input, date-input, calendar, menubar, status-bar, file-dialog | Built-in | Built-in | Yes |
| **[ratatui-interact](https://docs.rs/ratatui-interact)** | Buttons (icon, block, toggle), checkboxes, text fields | Built-in | Tab navigation | Click regions |
| **[tui-widgets](https://github.com/ratatui/tui-widgets)** | Popups, prompts, scrollviews, dialog boxes | Manual | Manual | Manual |
| **[rat-focus](https://crates.io/crates/rat-focus)** | Focus model only | N/A | Ordered list | N/A |

#### Specialized Widgets

| Library | Purpose | Notes |
|---------|---------|-------|
| [tui-textarea](https://crates.io/crates/tui-textarea) | Multi-line text editor | Undo/redo, search, vim-like |
| [tui-input](https://crates.io/crates/tui-input) | Single-line input | Headless, cursor support |
| [tui-slider](https://crates.io/crates/tui-slider) | Sliders | Horizontal/vertical |
| [tui-prompts](https://lib.rs/crates/tui-prompts) | Interactive prompts | Confirmation dialogs |
| [ratatui-image](https://crates.io/crates/ratatui-image) | Image rendering | Sixel, unicode-halfblocks |

#### Frameworks (Full Component Systems)

| Library | Style | Best For |
|---------|-------|----------|
| [rat-salsa](https://crates.io/crates/rat-salsa) | Event queue + tasks + timers + dialogs | Complex async apps |
| [tui-realm](https://crates.io/crates/tui-realm) | Elm/React-inspired | Component architecture |
| [widgetui](https://crates.io/crates/widgetui) | Bevy-like ECS | Game-style UI |

**Recommendation**: Use `rat-widget` as the primary widget library. It provides the most complete set of interactive components with built-in focus and event handling, which aligns with our Phase 1-2 goals (Focus & SelectableMenu).

### Design Tokens & Accessibility
- Three-tier hierarchy: **Global → Alias → Component** tokens
- WCAG contrast: colors 500 and below for light bg, 600+ for dark bg
- Semantic naming (`color-primary` vs hardcoded hex)
- High-contrast themes as accessibility requirement

**Sources**: [awesome-tuis](https://github.com/rothgar/awesome-tuis), [Textual](https://realpython.com/python-textual/), [ratatui discussions](https://github.com/ratatui/ratatui/discussions/220), [WCAG design tokens](https://www.w3.org/2023/09/13-inclusive-design-tokens-minutes.html), [design.dev](https://design.dev/guides/design-systems/)

### Industry Tool Analysis

#### AI CLI Tools

| Tool | Framework | Key Visual Features | Takeaways |
|------|-----------|---------------------|-----------|
| [Claude Code](https://github.com/anthropics/claude-code) | React + Ink | Custom renderer, theme-driven, spinners, progress bars | Theme context pattern, incremental updates |
| [OpenCode](https://github.com/opencode-ai/opencode) | Go | TUI, multi-model support, 70K+ stars | Fast-growing competitor, Go-based |
| [Aider](https://aider.chat/) | Python | Voice input, multi-model, git integration | Has voice, strong community |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli) | TypeScript | Agent mode, open source | Google backing, growing adoption |

#### TUI Frameworks & Dashboards

| Tool | Framework | Key Visual Features | Takeaways |
|------|-----------|---------------------|-----------|
| [Textual](https://textual.textualize.io/) | Python | DataTable, Markdown streaming, syntax highlighting, CSS styling | Rich widget library, reactive attributes |
| [Lazygit](https://jesseduffield.com/Lazygit-5-Years-On/) | Go + gocui | Multi-panel layout, real-time stats, vim-style navigation | Visual context aids understanding |
| [Warp](https://www.warp.dev/) | Rust | AI suggestions, block-based UI, modern editing | Ask AI feature, error explanations |
| [DevDash](https://github.com/Phantas0s/devdash) | Go | Configurable widgets, YAML config, SSH support | Dashboard inspiration |
| [ccstatusline](https://github.com/sirmalloc/ccstatusline) | Shell | Claude Code status bar, tokens, cost, themes | Direct competitor for status HUD |

#### Voice Input Tools

| Tool | Approach | Key Features | Gaps |
|------|----------|--------------|------|
| [VoiceMode MCP](https://getvoicemode.com/) | MCP server | Natural conversations, local Whisper option, TTS | No HUD, Claude-only |
| [Wispr Flow](https://wisprflow.ai/) | Premium app | Context-aware, syntax understanding, polished | Paid, closed source |
| [Super Whisper](https://superwhisper.com/) | Mac app | Fast local STT, coding-optimized | Mac-only, paid |
| [OpenWhispr](https://github.com/HeroTools/open-whispr) | Desktop app | Open source, local/cloud options | No AI CLI integration |
| [agent-cli](https://github.com/basnijholt/agent-cli) | Python CLI | Voice hotkey, local ASR/TTS, multi-platform | Different architecture |

### MCP & AI Integration Landscape (2025-2026)

The [Model Context Protocol (MCP)](https://www.anthropic.com/news/model-context-protocol) has become the industry standard for AI tool integration:

- **Adopted by**: Anthropic, OpenAI, Google, Microsoft, AWS
- **Supported by**: Cursor, Figma, Replit, Sourcegraph, Zapier, Claude Desktop
- **2026 Roadmap**: Agent-to-agent communication, multi-modal (images, video, audio)
- **Governance**: [Linux Foundation Agentic AI Foundation (AAIF)](https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation)

---

## Current State

### UI Architecture

Codex Voice has **two UI modes**:

| Mode | Framework | Visual Style |
|------|-----------|--------------|
| **Overlay** (`codex-voice`) | Raw ANSI + Crossterm | Multi-row status banner at bottom, themed colors |
| **Full TUI** (`rust_tui`) | Ratatui + Crossterm | 3-panel layout, red theme, rounded borders |

### Current Visual Components

#### 1. Multi-Row Status Banner (`status_line.rs`)
```
╭─── coral ──────────────────────────────────────────────────────╮
│ ● AUTO │ Rust │ -40dB  ▁▂▃▅▆▇█  -51dB  Listening Auto Mode    │
│ ^R rec  ^V auto  ^T send  ? help  ^Y theme                    │
╰────────────────────────────────────────────────────────────────╯
```

#### 2. Theme System (`theme.rs`)

| Theme | Border Style | Indicators | Background |
|-------|--------------|------------|------------|
| **Coral** | `─│┌┐└┘` single | ● ◉ ● ○ | Transparent |
| **Catppuccin** | `═║╔╗╚╝` double | ◉ ◈ ◆ ◇ | #1e1e2e |
| **Dracula** | `━┃┏┓┗┛` heavy | ⬤ ⏺ ⏵ ○ | #282a36 |
| **Nord** | `─│╭╮╰╯` rounded | ◆ ❄ ▸ ◇ | #2e3440 |
| **Ansi** | `─│┌┐└┘` single | * @ > - | Black |
| **None** | `─│┌┐└┘` single | * @ > - | None |

#### 3. Status Message Styling (`status_style.rs`)

| State | Indicator | Color |
|-------|-----------|-------|
| Recording | `● REC` | Red |
| Processing | `◐` (animated) | Yellow |
| Success | `✓` | Green |
| Warning | `⚠` | Yellow |
| Error | `✗` | Red |
| Info | `ℹ` | Blue |

#### 4. Help Overlay (`help.rs`)
```
┌─────────────────────────────────────┐
│  Codex Voice - Keyboard Shortcuts   │
├─────────────────────────────────────┤
│  Ctrl+R   Start voice capture       │
│  Ctrl+V   Toggle auto-voice         │
│  Ctrl+T   Toggle send mode          │
│  ...                                │
├─────────────────────────────────────┤
│  Press any key to close             │
└─────────────────────────────────────┘
```

#### 5. Theme Picker (`theme_picker.rs`)
- Shows visual preview of each theme's indicator
- Marks current theme with ▶
- Uses theme-specific borders
- Press 1-6 to select (numbers only, no arrow keys yet)

#### 6. Startup Banner (`banner.rs`)
- Simple text line: `Codex Voice v1.0.28 │ Rust │ theme: coral │ auto-voice: on │ -35dB`
- Minimal version for narrow terminals

---

## Completed Implementations

### Tier 0 - Quick Wins ✅

| Task | Status | Notes |
|------|--------|-------|
| ANSI colors to overlay status line | ✅ Done | `status_style.rs`, `writer.rs` |
| Modern Braille spinner | ✅ Done | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` |
| Unicode state indicators | ✅ Done | ●, ✓, ✗, ⚠, ◐, ℹ |

### Tier 1 - Core Visual System ✅

| Task | Status | Notes |
|------|--------|-------|
| Theme struct (6 themes) | ✅ Done | `theme.rs` |
| `--theme` CLI flag | ✅ Done | `config.rs` |
| `--no-color` flag | ✅ Done | Respects `NO_COLOR` env |
| Color mode detection | ✅ Done | TrueColor/256/ANSI auto-detect |
| Theme-specific borders | ✅ Done | Single/double/heavy/rounded |
| Theme-specific indicators | ✅ Done | Unique per theme |
| Background colors | ✅ Done | TrueColor themes have bg |

### Tier 2 - Enhanced Status Line ✅

| Task | Status | Notes |
|------|--------|-------|
| Multi-row banner layout | ✅ Done | 4 lines with borders |
| Keyboard shortcuts display | ✅ Done | Bottom row of banner |
| Live recording duration | ✅ Done | Shows "2.5s" |
| Pipeline indicator | ✅ Done | "Rust" or "Python" |
| Sensitivity display | ✅ Done | "-40dB" |
| Live waveform meter | ✅ Done | `▁▂▃▅▆▇█` during recording |
| Responsive breakpoints | ✅ Done | 80/60/40/25 column widths |

### Tier 3 - Help and Discoverability ✅

| Task | Status | Notes |
|------|--------|-------|
| Help overlay (`?` key) | ✅ Done | `help.rs` |
| Startup banner | ✅ Done | `banner.rs` (simple version) |
| Session stats on exit | ✅ Done | `session_stats.rs` |
| Theme picker overlay | ✅ Done | `theme_picker.rs` |

### Tier 4 - Advanced Features (Partial)

| Task | Status | Notes |
|------|--------|-------|
| Visual mic meter | ✅ Done | `audio_meter.rs` |
| Real-time audio level | ⏭️ Skipped | Complex, lower priority |
| Syntax highlighting | ⏭️ Skipped | High effort |
| Progress bar for downloads | ⏭️ Skipped | Not needed yet |

### Tier 5 - Polish (Partial)

| Task | Status | Notes |
|------|--------|-------|
| Responsive narrow-terminal | ✅ Done | Graceful degradation |
| ANSI-only fallback | ✅ Done | `Theme::Ansi` |
| Transcript history panel | ⏭️ Skipped | TUI-only |
| Notification sounds | ⏭️ Skipped | Low priority |

---

## Professional Polish Guidelines

### A. Design Token Layer (Beyond Basic Themes)

Upgrade from basic palette colors to semantic UI tokens:

```rust
    pub struct ThemeColors {
    // === EXISTING ===
    pub recording: &'static str,
    pub processing: &'static str,
    pub success: &'static str,
    pub warning: &'static str,
    pub error: &'static str,
    pub info: &'static str,

    // === TEXT HIERARCHY ===
    pub text_primary: &'static str,   // Main content
    pub text_muted: &'static str,     // Hints, shortcuts, secondary
    pub text_accent: &'static str,    // Highlighted/emphasized

    // === SURFACES ===
    pub surface: &'static str,        // Panel background
    pub surface_alt: &'static str,    // Alternating rows, hover

    // === INTERACTIVE STATES ===
    pub selection_bg: &'static str,   // Selected item background
    pub selection_fg: &'static str,   // Selected item text
    pub focus_border: &'static str,   // Focused panel border
    pub disabled: &'static str,       // Disabled controls

    // === METER GRADIENT ===
    pub meter_low: &'static str,      // Green zone (safe)
    pub meter_mid: &'static str,      // Yellow zone (loud)
    pub meter_high: &'static str,     // Red zone (clipping)

    // === LOGO ===
    pub logo: &'static str,           // ASCII art logo color
}
```

**Implementation detail (important)**:
- Store colors as data (e.g., `Rgb(u8,u8,u8)` + 256/ANSI fallbacks), not raw
  escape strings. Compute escape sequences after capability detection.
- This enables real contrast checks and consistent fallback behavior.

**Why**: Stop hardcoding "yellow here, green there" — render "warning style" consistently everywhere.

### B. Fixed-Width Formatting (Anti-Jank)

Values that change every frame should have **fixed width** to prevent UI "wiggle":

| Element | Format | Example |
|---------|--------|---------|
| dB level | 5 chars, right-aligned | ` -40dB`, ` -5dB` |
| Duration | Fixed width | `  2.5s`, ` 12.3s` |
| Waveform | Constant width per breakpoint | 8 chars at 80+, 6 at 60+ |
| Percentage | 4 chars | ` 42%`, `100%` |

### C. Consistent Spacing & Alignment Rules

| Rule | Convention |
|------|------------|
| Titles | Title Case |
| Inner padding | Always 1 cell |
| Section separators | ` │ ` (space-pipe-space) |
| Column alignment | Left-align labels, right-align values |
| Shortcut format | `^R` or `Ctrl+R` (pick one, be consistent) |

### D. Color Intensity Hierarchy

Use **muted colors for background info**, accent only for state changes:

| Element | Intensity | Token |
|---------|-----------|-------|
| Shortcuts row | Muted | `text_muted` |
| Status message | Normal | `text_primary` |
| Mode indicator | Accent | `text_accent` or semantic |
| Errors/Warnings | Semantic | `error`, `warning` |
| Borders (unfocused) | Muted | `border` |
| Borders (focused) | Accent | `focus_border` |

### E. Interaction Spec + Action Registry (Anti-Drift)

Define a single action registry that powers:
- Keybindings (overlay + TUI)
- Help overlay labels
- Command palette entries
- Settings toggles (where applicable)

Each action should include: `id`, `label`, `default_key`, `scope` (global/modal),
and `enabled_when` rules. This keeps shortcuts and UI labels consistent.

### F. Render Guarantees (No Flicker, No Jank)

Guarantee the overlay feels stable:
- Dirty-region redraws (only redraw on changes)
- Frame rate cap (10–15 FPS for animations)
- Crash-safe terminal restore (panic hook)
- Fixed-width formatting for live values

### G. Modular UI Architecture (Scalable by Default)

Keep the overlay and TUI from diverging by enforcing a shared component contract:

- **Unidirectional flow**: `Event -> Action -> State -> Render`
- **Headless ui_core**: shared `AppState`, `Action`, `ActionRegistry`, `Keybindings`,
  `Reducer`, `DirtyFlags`, and `Announcements` (no I/O)
- **Two thin shells**: Overlay shell + Ratatui shell both map events -> Actions -> Reducer -> Render
- **Pure renderers**: render functions take state + width and return lines/widgets
- **Thin IO shells**: input reading, PTY, and writer threads stay separate from UI logic
- **Component boundaries**: each overlay implements `render()`, `handle_event()`, and `focus()`
- **Shared primitives**: SelectableMenu, FocusManager, ActionRegistry live in reusable modules
- **No hidden global state**: all UI state should be owned by explicit structs
- **Backend-agnostic hooks**: avoid Codex-only assumptions in UI state and metrics

**Overlay stack (not just focus):**
- `overlay_stack: Vec<OverlayKind>` controls z-order and input routing
- `overlay_state: HashMap<OverlayKind, OverlayState>` stores per-overlay state
- Focus stack restores the correct target when overlays pop

**Source of truth flow**:
```
Input -> Keybindings -> Action -> Reducer -> State -> Renderer(s)
```

### H. Scalability Guardrails

Rules to keep the UI maintainable as features grow:

- Cap lists (history, recent commands) with fixed max sizes + eviction policy
- Virtualize long lists (render only visible window)
- Centralize spacing + layout constants (no magic numbers in render code)
- Avoid duplication: share theme tokens + layout logic between overlay and TUI
- Version all persisted configs; add migrations + tests for each version
- Keep event handling deterministic (no blocking or async inside render)

### I. Shared UI Primitives Module (Prevent Drift)

Create a shared module for reusable UI pieces so overlay and TUI stay in sync:
- `ui_primitives/` for SelectableMenu, FocusManager, ActionRegistry, layout constants
- Pure render helpers (state + width -> lines) for snapshot tests
- Reused by overlay + ratatui layer to keep behaviors identical

### J. Design Tokens Spec + Automated Contrast Checks

Define a token spec (global → alias → component) and enforce contrast rules:
- Token definitions live in one place and are referenced by both UI modes
- Automated tests verify WCAG AA (and AAA for high-contrast)
- Token changes require snapshots + contrast tests update

### E. Anchored HUD Zones (Left / Center / Right)

Make the banner feel like a HUD by anchoring information so the eye always knows where to look:
- **Left**: identity + mode chips + status icon
- **Center**: primary message / pipeline stage (fixed-width region)
- **Right**: metrics (duration, dB, meter, model/pipeline), right-aligned

Conceptual structure:
```
╭─ Codex Voice ──────────────────────────────────────────────────╮
│ ● REC  AUTO  |  [2/4] Transcribing…        |  02.5s  -40dB ▁▂▃▅ │
│ : commands  ^R rec  ^V auto  ^S settings  ? help               │
╰───────────────────────────────────────────────────────────────╯
```

### F. Progressive Disclosure (Minimal by Default)

Keep the default HUD minimal; reveal depth on demand:
- Details via overlays (help, settings, history, error details)
- Optional debug/perf overlay toggles (off by default)
- Avoid persistent noise unless the user asks for it

### G. HUD Invariants (No-Jank Guarantee)

Formalize layout stability as a contract:
- No segment changes width during a session (except breakpoint changes)
- All numeric fields right-aligned with fixed width
- Meter area fixed width per breakpoint
- Never insert/remove separators mid-run; only swap content
- Center message stays centered and never shifts left/right

### H. Segment Layout Engine + No-Wrap Guarantee

Enforce invariants in code (not convention):
- Define left/center/right segments with fixed max widths per breakpoint
- Fit helpers: `fit_left`, `fit_center`, `fit_right` with display-width truncation
- Single safe render path: sanitize control chars, strip/escape ANSI from user text,
  truncate by display width, then render
- No-wrap guarantee: never print newlines while drawing HUD; always MoveTo + ClearLine + padded content
- Test invariant: every rendered line display width <= terminal width (all breakpoints)

### I. Focus Ring Consistency

Apply focus styling everywhere, including overlay mode:
- Focused element: brighter border or `▌` left marker
- Selected row: reverse video or `selection_bg` + `selection_fg`
- All modals must show focus (no invisible focus state)

---

## Future Phases

### Phase -2: Project Identity & Multi-Backend Architecture (NEW)

**Goal**: Establish universal identity and abstract backend support before feature work.

**Why First**: The name "Codex Voice" limits market positioning. Multi-backend support should be architectural, not bolted on.

#### A. Finalize Project Name

- [ ] Evaluate name candidates (see [Project Identity](#project-identity--positioning))
- [ ] Check domain/trademark availability
- [ ] Update all references (binary name, docs, configs)
- [ ] Design minimal logo/wordmark (ASCII art for banner)

#### B. Abstract AI Backend

```rust
// src/backend/mod.rs
pub trait AiBackend: Send + Sync {
    fn name(&self) -> &str;
    fn command(&self) -> Vec<String>;
    fn prompt_pattern(&self) -> &str;
    fn detect_thinking(&self) -> Option<&str>;  // Pattern for "AI is thinking"
}

pub struct CodexCli;
pub struct ClaudeCode;
pub struct GeminiCli;
pub struct Aider;
pub struct OpenCode;
pub struct Custom { command: String }
```

#### C. CLI Flag for Backend Selection

```bash
voice-hud                           # Default: codex
voice-hud --backend gemini
voice-hud --backend aider
voice-hud --backend "custom-tool --flag"
```

#### D. Backend-Specific Prompt Detection

Each backend has different prompt patterns:
- Codex CLI: auto-learn (no default regex)
- Claude Code: `>`
- Gemini CLI: `>`
- Aider: `>`
- Custom: configurable regex

#### E. Update Config Schema

```toml
# ~/.config/voice-hud/preferences.toml
[backend]
default = "codex"  # codex | claude | gemini | aider | opencode | custom
custom_command = "" # Used when default = "custom"
prompt_pattern = "" # Optional override
```

**Files**: NEW `src/backend/mod.rs`, `src/backend/claude.rs`, `src/backend/gemini.rs`, etc.

**Deliverables**:
- [ ] `AiBackend` trait defined
- [ ] 4+ backend implementations (Claude, Gemini, Aider, OpenCode)
- [ ] `--backend` CLI flag
- [ ] Config file backend selection
- [ ] All docs updated with new name

---

### Phase -1: Reliability & Terminal Safety Foundation (Pre-Phase)

**Goal**: Make the overlay safe, debuggable, and deterministic before adding more UI features.

**Why First**: “UI polish” is only shippable if the terminal never gets corrupted and failures are diagnosable.

#### A. Panic-Safe Terminal Restore

- Always restore raw mode, cursor, alt screen, colors on exit **and** panic
- Add a Drop guard for terminal state
- Install a panic hook that restores terminal + writes crash log

#### B. Structured Logging + Crash Logs

- Optional `--log-file` for structured logs
- On crash, write last N events + state summary to a crash log

#### C. `codex-voice doctor` Command

Prints detected capabilities and system info:
- Terminal: color depth, unicode support, mouse, graphics protocol
- Config paths + active config files
- Audio device info + selected device

#### D. Deterministic Render Rule

- Rendering must be pure: `state -> Vec<String>`
- No side effects inside `render()` (no I/O, no state mutation)
- Side effects only in the event handler

#### E. Privacy + Sanitization

- Redaction policy default-on for persisted data (logs/history).
- Crash logs store metadata only unless explicitly enabled.
- History retention modes: full / truncated / none.
- Strip ESC/control chars before rendering or persistence.
- Create files with restrictive permissions (best effort).

#### F. Terminal Coexistence Contract

- Always save/restore cursor during banner draw.
- Never overwrite partially typed input.
- Write only inside reserved rows.
- On resize: recompute reserved rows, clear old banner region, redraw full frame.
- Enable mouse only while a modal is open.

**Files**: NEW `terminal_restore.rs`, `crash_log.rs`, `doctor.rs`, `logging.rs`

### Phase 0: Accessibility Foundation (NEW - Critical)

**Goal**: Bake accessibility into the foundation before building features

**Why First**: Retrofitting accessibility is expensive. Building it in from the start ensures all future components inherit these patterns.

#### A. High-Contrast Theme

Add `Theme::HighContrast` with maximum visibility:

```rust
pub const HIGH_CONTRAST: ThemeColors = ThemeColors {
    recording: "\x1b[97;41m",      // White on red bg
    processing: "\x1b[30;43m",     // Black on yellow bg
    success: "\x1b[97;42m",        // White on green bg
    warning: "\x1b[30;43m",        // Black on yellow bg
    error: "\x1b[97;41m",          // White on red bg
    info: "\x1b[97;44m",           // White on blue bg
    text_primary: "\x1b[97m",      // Bright white
    text_muted: "\x1b[37m",        // White (not dim)
    border: "\x1b[97m",            // Bright white borders
    bg_primary: "\x1b[40m",        // Pure black bg
    // All contrast ratios >= 7:1 (WCAG AAA)
};
```

#### B. Reduced Motion Flag

```bash
codex-voice --reduced-motion   # or --no-anim
```

When enabled:
- Spinner shows static `[...]` instead of animation
- Meter shows static bar instead of live waveform
- No blinking cursors
- Respect `REDUCE_MOTION` environment variable

#### C. State Announcements (Screen Reader Support)

Add announcement hooks for state changes:

```rust
pub struct Announcement {
    pub message: String,
    pub priority: AnnouncePriority,  // Polite, Assertive
}

// Example usage
fn on_recording_start() {
    announce("Recording started", AnnouncePriority::Assertive);
}

fn on_transcript_ready(text: &str) {
    announce(&format!("Transcript ready: {}", text), AnnouncePriority::Polite);
}
```

For terminals that support it, output to stderr with specific formatting that screen readers can parse.

#### D. Minimum Contrast Validation

Add compile-time or startup validation:

```rust
impl ThemeColors {
    pub fn validate_contrast(&self) -> Result<(), Vec<ContrastError>> {
        // Check all fg/bg pairs meet WCAG AA (4.5:1) minimum
        // Warn if any pair is below AAA (7:1)
    }
}
```

#### E. CLI Flags (Elevate Priority)

| Flag | Purpose | Env Var |
|------|---------|---------|
| `--high-contrast` | Maximum contrast colors | `HIGH_CONTRAST=1` |
| `--reduced-motion` | Disable animations | `REDUCE_MOTION=1` |
| `--no-unicode` | ASCII-only indicators | `NO_UNICODE=1` |
| `--announce` | Enable state announcements | `ANNOUNCE=1` |

**Files**: `theme.rs`, `config.rs`, `main.rs`, NEW `accessibility.rs`

---

### Phase 0.5: Performance Architecture (NEW)

**Goal**: Make sub-millisecond rendering and low flicker consistently true.

**Core Mechanisms**:
- **Dirty-line rendering**: keep previous frame, only rewrite changed lines
- **Render budget + instrumentation**: track `render_time_us`, `flush_time_us`, `frame_drops`
- **Optional perf overlay**: `--debug-perf` shows `r=0.42ms f=0.18ms fps=14`
- **Slow render warnings**: `WARN slow_render render=3.8ms width=80 mode=banner`
- **Event-driven redraw**: render on state change; ticks only for animations
- **Tick throttling**: cap at 10–15 FPS; if `--reduced-motion`, no tick loop
- **Update coalescing**: collapse high-frequency signals (meter/timers) to latest value per tick
- **Metrics foundation**: collect latency, queue depth, model identity (if available),
  and usage/cost metadata only when the backend provides it (opt-in, no PII by default)

**Files**: `writer.rs`, NEW `render_diff.rs`, `perf_metrics.rs`, `perf_overlay.rs`, `hud_metrics.rs`

---

### Phase 1: Focus & Selection Model (Foundation)

**Goal**: Standardize keyboard interaction across all overlays

**Universal Conventions**:
| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection |
| `Enter` | Select / Confirm |
| `Esc` | Close overlay / Cancel |
| `Home` / `End` | Jump to first/last |
| `PgUp` / `PgDn` | Page through long lists |
| `Tab` / `Shift+Tab` | Change focus between panes (TUI) |
| `/` or `Ctrl+F` | Search/filter within list |

**Visual Focus Indicators**:
- Focused panel: border uses `focus_border` color
- Selected row: reverse video or `selection_bg` + `selection_fg`
- Current item marker: `▶` or `►`

**Focus Management (NEW)**:

| Behavior | Description |
|----------|-------------|
| **Focus Trap** | Tab cycles within modal, never escapes to background |
| **Focus Restoration** | Remember & restore focus after overlay closes |
| **Focus Visible** | Always show which element has focus (no invisible focus) |
| **Logical Order** | Tab order follows visual layout (top-to-bottom, left-to-right) |

```rust
pub struct FocusManager {
    focus_stack: Vec<FocusTarget>,  // Stack for nested modals
    current_focus: Option<FocusTarget>,
}

impl FocusManager {
    /// Push focus to new modal, remember previous
    pub fn push(&mut self, target: FocusTarget) {
        if let Some(current) = self.current_focus.take() {
            self.focus_stack.push(current);
        }
        self.current_focus = Some(target);
    }

    /// Pop modal, restore previous focus
    pub fn pop(&mut self) -> Option<FocusTarget> {
        let popped = self.current_focus.take();
        self.current_focus = self.focus_stack.pop();
        popped
    }
}
```

**This alone makes the UI feel like "real software".**

---

### Phase 2: Reusable SelectableMenu Component

**Goal**: Build once, use everywhere (theme picker, settings, command palette, history)

**Must-Have Behaviors**:
```rust
pub struct SelectableMenu<T> {
    items: Vec<MenuItem<T>>,
    selected_index: usize,
    scroll_offset: usize,
    visible_count: usize,
    filter_text: Option<String>,

    // === NEW: Enhanced Options ===
    debounce_ms: u16,              // Typeahead filter debounce (default: 150ms)
    mouse_enabled: bool,           // Click to select (default: true)
    wrap_navigation: bool,         // Up at top → bottom (default: true)
    announce_selection: bool,      // Accessibility announcements (default: true)
    show_scroll_indicator: bool,   // Show ▲/▼ when scrollable (default: true)
}

pub struct MenuItem<T> {
    pub label: String,
    pub description: Option<String>,
    pub shortcut: Option<String>,  // NEW: Show shortcut hint (e.g., "^R")
    pub value: T,
    pub disabled: bool,
    pub group: Option<String>,     // NEW: For grouped menus
}

pub enum MenuEvent<T> {
    Selected(T),
    Cancelled,
    FilterChanged(String),
    // NEW: For parent to handle special keys
    Unhandled(KeyEvent),
}
```

**Key Features**:
- Up/Down + Home/End navigation
- PageUp/PageDown for long lists
- Visible selection highlight (reverse video)
- Scroll indicator when list overflows (`▲ 3 more` / `▼ 5 more`)
- **Mouse click support** (standard, not optional)
- **Debounced typeahead** (150ms default, prevents flicker)
- **Wrap navigation** (up at first item → last item)
- **Accessibility announcements** (selection changes announced)
- **Match highlighting** for filtered substrings (visual search feedback)

**Pro Feature - Typeahead Filter** (high wow, low effort):
```
╭─ Themes ─────────────────────────────╮
│ Filter: dra█                         │
├──────────────────────────────────────┤
│ ▶ dracula      ━┃┏  Bold contrast    │
╰─ 1/8 matches  ↑↓ navigate  Esc clear─╯
```
User types "drac" → jumps/filters to Dracula. Shows `x/y` count.

---

### Phase 2.5: Overlay Completeness (NEW)

**Goal**: Add the “trust layer” overlays that make the HUD feel complete.

#### A. Error Details / Diagnostics Overlay

- Banner shows short error: `✗ Transcription failed`
- Press `D` for details
- Details modal includes: error chain, HTTP status, retries, last log lines

#### B. Activity / Pipeline Overlay

Small modal showing recent pipeline steps + timings:
- Capture → Transcribe → Format → Send
- Each step shows duration + status

#### C. Scrollable Overlays + Footer Hint Bar

- Help/Error/Confirm overlays support scrolling (↑↓, PgUp/PgDn, Home/End)
- Always show a consistent footer hint bar (from Action Registry):
  `↑↓ navigate  PgUp/PgDn scroll  Enter select  Esc close`

**Files**: NEW `error_overlay.rs`, `pipeline_overlay.rs`

---

### Phase 2.6: HUD Widgets (Core Metrics) (NEW)

**Goal**: Make the overlay feel like a true developer HUD.

**Widgets (when data is available)**:
- Model status (active backend/model name)
- Response latency (send → first token)
- Context usage (used/limit)
- Queue depth (pending transcripts)
- Thinking indicator (when backend is processing)

**Notes**:
- Cost tracking is optional and only shown when the backend provides usage data.
- Widgets must degrade gracefully when data is unavailable.
- Define a backend metadata contract (model name, latency, context usage, costs) with
  optional fields; Codex is the first provider, others can plug in later.

**Files**: NEW `hud_widgets.rs`, `hud_metrics.rs`

---

### Phase 3: Theme Persistence

**Goal**: Save preferences across sessions

**Config Location**: `~/.config/codex-voice/preferences.toml`

**Format**:
```toml
[appearance]
theme = "catppuccin"
status_style = "banner"  # or "minimal"
unicode = true
animations = true

[audio]
sensitivity_db = -35.0
auto_voice = true
send_mode = "auto"  # auto | manual | confirm
```

**Implementation**:
1. Create `preferences.rs` module
2. Load on startup (before banner)
3. Save on change (theme picker, settings)
4. Respect `XDG_CONFIG_HOME` if set
5. CLI flags override saved preferences

**Files**: NEW `preferences.rs`, `main.rs`, `config.rs`

---

### Phase 3.2: Config Migration + Validation (NEW)

**Goal**: Prevent user config from bricking the app as schema evolves.

**Deliverables**:
- Preferences file versioning (schema version field)
- Validation with friendly warnings + safe defaults
- Automatic migration for older configs
- `codex-voice --check-config` optional validator

**Files**: NEW `config_migrate.rs`, `preferences.rs`

---

### Phase 3.5: Design Tokens Upgrade (Moved Earlier)

**Goal**: Semantic color system before adding more UI components

**Why Now**: All future components (Settings, Command Palette, History) need consistent tokens. Building without them means refactoring later.

```rust
pub struct ThemeColors {
    // === EXISTING (keep) ===
    pub recording: &'static str,
    pub processing: &'static str,
    pub success: &'static str,
    pub warning: &'static str,
    pub error: &'static str,
    pub info: &'static str,

    // === TEXT HIERARCHY (add) ===
    pub text_primary: &'static str,   // Main content
    pub text_muted: &'static str,     // Hints, shortcuts, secondary
    pub text_accent: &'static str,    // Highlighted/emphasized

    // === SURFACES (add) ===
    pub surface: &'static str,        // Panel background
    pub surface_alt: &'static str,    // Alternating rows, hover

    // === INTERACTIVE STATES (add) ===
    pub selection_bg: &'static str,   // Selected item background
    pub selection_fg: &'static str,   // Selected item text
    pub focus_border: &'static str,   // Focused panel border
    pub focus_ring: &'static str,     // Visible focus indicator (accessibility)
    pub disabled: &'static str,       // Disabled controls

    // === METER GRADIENT (add) ===
    pub meter_low: &'static str,      // Green zone (safe)
    pub meter_mid: &'static str,      // Yellow zone (loud)
    pub meter_high: &'static str,     // Red zone (clipping)

    // === ACCESSIBILITY (add) ===
    pub contrast_ratio: f32,          // Track WCAG compliance level
    pub announce_prefix: &'static str, // "[Status]" for screen readers
}
```

**Migration Path**:
1. Add new fields with defaults matching current behavior
2. Update one component at a time to use semantic tokens
3. No breaking changes during migration

**Files**: `theme.rs`

---

### Phase 3.6: Keybinding Customization (NEW)

**Goal**: Allow users to remap keyboard shortcuts

**Config Location**: `~/.config/codex-voice/keybindings.toml`

**Format**:
```toml
# Custom keybindings (override defaults)
[keybindings]
record = "ctrl+r"           # Default
auto_voice = "ctrl+v"       # Default
send_transcript = "ctrl+t"  # Default
command_palette = ":"       # Default (vim-style)
# command_palette = "ctrl+p" # Alternative (VS Code-style)
theme_picker = "ctrl+y"     # Default
help = "?"                  # Default
quit = "ctrl+q"             # Default

# User can add custom bindings
# clear_transcript = "ctrl+l"
# increase_sensitivity = "ctrl+up"
# decrease_sensitivity = "ctrl+down"
```

**Implementation**:
```rust
pub struct Keybindings {
    bindings: HashMap<Action, KeyCombo>,
    reverse_lookup: HashMap<KeyCombo, Action>,
}

impl Keybindings {
    pub fn load() -> Self {
        let defaults = Self::defaults();
        let user_config = Self::load_user_config();
        defaults.merge(user_config)
    }

    pub fn action_for_key(&self, key: KeyCombo) -> Option<Action> {
        self.reverse_lookup.get(&key).copied()
    }

    pub fn display_shortcut(&self, action: Action) -> String {
        // Returns "^R" or "Ctrl+R" based on display preference
    }
}
```

**Conflict Detection**:
- Warn on startup if two actions map to same key
- Provide `codex-voice --check-keybindings` to validate config

**Files**: NEW `keybindings.rs`, `config.rs`, `input.rs`

---

### Phase 4: Settings Overlay

**Goal**: Interactive settings panel with toggles and sliders

**Visual Design**:
```
╭─ Settings ───────────────────────────────────────────╮
│                                                      │
│   Auto-voice         [ON ] OFF                       │
│   Send mode          AUTO  [MANUAL]  CONFIRM         │
│   Sensitivity        ────●──────────  -35dB          │
│   Animations         [ON ] OFF                       │
│   Unicode            [ON ] OFF                       │
│                                                      │
├──────────────────────────────────────────────────────┤
│   ↑↓ navigate  ←→ change  Enter confirm  Esc cancel  │
╰──────────────────────────────────────────────────────╯
```

**Settings to Include**:
| Setting | Type | Values |
|---------|------|--------|
| Auto-voice | Toggle | On/Off |
| Send mode | Radio | Auto / Manual / Confirm |
| Sensitivity | Slider | -60dB to -20dB |
| Animations | Toggle | On/Off |
| Unicode | Toggle | On/Off |
| Theme | Link | Opens theme picker |
| Calibration | Link | Opens audio calibration |

---

### Phase 4.2: Audio Calibration Overlay (NEW)

**Goal**: Make sensitivity a guided, professional workflow.

**Flow**:
1. Measure ambient noise (3–5s)
2. Measure speech peak (3–5s)
3. Recommend threshold + show a “Set recommended” action

**UX**:
- Simple gauge + live meter
- Clear guidance text (e.g., “Speak naturally…”)
- Result summary and one‑key apply

---

### Phase 4.7: Text Editing Component (NEW)

**Goal**: Make confirm/edit flows feel real, not hacky.

**Minimum Editor Features**:
- Cursor movement + insert, backspace, delete
- Multi-line wrap
- Optional selection (nice-to-have)
- `Open in $EDITOR` fallback for long edits

**Use Cases**:
- Transcript confirm/edit flow
- Any future text fields (search, filters, notes)

**Files**: NEW `text_editor.rs`

---

### Phase 5: Command Palette

**Goal**: Discoverable access to all actions (like `:` in vim or `Ctrl+P` in VS Code)

**Trigger**: `:` or `Ctrl+P`

**Visual Design**:
```
╭─ Command Palette ────────────────────────────────────╮
│ > █                                                  │
├──────────────────────────────────────────────────────┤
│   Toggle auto-voice                           ^V     │
│   Start recording                             ^R     │
│   Stop recording                              Enter  │
│   Change theme...                             ^Y     │
│   Open settings...                            ^S     │
│   Adjust sensitivity...                              │
│   View history...                                    │
│   Open help                                   ?      │
│   Quit                                        ^Q     │
╰─ ↑↓ select  Enter run  Esc close ────────────────────╯
```

**Features**:
- Fuzzy search/filter as you type
- Shows keyboard shortcut hints
- Groups: Voice, Settings, Navigation
- Scales with features (add commands without UI changes)
- Driven by the Action Registry (same source as help + keybindings)

**Why it's worth it**: Solves discoverability, stops cramming shortcuts into status bar.

---

### Phase 6: Transcript Confirm + Edit Flow

**Goal**: Add safety/trust for manual/confirm send modes

**Visual Design**:
```
╭─ Confirm Transcript ─────────────────────────────────╮
│                                                      │
│   "Fix the login bug in the authentication          │
│    module. The issue is that users are being        │
│    logged out unexpectedly after..."                │
│                                                      │
│   Duration: 4.2s  │  Words: 28  │  Confidence: 94%  │
│                                                      │
├──────────────────────────────────────────────────────┤
│   Enter send  │  E edit  │  C copy  │  Esc cancel   │
╰──────────────────────────────────────────────────────╯
```

**Actions**:
| Key | Action |
|-----|--------|
| `Enter` | Send to Codex |
| `E` | Edit transcript (opens in prompt) |
| `C` | Copy to clipboard |
| `Esc` | Cancel/discard |

---

### Phase 7: History Panel

**Goal**: Browse and reuse previous transcripts

**Visual Design**:
```
╭─ History ────────────────────────────────────────────╮
│                                                      │
│   12:34  "Fix the login bug in..."          ✓ Sent   │
│   12:32  "Add unit tests for the..."        ✓ Sent   │
│   12:30  "Refactor authentication..."       ✗ Error  │
│   12:28  "What files handle routing?"       ✓ Sent   │
│   12:25  [No speech detected]               ⚠ Skip , │
│                                                      │
├──────────────────────────────────────────────────────┤
│   ↑↓ select  Enter reuse  / search  Esc close        │
╰──────────────────────────────────────────────────────╯
```

**Data Stored**:
```rust
pub struct HistoryEntry {
    pub timestamp: DateTime<Local>,
    pub transcript: String,
    pub duration_secs: f32,
    pub outcome: Outcome,  // Sent, Error, Cancelled, NoSpeech
}
```

**Features**:
- Last N transcripts (configurable, default 50)
- Search/filter
- Reuse: copies transcript back to prompt
- In full TUI: side panel option

---

### Phase 7.5: Outbox / Retry Queue (NEW)

**Goal**: Make failures recoverable and visible.

**Features**:
- Right-side “queued” count in HUD
- Outbox overlay to retry/discard failed sends
- Clear policy: auto-retry with backoff vs manual retry

---

### Phase 8: ASCII Art Startup Banner

**Goal**: Impressive first impression (lower priority than interactivity)

**Wide Terminal (80+ cols)**:
```
╭──────────────────────────────────────────────────────────────────────────────╮
│                                                                              │
│     ██████╗ ██████╗ ██████╗ ███████╗██╗  ██╗                                 │
│    ██╔════╝██╔═══██╗██╔══██╗██╔════╝╚██╗██╔╝                                 │
│    ██║     ██║   ██║██║  ██║█████╗   ╚███╔╝                                  │
│    ██║     ██║   ██║██║  ██║██╔══╝   ██╔██╗                                  │
│    ╚██████╗╚██████╔╝██████╔╝███████╗██╔╝ ██╗                                 │
│     ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝   VOICE v1.0.28                 │
│                                                                              │
│    ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐             │
│    │   ^R  Record    │  │   ^V  Auto      │  │   :  Commands   │             │
│    └─────────────────┘  └─────────────────┘  └─────────────────┘             │
│                                                                              │
│    Pipeline: Rust  │  Theme: coral  │  Auto: ON  │  -35dB                    │
│                                                                              │
╰──────────────────────────────────────────────────────────────────────────────╯
```

**Responsive Variants**: 60-col and minimal versions as previously designed.

---

### Phase 9: New Themes (Gruvbox, Solarized, High-Contrast)

**New Themes**:

| Theme | Style | Key Colors | Contrast |
|-------|-------|------------|----------|
| **Gruvbox** | Retro warm | #fb4934 red, #fabd2f yellow, #b8bb26 green, #83a598 aqua | AA |
| **Solarized Light** | Scientific light | #dc322f red, #b58900 yellow, #859900 green, #268bd2 blue | AA |
| **Solarized Dark** | Scientific dark | Same palette, dark bg | AA |
| **High-Contrast** | Maximum visibility | White/Black with colored backgrounds | **AAA** |

**High-Contrast Theme Details**:
```rust
pub const HIGH_CONTRAST: ThemeColors = ThemeColors {
    // All text is bright white or black for maximum contrast
    text_primary: "\x1b[97m",       // Bright white
    text_muted: "\x1b[37m",         // Regular white (no dim!)
    text_accent: "\x1b[93m",        // Bright yellow

    // Status colors use backgrounds for visibility
    recording: "\x1b[97;41m",       // White on red
    processing: "\x1b[30;43m",      // Black on yellow
    success: "\x1b[30;42m",         // Black on green
    error: "\x1b[97;41m",           // White on red
    warning: "\x1b[30;43m",         // Black on yellow

    // Borders are always visible
    border: "\x1b[97m",             // Bright white
    focus_border: "\x1b[93m",       // Bright yellow (stands out)

    // Pure black background
    bg_primary: "\x1b[40m",
    surface: "\x1b[40m",

    contrast_ratio: 21.0,           // Maximum possible
};
```

Add after core interactivity is solid.

---

### Phase 9.5: Release Engineering & Compatibility Matrix (NEW)

**Goal**: Make UI changes shippable and stable across platforms and terminals.

**Deliverables**:
- CI across Linux/macOS/Windows
- Terminal matrix smoke tests (Terminal.app, iTerm2, Kitty, WezTerm, Windows Terminal, VS Code)
- Feature flags / compile-time toggles for platform-specific behaviors
- Packaging checks (Homebrew, Windows packaging, Linux tarball)

---

## Micro-Interactions

### A. Stage-Based Pipeline Stepper

Instead of just "Processing...", show pipeline stages:

```
◯ Captured  →  ● Transcribing  →  ◯ Formatting  →  ◯ Sending
```

Or compact:
```
[2/4] Transcribing...
```

### B. Toast Notifications (Non-Blocking)

Small messages that auto-dismiss after ~2 seconds:

```
┌──────────────────────┐
│  ✓ Transcript sent   │
└──────────────────────┘
```

Types:
- `✓ Sent` (success)
- `⚠ Low input level` (warning)
- `✗ Network error` (error, with "press ? for details")

Toast rules:
- Max 2 visible at once (stacked)
- Never steals focus
- Auto-dismiss unless error

### C. Error Detail Expansion

When error occurs:
- Banner shows short message: `✗ Transcription failed`
- Press `D` to expand details in popup
- Keeps default UI clean while enabling debugging

### D. HUD Polish Details (Daily-Use Quality)

Small touches that make the overlay feel complete:
- Severity stripe in left gutter (accent per status).
- Meter peak-hold (~500ms) + clip indicator.
- "Details" affordance not just for errors (Info/Why/How).
- Standardize mode chips (fixed width, padding, separators).
- Clear modal hierarchy (top modal = stronger border; dim underneath).

---

## Accessibility & Compatibility

### CLI Flags to Add

| Flag | Purpose |
|------|---------|
| `--no-unicode` | ASCII-only indicators (*, >, -, etc.) |
| `--reduced-motion` | Disable spinners/meters for slow terminals (alias `--no-anim`) |
| `--high-contrast` | Maximum contrast colors |
| `--status-height 1|2|4` | Control banner size |

### Graceful Degradation at Breakpoints

| Width | Degradation |
|-------|-------------|
| 80+ | Full layout |
| 60-79 | Compact shortcuts (`^R` vs `Ctrl+R`) |
| 40-59 | Replace meter with `▮▮▯` gauge, short labels |
| 25-39 | Mode + essential hint only |
| <25 | Indicator + "? help" only |

### Always Visible

At any width, keep:
- Current mode indicator
- `? help` or `Esc close` hint

### Screen Reader Compatibility

| Feature | Implementation |
|---------|----------------|
| State announcements | Output to stderr with parseable format |
| Focus changes | Announce new focus target |
| Error messages | Assertive priority announcement |
| Selection changes | Polite priority announcement |

---

## Privacy + Sanitization (Phase -1)

Terminal overlays should be safe to run all day without leaking sensitive data or
allowing escape-sequence injection.

**Deliverables**
- Redaction policy (default-on) for any persistent logs/history.
- Crash logs store metadata only by default (durations, state transitions, error codes).
- History retention modes: store full / store truncated / store none, plus `history.enable`.
- Strip control characters from any user/transcript text before rendering.
  Remove ESC and other control chars (< 0x20) except newline where needed.
- Create preferences/history/log files with restrictive permissions (best-effort).

## Terminal Coexistence Contract (Phase -1 / 0.5)

Overlays run inside shells and multiplexers; protect scrollback and prompt state.

**Rules**
- Always save/restore cursor position during banner draw (no cursor drift).
- Never overwrite partially typed user input.
- Always write only to a known reserved region.
- On resize: recompute reserved rows, redraw full frame, clear old banner region.
- Tmux/multiplexer friendly: avoid assumptions about background color or cursor origin.
- Mouse enable policy: enable only while a modal is open, disable immediately on close.

## Update Coalescing / Backpressure (Phase 0.5)

High-frequency signals (e.g., meter) must not drive paint rate.

**Rule of thumb**
- State changes: repaint immediately.
- Continuous signals: coalesce to latest value and repaint on tick (10–15 FPS).

---

## Testing Strategy

### Unit Tests

| Component | Test Coverage |
|-----------|--------------|
| `SelectableMenu` | Navigation (up/down/home/end/pgup/pgdn), wrap behavior, filter matching |
| `FocusManager` | Push/pop stack, restoration after modal close |
| `ThemeColors` | Contrast ratio validation, fallback behavior |
| `Keybindings` | Conflict detection, merge with defaults |
| `Preferences` | Load/save/merge with CLI flags |

**Example Test**:
```rust
#[test]
fn selectable_menu_wraps_at_boundaries() {
    let mut menu = SelectableMenu::new(vec!["A", "B", "C"]);
    menu.wrap_navigation = true;

    menu.move_up(); // At index 0
    assert_eq!(menu.selected_index, 2); // Wrapped to last

    menu.move_down(); // At index 2
    assert_eq!(menu.selected_index, 0); // Wrapped to first
}
```

### Integration Tests

| Scenario | Validation |
|----------|------------|
| Overlay state transitions | Help → Theme Picker → Settings → Close |
| Focus trap in modals | Tab never escapes overlay |
| Theme persistence | Save theme, restart, verify loaded |
| Keybinding override | Custom binding takes precedence |

### Snapshot Tests

Capture rendered output for regression testing:

- Make render functions pure and deterministic (`state -> Vec<String>`)
- Snapshot widths: 120 / 80 / 60 / 40 / 25
- Snapshot flags: unicode on/off, reduced-motion, no-color
- Snapshot states: idle, recording, processing, warning, error

```rust
#[test]
fn theme_picker_renders_correctly() {
    let output = render_theme_picker(Theme::Coral, 80);
    insta::assert_snapshot!(output);
}
```

**Banner Snapshot Backbone**:
```rust
#[test]
fn banner_snapshots_across_widths() {
    for width in [120, 80, 60, 40, 25] {
        let lines = render_banner(&state, width, theme, flags);
        insta::assert_snapshot!(format!("w{}", width), lines.join("\n"));
    }
}
```

### Renderer Invariants (NEW)

- For each breakpoint, assert every rendered line display width ≤ terminal width
- Assert renderer never emits newline characters in HUD region

### Property-Based Tests (NEW)

**Targets**:
- `SelectableMenu` selection invariants (no out-of-bounds, wrap behavior)
- Filter invariants (selected index always valid after filter change)
- FocusManager stack never escapes modal focus

### Fuzz / Robustness Tests (NEW)

Targets:
- Input parser + router escape sequences (no panics on malformed CSI)
- Overlay state machine with random key + resize events
- Prompt/ANSI stripping logic
- Overlay rendering with extreme widths (0..300)

### Concurrency Correctness Tests (NEW)

If the UI receives events from multiple threads:
- Deterministic event queue for test interleavings
- “Nasty interleavings” for audio/network events
- Loom model tests for terminal restore + event loop invariants

### PTY Integration Tests (Optional but Powerful)

Run the app under a pseudo-terminal and:
- Send key sequences
- Simulate resize events
- Assert output contains expected frames

### Accessibility Audit Checklist

Before each release:

- [ ] All themes pass WCAG AA contrast (4.5:1 for text)
- [ ] High-contrast theme passes WCAG AAA (7:1)
- [ ] All interactive elements have visible focus indicators
- [ ] State changes produce announcements
- [ ] `--reduced-motion` disables all animations
- [ ] `--no-unicode` produces readable ASCII output
- [ ] Tab order is logical in all overlays

### Performance Tests

| Metric | Target |
|--------|--------|
| Render time (status line) | < 1ms |
| Flush time (status line) | < 1ms |
| Frame drops (steady state) | 0 |
| Filter response (100 items) | < 16ms (60fps) |
| Memory (history, 1000 entries) | < 10MB |
| Startup time | < 100ms |

### Definition of Done (Per Phase)

Every phase should be considered complete only if:
- ✅ Unit tests for core logic
- ✅ Snapshot tests for 80/60/40 widths (plus key flags)
- ✅ Works with `--no-color`, `--no-unicode`, `--reduced-motion`
- ✅ No terminal corruption on crash (panic restore test)
- ✅ Perf budget met (render < 1ms, no frame spam)
- ✅ Keybindings + help text updated
- ✅ Action registry stays source-of-truth for keys/help/palette
- ✅ Persistent data respects redaction/sanitization defaults

### ADR Checkpoints (Before Implementation)

For phases that change architecture or persistence, write an ADR in `docs/dev/adr/`
before coding. At minimum:
- Focus + selection model (input routing, modal focus stack)
- SelectableMenu component contract + reuse between overlay/TUI
- Preferences/config persistence + migration strategy (CLI overrides, schema version)
- Keybinding customization + command palette action registry
- History storage format/retention and privacy considerations

Update the ADR index in `docs/dev/adr/README.md` when adding new records.

---

## Implementation Roadmap

### Recommended Priority Order (Impact vs Effort)

| Priority | Phase | Task | Effort | Impact | Why This Order |
|----------|-------|------|--------|--------|----------------|
| **0** | -2 | Project Identity + Multi-Backend Architecture | Medium | **Critical** | Name + positioning must be settled first |
| **1** | -1 | Terminal Safety + Crash Diagnostics + Privacy | Medium | **Critical** | Prevents terminal corruption + data leaks |
| **2** | 0 | Accessibility Foundation | Low | **Critical** | Bake in from start, not retrofit |
| **3** | 0.5 | Performance Architecture | Medium | **High** | Prevents flicker + perf regressions |
| **4** | 1-2 | Focus & SelectableMenu (rat-widget) | Medium | **Critical** | Foundation for all overlays |
| **5** | 2.5 | Overlay Completeness | Low | High | Trust + debug visibility |
| **6** | 2.6 | HUD Module System | Medium | **High** | Core differentiator as true HUD |
| **7** | 3 | Theme Persistence | Low | High | Enable all preferences |
| **8** | 3.2 | Config Migration + Validation | Low | High | Prevents config brick |
| **9** | 3.5 | Design Tokens Upgrade | Low | High | Before building more UI |
| **10** | 3.6 | Action Registry + Keybinding Customization | Low | Medium | Prevents shortcut drift |
| **11** | 4 | Settings Overlay | Medium | High | Uses rat-widget components |
| **12** | 4.2 | Audio Calibration Overlay | Low | High | Makes sensitivity feel professional |
| **13** | 4.7 | Text Editing Component (tui-textarea) | Medium | High | Enables confirm/edit UX |
| **14** | 5 | Command Palette | Medium | **High** | Uses SelectableMenu |
| **15** | 6 | Transcript Confirm Flow | Low | Medium | Depends on editor |
| **16** | 7 | History Panel | Medium | Medium | Lower priority |
| **17** | 7.5 | Outbox / Retry Queue | Low | Medium | Trust + recoverability |
| **18** | 8 | ASCII Art Banner | Medium | Low | Pure polish |
| **19** | 9 | New Themes (+ High-Contrast) | Low | Medium | Uses design tokens |
| **20** | 9.5 | Release Engineering & Compat Matrix | Medium | Medium | Shippable across platforms |

### Gaps Addressed in This Revision

| Gap | Solution | Phase |
|-----|----------|-------|
| Name locks to Codex only | Universal name + multi-backend trait | -2 |
| Single backend (Codex) | `AiBackend` trait + implementations | -2 |
| No competitive positioning | Market analysis + differentiators | -2 |
| No widget library strategy | rat-widget + ratatui-interact stack | 1-2 |
| No icon vocabulary spec | Unicode-only icons, no emojis | All |
| No HUD module system | Pluggable fixed-width modules | 2.6 |
| No panic-safe terminal restore | Drop guard + panic hook terminal restore | -1 |
| No crash diagnostics | Structured logging + crash log + `doctor` | -1 |
| No privacy/redaction policy | Redaction + metadata-only crash logs | -1 |
| No control-char sanitization | Strip ESC/control chars before rendering | -1 |
| No terminal coexistence contract | Cursor/prompt/scrollback invariants | -1 / 0.5 |
| No render diffing/instrumentation | Dirty-line rendering + perf overlay | 0.5 |
| No update coalescing | Coalesce meter/timers to tick | 0.5 |
| No error details overlay | Diagnostics overlay with `D` details | 2.5 |
| No pipeline activity overlay | Activity/timing modal | 2.5 |
| No config migration story | Schema versioning + migrations | 3.2 |
| Tokens are escape strings | Store colors as data + compute escapes | 3.5 |
| No text editor widget | tui-textarea + `$EDITOR` fallback | 4.7 |
| No high-contrast theme | Added `Theme::HighContrast` | 0, 9 |
| No screen reader support | Added announcement system | 0 |
| No reduced motion option | Added `--reduced-motion` flag | 0 |
| No focus trap for modals | Added `FocusManager` with stack | 1 |
| No focus restoration | Push/pop focus on modal open/close | 1 |
| No action registry | Central action registry (keys/help/palette) | 3.6 |
| No shortcut customization | Added `keybindings.toml` | 3.6 |
| No typeahead debouncing | Added `debounce_ms` to SelectableMenu | 2 |
| No match highlighting | Highlight filter matches in list rows | 2 |
| No mouse support | Made mouse standard in SelectableMenu | 2 |
| No overlay stack | Overlay stack + focus restoration | 1 |
| No shared ui_core | Headless reducer + shared state/actions | 1 |
| No segment layout engine | Fixed-width segments + fit helpers | 0.5 |
| No no-wrap guarantee | Enforce line width + no newlines | -1 |
| No calibration flow | Guided audio calibration overlay | 4.2 |
| No outbox / retry | Failed queue + retry overlay | 7.5 |
| No scrollable overlays | Shared scroll handling + hint bar | 2.5 |
| No testing strategy | Added comprehensive test plan | New section |
| Design tokens too late | Moved from Phase 8 to Phase 3.5 | 3.5 |

### Implementation Notes

**Overlay Mode (Raw ANSI + Crossterm)**:
- Keep overlays modal: capture input until closed
- Single event loop with "current screen" state:
  ```rust
  enum OverlayMode {
      None,
      Help,
      ThemePicker,
      Settings,
      CommandPalette,
      History,
      TranscriptConfirm,
  }
  ```
- Minimize redraw: only on events or tick
- Tick rate: 10-15 FPS max for animations
- Render must be pure (state → lines); side effects only in handlers
- Cache last frame; only write changed lines; flush once
- Optional perf overlay for render/flush timing
- Reserved rows contract: render only in a known bottom region and always restore cursor
- Mouse enable only while a modal is open; disable on close
- No-wrap guarantee: never emit newlines when drawing HUD rows

**Full TUI (Ratatui)**:
- Use `ListState` for selection lists
- Use `Modifier::REVERSED` for selection highlight
- Focus model:
  ```rust
  enum Focus {
      Output,
      Prompt,
      Status,
      Sidebar,
  }
  ```
- Tab/Shift+Tab cycles focus
- Focused panel border uses `focus_border`

---

## Phase Execution Checklist (Claude)

Use this checklist for each phase before handing work off for review.
This complements the "Definition of Done (Per Phase)" in Testing Strategy.

### Phase Setup
- [ ] Phase scope defined (phase ID + deliverables)
- [ ] Dependencies tracked in `docs/active/MASTER_PLAN.md`
- [ ] Feature flags/config toggles documented (if applicable)

### Implementation Gates
- [ ] Render path remains pure (state → lines); no side effects in render
- [ ] Perf instrumentation updated for new render paths (render/flush timing)
- [ ] Error paths surface user-facing message + details/logging where relevant

### Testing & Verification
- [ ] Unit tests for new logic
- [ ] Snapshot tests across widths (120/80/60/40/25) + flags (no-color/no-unicode/reduced-motion)
- [ ] Property-based tests for menus/focus when touched
- [ ] Concurrency/PTY tests if threading/terminal lifecycle changed
- [ ] `cd rust_tui && cargo build --release --bin codex-voice`

### Docs & Tracking
- [ ] Doc updates per checklist (USAGE/CLI_FLAGS/README/etc)
- [ ] `docs/CHANGELOG.md` updated for user-facing changes
- [ ] `docs/active/MASTER_PLAN.md` updated; completed items moved to `docs/archive/`

### Review Handoff
- [ ] Self-review against security/memory/errors/concurrency/perf/style checklist
- [ ] Tests run + results listed in handoff
- [ ] Known risks/open questions documented
- [ ] If releasing: update changelog + bump version; maintain local review notes in `docs/active/` (gitignored)

---

## Future Vision: Beyond Whisper

This section explores features beyond the current roadmap, inspired by industry tools and emerging AI capabilities. These represent potential directions for Codex Voice to evolve into a comprehensive developer HUD.

### Bridge Milestones (Near-Term)

Short-term steps that reuse current work and unlock future features sooner:
- **Markdown renderer in TUI-only first** (use `tui-markdown` + optional `syntect`)
- **Evaluate dialog/prompt widgets** (ratatui-interact / tui-widgets) before building from scratch
- **Wrap third-party widgets behind our component trait** to keep shared primitives intact

### Visual Features from Industry Leaders

#### A. Markdown Rendering & Syntax Highlighting

Inspired by [md-tui](https://github.com/henriklovhaug/md-tui), [Textual Markdown](https://textual.textualize.io/widgets/markdown/), and [tui-markdown](https://deepwiki.com/joshka/tui-markdown).

**Use Case**: Display Claude's responses with proper formatting in the TUI.

```
╭─ Response ─────────────────────────────────────────────────────╮
│ Here's the fix for your authentication bug:                    │
│                                                                │
│ ```rust                                                        │
│ fn authenticate(user: &User) -> Result<Token, AuthError> {     │
│     if user.password_matches(&hash) {                          │
│         Ok(Token::generate(user.id))                           │
│     } else {                                                   │
│         Err(AuthError::InvalidCredentials)                     │
│     }                                                          │
│ }                                                              │
│ ```                                                            │
│                                                                │
│ The issue was **missing error handling** on line 42.           │
╰────────────────────────────────────────────────────────────────╯
```

**Implementation Options**:
- [tui-markdown](https://github.com/joshka/tui-markdown) - Rust library returns `ratatui::Text`
- [syntect](https://crates.io/crates/syntect) - Rust syntax highlighting (used by bat)
- Stream-render as response arrives (like Textual 5.0)

**Files**: NEW `markdown.rs`, `syntax.rs`

---

#### B. Inline Terminal Graphics

Inspired by [timg](https://github.com/hzeller/timg), [Are We Sixel Yet?](https://www.arewesixelyet.com/), and Claude Code [feature request #2266](https://github.com/anthropics/claude-code/issues/2266).

**Protocols Supported**:
| Protocol | Terminals | Quality |
|----------|-----------|---------|
| **Kitty Graphics** | Kitty, WezTerm, Konsole, Ghostty | Best |
| **iTerm2** | iTerm2, WezTerm, Konsole | Good |
| **Sixel** | XTerm, foot, VS Code terminal | Widely supported |

**Use Cases**:
- Display diagrams/charts from Claude responses
- Show screenshots for UI debugging
- Visualize code architecture diagrams
- Display mermaid/graphviz renders inline

```
╭─ Architecture Diagram ─────────────────────────────────╮
│  ┌─────────┐    ┌─────────┐    ┌─────────┐            │
│  │ Input   │───▶│ Voice   │───▶│ Whisper │            │
│  │ Thread  │    │ Manager │    │ STT     │            │
│  └─────────┘    └─────────┘    └─────────┘            │
│       │              │              │                  │
│       ▼              ▼              ▼                  │
│  ┌─────────────────────────────────────────┐          │
│  │            Writer Thread                 │          │
│  └─────────────────────────────────────────┘          │
╰────────────────────────────────────────────────────────╯
```

**Implementation**:
```rust
pub enum GraphicsProtocol {
    Kitty,
    ITerm2,
    Sixel,
    Ascii,  // Fallback
}

pub fn detect_graphics_support() -> GraphicsProtocol {
    // Query terminal capabilities
    // $TERM_PROGRAM, KITTY_WINDOW_ID, etc.
}

pub fn render_image(path: &Path, protocol: GraphicsProtocol) -> String {
    // Return appropriate escape sequences
}
```

**Files**: NEW `graphics.rs`, `image_render.rs`

---

#### C. Multi-Panel Lazygit-Style Layout

Inspired by [Lazygit](https://jesseduffield.com/Lazygit-5-Years-On/) and [Lazydocker](https://lazydocker.com/).

**Full TUI Mode Enhancement**:
```
┌─ Transcript History ──────┬─ Current Session ─────────────────────────┐
│ 12:34 "Fix the login..."  │ ● Recording... 2.3s                       │
│ 12:32 "Add unit tests..." │                                           │
│ 12:30 "Refactor auth..."  │ "Please add error handling to the        │
│ 12:28 "What files..."     │  authentication module and ensure all    │
│                           │  edge cases are covered..."               │
├───────────────────────────┼───────────────────────────────────────────┤
│ ▶ 12:34 Fix login bug     │ Session: 15 min │ Transcripts: 12         │
│   12:32 Add unit tests    │ Auto-voice: ON  │ Sent: 10 │ Errors: 1    │
└───────────────────────────┴───────────────────────────────────────────┘
 ^R record  ^V auto  Tab switch  / filter  ? help
```

**Key Patterns from Lazygit**:
- Vim-style navigation (j/k, gg/G)
- Real-time stats in status bar
- Panel focus with visual highlight
- Contextual keybindings per panel

---

### AI & LLM Extensions

#### D. MCP (Model Context Protocol) Integration

The [Model Context Protocol](https://www.anthropic.com/news/model-context-protocol) is the emerging standard for AI tool integration, adopted by Anthropic, OpenAI, Google, Microsoft.

**Codex Voice as MCP Client**:
```toml
# ~/.config/codex-voice/mcp.toml
[servers]
github = { command = "npx", args = ["-y", "@modelcontextprotocol/server-github"] }
filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem"] }
postgres = { command = "uvx", args = ["mcp-server-postgres", "postgresql://..."] }
```

**Voice-Triggered MCP Tools**:
```
You: "Check my open PRs on the auth repo"
     ↓
Codex Voice → MCP GitHub Server → Response
     ↓
HUD: "3 open PRs: #142 (review requested), #138 (approved), #135 (draft)"
```

**MCP Tool Categories**:
| Category | Example Servers | Voice Use Case |
|----------|-----------------|----------------|
| **Code** | GitHub, GitLab, filesystem | "Show diff for PR 142" |
| **Data** | Postgres, SQLite, Redis | "Query users created today" |
| **Docs** | Notion, Confluence | "Find the API spec for auth" |
| **Infra** | AWS, GCP, Docker | "Check staging pod status" |
| **Custom** | Your own tools | "Run the nightly build" |

**Files**: NEW `mcp_client.rs`, `mcp_config.rs`

---

#### E. Multi-Model Support

Inspired by [AIChat](https://github.com/sigoden/aichat) - "All-in-one LLM CLI tool with access to OpenAI, Claude, Gemini, Ollama, Groq, and more."

**Voice-to-Multiple-Models**:
```
╭─ Model Router ─────────────────────────────────────────╮
│                                                        │
│   Current: claude-3-opus                               │
│                                                        │
│   Available Models:                                    │
│   ▶ claude-3-opus     (Anthropic)      ✓ Connected    │
│     claude-3-sonnet   (Anthropic)      ✓ Connected    │
│     gpt-4-turbo       (OpenAI)         ✓ Connected    │
│     gemini-pro        (Google)         ○ Not configured│
│     llama-3-70b       (Ollama local)   ✓ Running      │
│                                                        │
├────────────────────────────────────────────────────────┤
│   ↑↓ select  Enter switch  Esc close                   │
╰────────────────────────────────────────────────────────╯
```

**Use Cases**:
- Route coding questions to Claude, general questions to GPT-4
- Use local Ollama for privacy-sensitive queries
- Compare responses across models
- Fallback to secondary model if primary is down

---

#### F. Local-First AI (Privacy Mode)

Inspired by [agent-cli](https://github.com/basnijholt/agent-cli) and [Jan](https://jan.ai/).

**Fully Offline Pipeline**:
```
Voice → Whisper (local) → Ollama/llama.cpp (local) → Response
         ↓
     No data leaves your machine
```

**Configuration**:
```toml
[privacy]
mode = "local"  # local | hybrid | cloud

[local_models]
stt = "whisper-large-v3"  # Already implemented!
llm = "ollama/codellama:34b"
tts = "piper"  # Optional: speak responses
```

**HUD Indicator**:
```
╭─── coral ─ 🔒 LOCAL ────────────────────────────────────╮
│ ● AUTO │ Local │ -40dB  ▁▂▃▅▆▇█  Processing locally... │
```

---

### Ambient HUD Features

#### G. Context-Aware Notifications

Inspired by [ambient agents research](https://www.digitalocean.com/community/tutorials/ambient-agents-context-aware-ai) and developer productivity studies showing [context switching costs 25 minutes](https://newsletter.techworld-with-milan.com/p/context-switching-is-the-main-productivity).

**Proactive HUD Alerts**:
```
┌────────────────────────────────────────────────┐
│ 💡 PR #142 has new comments (2 min ago)        │
│    Press Enter to view, Esc to dismiss         │
└────────────────────────────────────────────────┘
```

**Alert Types**:
| Trigger | Alert | Action |
|---------|-------|--------|
| PR review requested | Badge + toast | Voice: "Open PR 142" |
| Build failed | Error toast | Voice: "Show build logs" |
| Long silence detected | Gentle prompt | "Need help with something?" |
| Meeting starting | Calendar alert | "Meeting in 5 min" |

**Ambient Mode** (Background Awareness):
- Low-intensity monitoring of GitHub/Slack/Calendar
- Surfaces relevant info without interrupting flow
- Voice-triggered: "Any updates on my PRs?"

---

#### H. Session Analytics Dashboard

Inspired by [Warp terminal](https://www.warp.dev/) and [CC Statusline](https://claudelog.com/claude-code-mcps/ccstatusline/).

**Real-Time Stats in HUD**:
```
╭─ Session Stats ────────────────────────────────────────╮
│                                                        │
│   Session Duration    │ 45:23                          │
│   Voice Commands      │ 28                             │
│   Transcripts Sent    │ 24 (85% success)               │
│   Avg Response Time   │ 1.2s                           │
│   Tokens Used         │ 12,450 / 100,000               │
│                                                        │
│   ═══════════════════════════════════════════ 12.4%    │
│                                                        │
│   Most Used Commands:                                  │
│   • "fix the bug" (8x)                                 │
│   • "add tests" (5x)                                   │
│   • "explain this" (4x)                                │
│                                                        │
╰────────────────────────────────────────────────────────╯
```

**Productivity Insights**:
- Track voice vs typing ratio
- Identify frequently repeated prompts (suggest shortcuts)
- Measure time saved vs manual typing
- Export session logs for analysis

---

#### I. Voice Macros & Snippets

**Problem**: Developers say the same things repeatedly.

**Solution**: Voice-triggered macros.

```toml
# ~/.config/codex-voice/macros.toml
[macros]
"fix it" = "Please fix the bug in the code I just showed you"
"add tests" = "Add comprehensive unit tests for this function with edge cases"
"explain" = "Explain this code in detail, focusing on the key logic"
"review" = "Review this code for bugs, security issues, and best practices"

[snippets]
"my context" = """
I'm working on a Rust project using ratatui for TUI.
The codebase uses crossbeam channels for threading.
Please follow the existing patterns when suggesting changes.
"""
```

**Trigger**:
```
You: "fix it"
     ↓
Expands to: "Please fix the bug in the code I just showed you"
```

---

#### J. Text-to-Speech Response (Optional)

Inspired by [agent-cli](https://github.com/basnijholt/agent-cli) which uses Kokoro (GPU) or Piper (CPU) for TTS.

**Use Case**: Hands-free coding sessions.

```
You: "What does this function do?"
     ↓
Claude: [response]
     ↓
TTS: "This function authenticates users by verifying..."
```

**Configuration**:
```toml
[tts]
enabled = false  # opt-in
engine = "piper"  # piper | kokoro | system
voice = "en_US-amy-medium"
speed = 1.2
```

**When Useful**:
- Accessibility (vision-impaired developers)
- Hands-free debugging while looking at hardware
- Long explanations while you're thinking

---

### Integration Ecosystem

#### K. IDE Integrations (VS Code, Neovim)

**VS Code Extension**:
- Codex Voice status in VS Code status bar
- Voice commands routed to active editor
- Transcript appears in VS Code panel

**Neovim Plugin**:
```lua
-- ~/.config/nvim/lua/plugins/codex-voice.lua
return {
  "codex-voice/nvim",
  config = function()
    require("codex-voice").setup({
      auto_start = true,
      status_line = true,  -- Show in lualine
    })
  end,
}
```

---

#### L. Warp-Style AI Features

Inspired by [Warp terminal](https://www.warp.dev/).

**Ask AI for Error Explanation**:
```
$ cargo build
error[E0382]: borrow of moved value: `data`
  --> src/main.rs:10:5
   │
9  │     let result = process(data);
   │                          ---- value moved here
10 │     println!("{:?}", data);
   │     ^^^^^^^^^^^^^^^^^^^^^^ value borrowed here after move

[Press ^A to ask AI about this error]
```

**Voice Alternative**:
```
You: "Explain that error"
     ↓
HUD: "The error occurs because `data` was moved into `process()`
      on line 9, so it can't be used on line 10. You can fix this
      by cloning the data or borrowing it instead."
```

---

### Implementation Priority (Future Phases)

| Phase | Feature | Effort | Impact | Dependencies |
|-------|---------|--------|--------|--------------|
| **10** | Markdown Rendering | Medium | High | Phase 5 (Command Palette) |
| **11** | Syntax Highlighting | Low | Medium | Phase 10 |
| **12** | MCP Client | High | **Very High** | Phase 3 (Preferences) |
| **13** | Multi-Model Support | Medium | High | Phase 12 |
| **14** | Local LLM Mode | Medium | Medium | Phase 13 |
| **15** | Terminal Graphics | Medium | Medium | Terminal detection |
| **16** | Voice Macros | Low | Medium | Phase 3 (Preferences) |
| **17** | Session Analytics | Low | Medium | Phase 3 (Preferences) |
| **18** | TTS Responses | Low | Low | Optional dependency |
| **19** | Ambient Notifications | High | Medium | MCP + External APIs |
| **20** | IDE Integrations | High | High | Stable API |

---

### Research Sources for Future Vision

| Topic | Sources |
|-------|---------|
| Terminal Graphics | [Are We Sixel Yet?](https://www.arewesixelyet.com/), [timg](https://github.com/hzeller/timg), [rasterm](https://github.com/BourgeoisBear/rasterm) |
| Markdown TUI | [md-tui](https://github.com/henriklovhaug/md-tui), [tui-markdown](https://deepwiki.com/joshka/tui-markdown), [Textual Markdown](https://textual.textualize.io/widgets/markdown/) |
| MCP Protocol | [Anthropic MCP](https://www.anthropic.com/news/model-context-protocol), [MCP Year Review](https://www.pento.ai/blog/a-year-of-mcp-2025-review), [AAIF](https://www.linuxfoundation.org/press/linux-foundation-announces-the-formation-of-the-agentic-ai-foundation) |
| AI CLI Tools | [AIChat](https://github.com/sigoden/aichat), [agent-cli](https://github.com/basnijholt/agent-cli), [Aider](https://aider.chat/) |
| Ambient Awareness | [DigitalOcean Ambient Agents](https://www.digitalocean.com/community/tutorials/ambient-agents-context-aware-ai), [Context Switching Research](https://newsletter.techworld-with-milan.com/p/context-switching-is-the-main-productivity) |
| Developer Productivity | [Warp Terminal](https://www.warp.dev/), [CC Statusline](https://claudelog.com/claude-code-mcps/ccstatusline/) |

---

## Research & Resources

### Design Inspiration

| Source | Key Takeaways |
|--------|---------------|
| [Claude CLI](https://kotrotsos.medium.com/claude-code-internals-part-11-terminal-ui-542fe17db016) | React + Ink, 6 themes, auto light/dark |
| [Textual](https://textual.textualize.io/) | CSS-like styling, focus management, widgets |
| [Ratatui](https://ratatui.rs/) | Constraint layouts, built-in widgets |
| [CLI Guidelines](https://clig.dev/) | Modern CLI best practices |
| [Evil Martians](https://evilmartians.com/chronicles/cli-ux-best-practices-3-patterns-for-improving-progress-displays) | Progress indicators, UX patterns |
| [awesome-tuis](https://github.com/rothgar/awesome-tuis) | Curated TUI project list |
| [rat-widget](https://crates.io/crates/rat-widget) | Extended widgets with event handling + focus + scrolling |
| [rat-focus](https://crates.io/crates/rat-focus) | Focus model with ordered focus list |
| [ratatui-interact](https://crates.io/crates/ratatui-interact) | Interactive components + mouse hit-testing |
| [tui-widgets](https://crates.io/crates/tui-widgets) | Popups/prompts/scrollviews collection |
| [ratatui discussions](https://github.com/ratatui/ratatui/discussions/220) | Best practices from maintainers |

### Accessibility Resources

| Source | Key Takeaways |
|--------|---------------|
| [WCAG Design Tokens](https://www.w3.org/2023/09/13-inclusive-design-tokens-minutes.html) | Tokens for accessibility settings |
| [design.dev](https://design.dev/guides/design-systems/) | Design systems complete guide |
| [Accessible Color Tokens](https://www.aufaitux.com/blog/color-tokens-enterprise-design-systems-best-practices/) | Enterprise best practices |
| [USWDS Design Tokens](https://designsystem.digital.gov/design-tokens/) | US Government design system |

### Color Themes

| Theme | Source | Style |
|-------|--------|-------|
| [Catppuccin](https://github.com/catppuccin) | GitHub | Pastel, soothing |
| [Dracula](https://draculatheme.com) | Official | High contrast, vibrant |
| [Nord](https://www.nordtheme.com) | Official | Arctic blue-gray |
| [Gruvbox](https://github.com/morhetz/gruvbox) | GitHub | Retro warm |
| [Solarized](https://ethanschoonover.com/solarized/) | Official | Scientific precision |

### Spinner Options

```
Braille (modern):  ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏
Dots (minimal):    ⣾⣽⣻⢿⡿⣟⣯⣷
Bounce (classic):  [=   ] [==  ] [=== ]
```

---

## File Locations

| File | Purpose |
|------|---------|
| `src/bin/codex_overlay/status_line.rs` | Multi-row status banner layout |
| `src/bin/codex_overlay/status_style.rs` | Message styling, status types |
| `src/bin/codex_overlay/theme.rs` | Color palettes, border sets |
| `src/bin/codex_overlay/theme_picker.rs` | Theme selection overlay |
| `src/bin/codex_overlay/help.rs` | Help overlay |
| `src/bin/codex_overlay/banner.rs` | Startup banner |
| `src/bin/codex_overlay/audio_meter.rs` | Waveform visualization |
| `src/bin/codex_overlay/writer.rs` | Terminal I/O, rendering |
| `src/bin/codex_overlay/main.rs` | Main loop, input handling |
| `src/bin/codex_overlay/config.rs` | CLI flags, configuration |
| `src/bin/codex_overlay/session_stats.rs` | Exit statistics |

### New Files (Planned)

**Phase -2 (Identity & Multi-Backend)**:
| File | Purpose | Phase |
|------|---------|-------|
| `src/backend/mod.rs` | AiBackend trait + registry | -2 |
| `src/backend/claude.rs` | Claude Code backend | -2 |
| `src/backend/gemini.rs` | Gemini CLI backend | -2 |
| `src/backend/aider.rs` | Aider backend | -2 |
| `src/backend/opencode.rs` | OpenCode backend | -2 |
| `src/backend/custom.rs` | Custom command backend | -2 |
| `src/hud/mod.rs` | HUD module trait + registry | -2 |
| `src/hud/mode.rs` | Mode indicator module | -2 |
| `src/hud/meter.rs` | Audio meter module | -2 |
| `src/hud/latency.rs` | Latency display module | -2 |
| `src/hud/queue.rs` | Queue depth module | -2 |
| `src/icons.rs` | Icon vocabulary (Unicode/ASCII) | -2 |

**Phase -1 to 9.5 (Core Roadmap)**:
| File | Purpose | Phase |
|------|---------|-------|
| `terminal_restore.rs` | Terminal state guard + panic restore | -1 |
| `crash_log.rs` | Crash log + event ring buffer | -1 |
| `doctor.rs` | `codex-voice doctor` diagnostics | -1 |
| `privacy.rs` | Redaction policy + retention settings | -1 |
| `sanitize.rs` | Control-char stripping + safe render helpers | -1 |
| `accessibility.rs` | Announcements, contrast validation, reduced motion | 0 |
| `render_diff.rs` | Dirty-line diff rendering | 0.5 |
| `perf_metrics.rs` | Render/flush timing metrics | 0.5 |
| `perf_overlay.rs` | Perf debug overlay | 0.5 |
| `event_coalesce.rs` | Update coalescing for high-frequency signals | 0.5 |
| `hud_layout.rs` | Segment layout engine + fit helpers | 0.5 |
| `hud_metrics.rs` | HUD metrics collection/aggregation | 0.5 |
| `focus.rs` | FocusManager, focus trap, restoration | 1 |
| `overlay_stack.rs` | Overlay stack + modal routing | 1 |
| `ui_primitives/` | Shared UI primitives (menus, focus, actions, layout) | 1 |
| `ui_core/` | Headless state/actions/reducer/dirty flags | 1 |
| `selectable_menu.rs` | Reusable arrow-key menu component | 2 |
| `error_overlay.rs` | Error details modal | 2.5 |
| `pipeline_overlay.rs` | Pipeline activity overlay | 2.5 |
| `hud_widgets.rs` | HUD widgets (model/latency/context/queue) | 2.6 |
| `backend_metadata.rs` | Backend metadata contract (optional fields) | 2.6 |
| `preferences.rs` | Config file load/save | 3 |
| `config_migrate.rs` | Config versioning + migrations | 3.2 |
| `keybindings.rs` | Keybinding config load/save/merge | 3.6 |
| `actions.rs` | Action registry (keys/help/palette/settings) | 3.6 |
| `design_tokens.rs` | Token spec + contrast checks | 3.5 |
| `settings.rs` | Settings overlay | 4 |
| `calibration_overlay.rs` | Audio calibration flow | 4.2 |
| `text_editor.rs` | Minimal text editor widget | 4.7 |
| `command_palette.rs` | Command palette overlay | 5 |
| `history.rs` | Transcript history | 7 |
| `outbox.rs` | Outbox / retry queue | 7.5 |
| `toast.rs` | Toast notification system | Micro |
| `ci/terminal_matrix.yml` | Terminal compatibility smoke tests | 9.5 |

**Phase 10-20 (Future Vision)**:
| File | Purpose | Phase |
|------|---------|-------|
| `markdown.rs` | Markdown parsing and rendering | 10 |
| `syntax.rs` | Syntax highlighting for code blocks | 11 |
| `graphics.rs` | Terminal graphics protocol detection | 15 |
| `image_render.rs` | Sixel/Kitty/iTerm2 image rendering | 15 |
| `mcp_client.rs` | MCP protocol client implementation | 12 |
| `mcp_config.rs` | MCP server configuration | 12 |
| `model_router.rs` | Multi-model routing and selection | 13 |
| `local_llm.rs` | Local LLM integration (Ollama) | 14 |
| `macros.rs` | Voice macros and snippets | 16 |
| `analytics.rs` | Session analytics and stats | 17 |
| `tts.rs` | Text-to-speech response output | 18 |
| `ambient.rs` | Ambient notifications system | 19 |

---

## Next Steps

1. **Phase -2**: Project Identity & Multi-Backend Architecture
   - Finalize project name (decide on VoxDev, VoiceHUD, etc.)
   - Implement `AiBackend` trait + backend implementations
   - Add `--backend` CLI flag
   - Update all docs with new name

2. **Phase -1**: Terminal Safety Foundation
   - Panic-safe terminal restore + Drop guard
   - Structured logging + crash log
   - `voice-hud doctor` diagnostics

3. **Phase 0 + 0.5**: Accessibility + Performance Architecture
   - Accessibility flags + announcements
   - Dirty-line rendering + perf overlay instrumentation

4. **Phase 1-2**: Focus/SelectableMenu (using rat-widget)
   - Focus stack + reusable menus
   - Integrate rat-widget for buttons, toggles, sliders

5. **Phase 2.5-2.6**: Overlay Completeness + HUD Module System
   - Error details + pipeline activity overlays
   - Pluggable HUD modules (mode, meter, latency, queue)

6. **Phase 3 + 3.2**: Preferences + Config Migration
   - Load/save preferences
   - Schema versioning + automatic migration

7. **Phase 3.5-3.6**: Design Tokens + Keybindings
   - Semantic tokens
   - Custom keybindings

8. **Phase 4 + 4.7 + 5**: Settings, Editor, Command Palette
   - Settings overlay (using rat-widget)
   - Text editor widget (using tui-textarea)
   - Command palette

9. **Phase 6-9.5**: Confirm flow, History, Banner, Themes, Release Engineering

10. **Audit** after each phase before proceeding

---

*Last updated: January 2026*
*Research sources: awesome-tuis, Textual, ratatui discussions, WCAG guidelines, rat-widget, ratatui-interact*
