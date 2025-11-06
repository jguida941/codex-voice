# Codex Voice TUI - Testing & Debug Guide

## 🚀 Quick Start Test

```bash
# 1. Clear the log
> $TMPDIR/codex_voice_tui.log

# 2. Launch TUI
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh

# 3. Test sequence:
#    - Press Ctrl+R (wait 5 seconds, speak)
#    - Press Enter (send to Codex)
#    - Press Ctrl+R again (wait 5 seconds, speak)
#    - Press Enter (THIS IS WHERE IT FAILS)

# 4. Exit with Ctrl+C

# 5. Check the log
cat $TMPDIR/codex_voice_tui.log
```

## 📊 Enhanced Logging (Just Added)

The TUI now logs:
- Every key press with modifiers
- Enter key presses with input content
- Voice capture success/failure
- Input field state after voice capture
- Send prompt success/failure

## 🔍 Debug the "Can't Send Second Message" Issue

### Step 1: Reproduce with Logging
```bash
# Clear log
> $TMPDIR/codex_voice_tui.log

# Launch TUI
./scripts/run_tui.sh
```

### Step 2: Follow This Exact Sequence
1. **Press Ctrl+R** → Speak → See transcript
2. **Press Enter** → Should work
3. **Press Ctrl+R** → Speak → See transcript
4. **Press Enter** → FAILS HERE?

### Step 3: Check What's in the Log
```bash
# Look for these patterns:
grep "Enter pressed" $TMPDIR/codex_voice_tui.log
grep "Input:" $TMPDIR/codex_voice_tui.log
grep "Key event" $TMPDIR/codex_voice_tui.log | tail -20
```

### Step 4: Test Other Keys When Stuck
When the Enter key doesn't work, try:
- **Backspace** - Does it delete characters?
- **Type 'a'** - Can you add text?
- **Esc** - Does it clear the field?
- **Arrow keys** - Do they scroll?

Log these results:
```bash
# After testing, check what keys were recognized:
grep "Key event" $TMPDIR/codex_voice_tui.log | tail -10
```

## 🔧 Test Why F2 and Alt+R Don't Work

### Test F2 Key
```bash
# Clear log
> $TMPDIR/codex_voice_tui.log

# Launch TUI
./scripts/run_tui.sh

# Press F2
# Check log for what was received:
grep "Key event" $TMPDIR/codex_voice_tui.log
```

Expected log entry if F2 works:
```
Key event: F(2) with modifiers: KeyModifiers(0x0)
```

### Test Alt+R (Option+R on Mac)
```bash
# Same process, press Alt+R (Option+R)
# Check log:
grep "Key event" $TMPDIR/codex_voice_tui.log
```

Expected log entry if Alt+R works:
```
Key event: Char('r') with modifiers: ALT
```

## 🛠️ Troubleshooting Commands

### 1. Test Audio Pipeline Separately
```bash
# Test recording
ffmpeg -f avfoundation -i ":0" -t 3 -loglevel quiet /tmp/test.wav
ls -la /tmp/test.wav

# Test transcription
/Users/jguida941/new_github_projects/codex_voice/.venv/bin/whisper \
  /tmp/test.wav \
  --output_dir /tmp \
  --output_format txt \
  --model base

cat /tmp/test.txt
```

### 2. Test with Fake Whisper (Isolate Audio Issues)
```bash
cd /Users/jguida941/new_github_projects/codex_voice/rust_tui

cargo run -- \
  --seconds 3 \
  --ffmpeg-device ":0" \
  --whisper-cmd ../stubs/fake_whisper \
  --whisper-model base \
  --codex-cmd codex
```

### 3. Check for Zombie Processes
```bash
# While TUI is "stuck"
ps aux | grep -E "(ffmpeg|whisper|codex)" | grep -v grep
```

## 📝 Log Analysis Commands

### View Full Log
```bash
cat $TMPDIR/codex_voice_tui.log
```

### View Only Errors
```bash
grep -i "error\|failed" $TMPDIR/codex_voice_tui.log
```

### Track Voice Capture Flow
```bash
grep -E "Ctrl\+R|Voice capture|Transcript|Input field" $TMPDIR/codex_voice_tui.log
```

### Track Enter Key Issues
```bash
grep -E "Enter pressed|send_prompt|Nothing to send" $TMPDIR/codex_voice_tui.log
```

## 🎯 Specific Tests for Your Issues

### Test 1: Is Input Field Actually Filled?
After second voice capture when Enter doesn't work:
```bash
# Check the log for:
grep "Input field now contains" $TMPDIR/codex_voice_tui.log | tail -1

# Should show something like:
# Input field now contains: 'your transcript here' (len=20)
```

### Test 2: Is Enter Key Being Detected?
When you press Enter after second capture:
```bash
# Check if Enter is logged:
grep "Enter pressed" $TMPDIR/codex_voice_tui.log | tail -1

# If nothing appears, the key isn't being detected
```

### Test 3: Is There an Error in send_prompt?
```bash
grep "send_prompt\|Calling Codex" $TMPDIR/codex_voice_tui.log | tail -5
```

## 💡 Hypothesis Testing

### Theory 1: Input Field Has Hidden Characters
After second voice capture, check:
```bash
grep "Input field now contains" $TMPDIR/codex_voice_tui.log | tail -1
# Look for weird characters or unexpected length
```

### Theory 2: Event Loop Is Blocked
```bash
# Monitor key events in real-time:
tail -f $TMPDIR/codex_voice_tui.log | grep "Key event"

# Then press various keys in the stuck TUI
# If no new lines appear, event loop is blocked
```

### Theory 3: Terminal State Corruption
```bash
# After TUI exits/crashes, reset terminal:
reset
clear

# Then try again
```

## 🔄 Alternative Workarounds

### Workaround 1: Use Python Version
```bash
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate
python codex_voice.py --seconds 5
```

### Workaround 2: Manual Reset Between Captures
1. After first exchange, press **Esc** to clear input
2. Then press **Ctrl+R** for voice capture
3. See if Enter works now

### Workaround 3: Exit and Restart
1. After first exchange works
2. Press **Ctrl+C** to exit
3. Restart TUI
4. Continue conversation

## 📈 Data to Collect

Please run this test and share:

```bash
# Complete test with logging
> $TMPDIR/codex_voice_tui.log  # Clear log
./scripts/run_tui.sh

# Do your normal flow that breaks:
# 1. Ctrl+R → Speak → Enter (works)
# 2. Ctrl+R → Speak → Enter (fails)
# 3. Press 'a' key (test if input works)
# 4. Press Backspace (test if it deletes)
# 5. Press F2 (test if it triggers)
# 6. Press Ctrl+C to exit

# Then share this output:
echo "=== LAST 50 LOG LINES ==="
tail -50 $TMPDIR/codex_voice_tui.log

echo "=== KEY EVENTS ==="
grep "Key event" $TMPDIR/codex_voice_tui.log

echo "=== ENTER PRESSES ==="
grep "Enter pressed" $TMPDIR/codex_voice_tui.log

echo "=== VOICE CAPTURES ==="
grep "Voice capture success" $TMPDIR/codex_voice_tui.log
```

## Summary

**Working**: Ctrl+R (first time), Enter (first time)
**Not Working**: Enter (second time), F2, Alt+R

The enhanced logging should reveal:
1. Whether Enter key is detected after second capture
2. What the input field contains
3. Why F2 and Alt+R don't trigger voice capture
4. Where the event loop might be getting stuck

Share the log output and we can pinpoint the exact issue!