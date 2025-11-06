# Codex Voice TUI - Working Setup & Known Issues

## Current Status (November 5, 2024)

### ✅ What's Working
- **Ctrl+R** triggers voice capture successfully
- **First voice capture** works and transcribes correctly
- **First message** sends to Codex successfully
- **Second voice capture** records and transcribes

### ❌ Known Issues
1. **After second voice capture**: Can't send message (Enter key not working?)
2. **F2 and Alt+R**: Not triggering voice capture
3. **TUI may freeze**: After multiple voice captures

## Confirmed Working Commands

### Launch Command That Works
```bash
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh
```

### Working Flow (First Time)
1. Launch TUI with above command
2. Press **Ctrl+R** (working for you)
3. Speak for 5 seconds
4. Transcript appears in input box
5. Press **Enter** to send to Codex
6. Codex responds

### Issue After First Exchange
1. Press **Ctrl+R** again (still works)
2. Speak for 5 seconds
3. Transcript appears in input box
4. **PROBLEM**: Enter key doesn't send message
5. TUI appears stuck

## Debug Investigation Commands

### 1. Monitor Log During Issue
```bash
# In separate terminal, watch log in real-time
tail -f $TMPDIR/codex_voice_tui.log

# Clear log before testing
> $TMPDIR/codex_voice_tui.log

# Then reproduce the issue and check log
```

### 2. Test Voice Capture Components
```bash
# Test if it's a whisper issue
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate
echo "test" | whisper --model base --output_format txt --output_dir /tmp -

# Test if it's an ffmpeg issue
ffmpeg -f avfoundation -i ":0" -t 2 -loglevel quiet /tmp/test.wav
ls -la /tmp/test.wav
```

### 3. Check Process State
```bash
# While TUI is "stuck", in another terminal:
ps aux | grep -E "(cargo|rust_tui|ffmpeg|whisper)"

# Check if any zombie processes
ps aux | grep defunct
```

## Potential Fixes to Try

### Fix 1: Kill and Restart
```bash
# If TUI gets stuck
# Press Ctrl+C multiple times to force quit
# Then restart:
./scripts/run_tui.sh
```

### Fix 2: Use Shorter Recording Time
```bash
# Modify recording time to 3 seconds
cd /Users/jguida941/new_github_projects/codex_voice/rust_tui
cargo run -- \
  --seconds 3 \
  --ffmpeg-device ":0" \
  --whisper-cmd /Users/jguida941/new_github_projects/codex_voice/.venv/bin/whisper \
  --whisper-model base \
  --codex-cmd codex
```

### Fix 3: Test with Fake Whisper
```bash
# Use stub to isolate issue
cd /Users/jguida941/new_github_projects/codex_voice/rust_tui
cargo run -- \
  --seconds 3 \
  --ffmpeg-device ":0" \
  --whisper-cmd ../stubs/fake_whisper \
  --whisper-model base \
  --codex-cmd codex
```

## Log Analysis Points

Check the log for these patterns:
```bash
grep -E "(Voice capture|failed|error)" $TMPDIR/codex_voice_tui.log
grep "Ctrl+R pressed" $TMPDIR/codex_voice_tui.log
grep "Enter" $TMPDIR/codex_voice_tui.log
```

## Current Configuration

### run_tui.sh Settings
- Recording: 5 seconds
- Device: ":0" (default macOS audio)
- Whisper: `.venv/bin/whisper`
- Model: base
- Codex: codex

### Key Bindings Status
| Key | Should Work | Actually Works |
|-----|------------|----------------|
| Ctrl+R | Yes | ✅ YES |
| F2 | Yes | ❌ NO |
| Alt+R | Yes | ❌ NO |
| Enter | Yes | ⚠️ Only first time |
| Ctrl+C | Yes | ✅ YES |

## Next Debugging Steps

### 1. Add More Logging
Need to add logging for:
- Enter key presses
- Input field state after voice capture
- Any errors during send_prompt

### 2. Test Continuous Voice Mode
```bash
# Try Ctrl+V to toggle continuous mode
# See if issue happens there too
```

### 3. Check Input Field State
After second voice capture, try:
- Backspace (does it delete characters?)
- Type manually (can you add text?)
- Esc (does it clear the field?)

## Hypothesis About The Bug

The issue might be:
1. **Input field not properly cleared** after first send
2. **Voice capture not releasing control** properly
3. **Event loop getting blocked** after multiple operations
4. **Memory/buffer issue** with repeated captures

## Test Sequence to Reproduce

1. Launch: `./scripts/run_tui.sh`
2. First capture: Ctrl+R → Speak → Enter → ✅ Works
3. Second capture: Ctrl+R → Speak → Enter → ❌ Stuck
4. Check if you can:
   - Type in input field?
   - Use arrow keys to scroll?
   - Press Ctrl+C to exit?

## Alternative: Python Version

If Rust TUI continues to have issues:
```bash
# Use original Python version
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate
python codex_voice.py --seconds 5
```

## Summary of Issue

**Working**: Ctrl+R voice capture, first message send
**Broken**: Sending message after second voice capture
**Unknown**: Why F2 and Alt+R don't work

The TUI appears to get into a bad state after the first round-trip to Codex.