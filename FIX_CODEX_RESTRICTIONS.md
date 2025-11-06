# Fix: Remove Codex Restrictions & Improve Voice Settings

## Problems to Fix

1. **Codex is read-only** - Can't edit files
2. **Recording is too short** - Only 5 seconds
3. **Too slow** - 5-6 second delays
4. **No tool access** - Codex tools are restricted

## Root Causes

1. Using `--skip-git-repo-check` flag restricts Codex
2. Using `exec` mode instead of interactive
3. Hard-coded 5-second recording limit
4. Not maintaining Codex session (spawning new each time)

## Quick Fixes

### 1. Remove Restrictions (Immediate)

Edit `/rust_tui/src/main.rs`:

```rust
// Remove the --skip-git-repo-check flag
// Change line 572 from:
.arg("--skip-git-repo-check")
// To: (just remove it)
```

### 2. Make Recording Time Configurable

Already supported! Just run with different duration:
```bash
# 10 seconds recording
./scripts/run_tui.sh --seconds 10

# 15 seconds recording
./scripts/run_tui.sh --seconds 15

# Or edit scripts/run_tui.sh to change default
```

### 3. Give Codex Full Access

We need to:
- Remove restrictive flags
- Use the current directory (not parent)
- Let Codex have its normal environment

## Implementation

Let me fix these issues now...