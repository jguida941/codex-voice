# Architecture Decision Records (ADRs)

ADRs capture the "why" behind key technical decisions so we can revisit them later
without re-litigating old context. Keep them short, factual, and specific.

## Index

### Core Architecture
| ADR | Title | Status |
|-----|-------|--------|
| [0002](0002-pty-passthrough-architecture.md) | PTY Passthrough Architecture | Accepted |
| [0009](0009-serialized-output-writer.md) | Serialized Output Writer Thread | Accepted |
| [0010](0010-sigwinch-atomic-flag.md) | SIGWINCH Signal Handling | Accepted |
| [0014](0014-json-ipc-protocol.md) | JSON IPC Protocol for External UIs | Accepted |

### Voice Pipeline
| ADR | Title | Status |
|-----|-------|--------|
| [0003](0003-non-streaming-stt.md) | Non-Streaming Speech-to-Text | Accepted |
| [0004](0004-python-fallback-chain.md) | Python Fallback Chain | Accepted |
| [0006](0006-auto-learn-prompt-detection.md) | Auto-Learn Prompt Detection | Accepted |
| [0011](0011-voice-send-modes.md) | Voice Send Modes (Auto vs Insert) | Accepted |

### Audio Processing
| ADR | Title | Status |
|-----|-------|--------|
| [0007](0007-mono-audio-downmixing.md) | Mono Audio Downmixing | Accepted |
| [0012](0012-bounded-audio-channels.md) | Bounded Audio Channel Capacities | Accepted |
| [0015](0015-no-device-hotplug.md) | No Audio Device Hotplug Recovery | Accepted |

### UX and Controls
| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-sensitivity-hotkeys.md) | Sensitivity Hotkeys (Ctrl+]/Ctrl+\\) | Accepted |
| [0008](0008-transcript-queue-overflow.md) | Transcript Queue Overflow Handling | Accepted |

### UI and HUD Architecture
| ADR | Title | Status |
|-----|-------|--------|
| [0017](0017-focus-and-overlay-stack.md) | Focus and Overlay Stack Model | Proposed |
| [0018](0018-selectable-menu-component.md) | SelectableMenu Component Contract | Proposed |
| [0019](0019-preferences-and-config-migrations.md) | Preferences and Config Migrations | Proposed |
| [0020](0020-action-registry-and-keybindings.md) | Action Registry and Keybindings | Proposed |
| [0021](0021-history-storage-and-retention.md) | History Storage and Retention | Proposed |
| [0022](0022-render-guarantees-and-layout-contract.md) | Render Guarantees and Layout Contract | Proposed |

### Security and Privacy
| ADR | Title | Status |
|-----|-------|--------|
| [0005](0005-logging-opt-in.md) | Logging Opt-In by Default | Accepted |
| [0013](0013-security-hard-limits.md) | Security Hard Limits | Accepted |

---

## Location
- Store ADRs in this folder: `docs/dev/adr/`

## Naming
- `NNNN-short-title.md` (4-digit, zero-padded)
- Example: `0015-new-feature-decision.md`

## Template
- Start from [0000-template.md](0000-template.md)

## Statuses
- **Proposed** - Under discussion
- **Accepted** - Decision made and implemented
- **Deprecated** - No longer applies
- **Superseded** - Replaced by another ADR (link to replacement)

## Process
1. Copy the template and increment the number (next: 0016)
2. Fill in Context, Decision, and Consequences
3. Add links to related docs or code
4. Update this index

## When to Write an ADR

Write an ADR when:
- Making a decision that affects multiple modules
- Choosing between multiple valid approaches
- Implementing something that future maintainers might question
- Making a trade-off (performance vs simplicity, security vs convenience, etc.)
- Changing an existing architectural decision

Don't write an ADR for:
- Bug fixes
- Minor refactoring
- Single-module implementation details
- Decisions already documented elsewhere
