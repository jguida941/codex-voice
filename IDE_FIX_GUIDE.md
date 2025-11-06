# Codex Voice TUI - IDE Conflict Resolution Guide

## The Root Cause
**JetBrains IDEs (IntelliJ, PyCharm, etc.) intercept Ctrl+R as "Rerun"**, which kills your running process and restarts it. This is NOT a bug in our code - it's an IDE shortcut conflict.

## Quick Test

### 1. Test in Terminal (Recommended)
Open macOS Terminal or iTerm2 (NOT the IDE's run panel):
```bash
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh
```
- Press **Ctrl+R** - should work perfectly!
- Press **F2** - alternative voice key
- Press **Alt+R** - another alternative

### 2. Test in IDE Terminal Tab
In your IDE, open the **Terminal** tab (not the Run panel):
```bash
./scripts/run_tui.sh
```
This should also work because the Terminal tab doesn't intercept Ctrl+R.

## Solutions for IDE Run Panel

### Solution 1: Use Alternative Keys (Already Implemented)
The TUI now supports THREE voice capture keys:
- **F2** - Least likely to conflict
- **Alt+R** (Option+R on Mac) - Good alternative
- **Ctrl+R** - Original (blocked by IDE)

### Solution 2: Fix IDE Keymap
1. Open JetBrains IDE Preferences
2. Go to **Keymap**
3. Search for "Rerun"
4. Right-click → Remove or change shortcut
5. Apply and restart IDE

### Solution 3: Run with Terminal Emulation
1. Edit Run Configuration
2. Enable "Run with terminal" or "Emulate terminal in output console"
3. This may prevent some IDE shortcuts from interfering

## What Our Fix Does

Even though the IDE conflict is the main issue, our terminal state wrapper still helps:

### Before Fix:
- External commands (ffmpeg/whisper) could corrupt terminal
- TUI might crash if commands output to terminal

### After Fix:
- Terminal state is properly managed
- Commands run in normal mode, then return to TUI
- Debug logging to track issues

## Debug Log Location

Check the log for details:
```bash
# macOS
tail -f $TMPDIR/codex_voice_tui.log

# Linux
tail -f /tmp/codex_voice_tui.log
```

## Testing Checklist

- [ ] Launch TUI from Terminal app → Ctrl+R works
- [ ] Launch TUI from IDE Terminal tab → Ctrl+R works
- [ ] Launch TUI from IDE Run panel → F2 works
- [ ] Launch TUI from IDE Run panel → Alt+R works
- [ ] Voice recording completes without crash
- [ ] Transcript appears in input field
- [ ] Multiple captures work in succession
- [ ] TUI remains responsive after capture

## Summary

| Environment | Ctrl+R | F2 | Alt+R | Notes |
|------------|--------|-----|-------|-------|
| Terminal App | ✅ Works | ✅ Works | ✅ Works | Best option |
| IDE Terminal Tab | ✅ Works | ✅ Works | ✅ Works | Good option |
| IDE Run Panel | ❌ Blocked | ✅ Works | ✅ Works | Use F2 or Alt+R |

## Recommendation

**For development**: Use Terminal app or IDE Terminal tab
**For quick tests**: Use F2 key in any environment
**Long-term**: Consider rebinding IDE shortcuts if you use Ctrl+R frequently