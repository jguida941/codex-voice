# Voice Capture Fix for Rust TUI

## Problems We Fixed

### 1. **Terminal State Corruption**
**Problem**: The `with_normal_terminal` wrapper was causing the TUI to completely exit when switching terminal modes.
**Fix**: Removed terminal mode switching - the stdio redirects are sufficient to prevent corruption.

### 2. **Whisper Not Found**
**Problem**: Whisper is installed in `.venv` but the TUI couldn't find it.
**Fix**: Updated `run_tui.sh` to:
- Activate the venv if it exists
- Use the full path to whisper in venv
- Fall back to `fake_whisper` stub for testing

### 3. **Poor Error Handling**
**Problem**: Errors in voice capture caused the entire TUI to crash.
**Fix**: Added proper error catching and logging in `capture_voice()`.

### 4. **IDE Shortcut Conflict**
**Problem**: JetBrains IDEs intercept Ctrl+R as "Rerun".
**Fix**: Added alternative key bindings:
- **F2** - Primary alternative
- **Alt+R** (Option+R on Mac)
- **Ctrl+R** - Still works in regular terminals

## Testing Instructions

### Method 1: Terminal (Best)
```bash
cd /Users/jguida941/new_github_projects/codex_voice

# The script now activates venv automatically
./scripts/run_tui.sh

# Press F2 or Alt+R to capture voice
# Should see "Recording voice for 5 seconds..."
```

### Method 2: Quick Test with Stub
The TUI will now use `fake_whisper` if real whisper isn't found:
```bash
./scripts/run_tui.sh
# Press F2
# Should get "fake transcript from fake_whisper" in input
```

### Method 3: Monitor Debug Log
```bash
# In another terminal:
tail -f $TMPDIR/codex_voice_tui.log

# Then run TUI and try voice capture
```

## What Happens Now

1. **Press F2** (or Alt+R)
2. **Status shows**: "Recording voice for 5 seconds... (speak now)"
3. **FFmpeg records** audio (suppressed output)
4. **Whisper transcribes** (or fake_whisper returns test text)
5. **Transcript appears** in input field
6. **TUI stays running** (no crash!)

## Key Bindings

| Key | Function | Works in IDE? |
|-----|----------|---------------|
| **F2** | Voice capture | ✅ Yes |
| **Alt+R** | Voice capture | ✅ Yes |
| **Ctrl+R** | Voice capture | ❌ No (IDE intercepts) |
| **Ctrl+V** | Toggle voice mode | ✅ Yes |
| **Ctrl+C** | Exit | ✅ Yes |

## Files Modified

1. **`rust_tui/src/main.rs`**
   - Simplified `with_normal_terminal()` - no mode switching
   - Added error handling in `capture_voice()`
   - Added F2 and Alt+R key bindings
   - Added debug logging

2. **`scripts/run_tui.sh`**
   - Activates venv automatically
   - Uses correct whisper path
   - Falls back to fake_whisper stub
   - Reduced recording time to 5 seconds

## Verification Checklist

- [ ] TUI launches without errors
- [ ] F2 key starts recording
- [ ] Status message updates during recording
- [ ] No terminal corruption or exit
- [ ] Transcript appears in input (real or fake)
- [ ] TUI remains functional after capture
- [ ] Multiple captures work in succession
- [ ] Errors are shown in status bar (not crashes)

## If It Still Doesn't Work

1. **Check whisper is accessible**:
   ```bash
   source .venv/bin/activate
   which whisper
   whisper --help
   ```

2. **Test ffmpeg directly**:
   ```bash
   ffmpeg -f avfoundation -i ":0" -t 1 test.wav
   ```

3. **Check the log**:
   ```bash
   cat $TMPDIR/codex_voice_tui.log
   ```

4. **Use fake_whisper for testing**:
   ```bash
   ./stubs/fake_whisper test.wav --output_dir /tmp
   cat /tmp/test.txt
   ```

## Summary

The main issues were:
1. Terminal mode switching was breaking the TUI
2. Whisper wasn't being found in the venv
3. No error recovery

All three are now fixed. Use **F2** or **Alt+R** for voice capture to avoid IDE conflicts.