# Codex Voice TUI - Complete Run Instructions

## ✅ Confirmed Working Setup

### Prerequisites Installed
- ✅ FFmpeg (for audio recording)
- ✅ Whisper (in `.venv/bin/whisper`)
- ✅ Codex CLI (`codex` command)
- ✅ Rust/Cargo (for building TUI)
- ✅ Python 3 (for PTY helper)

### What Currently Works
- ✅ **Ctrl+R** triggers voice capture
- ✅ **First message** sends successfully
- ✅ **Codex responds** correctly
- ⚠️ **Second voice capture** works but can't send
- ❌ **F2 key** doesn't trigger voice capture
- ❌ **Alt+R** doesn't trigger voice capture

## 🚀 How to Run

### Method 1: Using the Launch Script (Recommended)
```bash
cd /Users/jguida941/new_github_projects/codex_voice
./scripts/run_tui.sh
```

This script:
- Activates Python venv (if exists)
- Sets up whisper path
- Configures 5-second recording
- Launches the Rust TUI

### Method 2: Direct Cargo Run
```bash
cd /Users/jguida941/new_github_projects/codex_voice/rust_tui

cargo run -- \
  --seconds 5 \
  --ffmpeg-device ":0" \
  --whisper-cmd /Users/jguida941/new_github_projects/codex_voice/.venv/bin/whisper \
  --whisper-model base \
  --codex-cmd codex
```

### Method 3: Build Then Run
```bash
# Build once
cd /Users/jguida941/new_github_projects/codex_voice/rust_tui
cargo build --release

# Run the binary directly
./target/release/rust_tui \
  --seconds 5 \
  --ffmpeg-device ":0" \
  --whisper-cmd /Users/jguida941/new_github_projects/codex_voice/.venv/bin/whisper \
  --whisper-model base \
  --codex-cmd codex
```

## 🎮 How to Use

### Basic Flow (First Message - Works)
1. Launch TUI with `./scripts/run_tui.sh`
2. Press **Ctrl+R** to start recording
3. Speak for 5 seconds
4. Transcript appears in input box
5. Press **Enter** to send to Codex
6. Codex response appears in output

### Known Issue (Second Message)
1. Press **Ctrl+R** again - recording works
2. Transcript appears in input box
3. Press **Enter** - ❌ DOESN'T SEND
4. TUI appears stuck

### Workaround for Multiple Messages
```bash
# After each message:
1. Send first message (works)
2. Press Ctrl+C to exit
3. Run ./scripts/run_tui.sh again
4. Continue conversation
```

## 🔑 Key Bindings

| Key | Action | Status |
|-----|--------|--------|
| **Ctrl+R** | Voice capture | ✅ Works |
| **F2** | Voice capture (alt) | ❌ Not working |
| **Alt+R** | Voice capture (alt) | ❌ Not working |
| **Enter** | Send message | ⚠️ Only first time |
| **Backspace** | Delete character | ✅ Works |
| **Esc** | Clear input | ✅ Works |
| **↑/↓** | Scroll output | ✅ Works |
| **Ctrl+V** | Toggle voice mode | ❓ Untested |
| **Ctrl+C** | Exit TUI | ✅ Works |

## 🔧 Configuration

### Audio Settings
- **Device**: `:0` (default macOS microphone)
- **Duration**: 5 seconds
- **Format**: 16kHz mono WAV

### Whisper Settings
- **Model**: base (fastest)
- **Language**: English (auto-detected)
- **Output**: Text file in temp directory

### Codex Settings
- **Mode**: exec with --skip-git-repo-check
- **Working Dir**: Parent directory (codex_voice)

## 🐛 Debugging

### Check the Log
```bash
# View log location
echo $TMPDIR/codex_voice_tui.log

# Monitor log in real-time
tail -f $TMPDIR/codex_voice_tui.log

# Clear log before testing
> $TMPDIR/codex_voice_tui.log
```

### Test Components Individually

#### Test Audio Recording
```bash
ffmpeg -f avfoundation -i ":0" -t 3 /tmp/test.wav
# Should create a WAV file
```

#### Test Whisper
```bash
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate
whisper /tmp/test.wav --model base --output_dir /tmp
cat /tmp/test.txt
```

#### Test Codex
```bash
echo "Hello" | codex exec --skip-git-repo-check -
```

## 📁 Project Structure

```
codex_voice/
├── rust_tui/           # Rust TUI application
│   ├── src/
│   │   └── main.rs    # Main TUI code
│   └── Cargo.toml     # Rust dependencies
├── scripts/
│   ├── run_tui.sh     # Launch script
│   └── run_in_pty.py  # PTY helper
├── stubs/
│   └── fake_whisper   # Test stub
├── .venv/             # Python virtual environment
│   └── bin/
│       └── whisper    # OpenAI Whisper
└── codex_voice.py     # Original Python version
```

## 🔄 Alternative: Python Version

If the Rust TUI has issues, use the original Python version:

```bash
# Activate venv
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate

# Run Python version
python codex_voice.py \
  --seconds 5 \
  --ffmpeg-device ":0" \
  --whisper-model base \
  --codex-cmd codex

# Usage:
# - Just speak when it says "Listening..."
# - Transcript is sent to Codex automatically
# - Response is printed
# - Starts listening again
```

## 🚨 Common Issues & Fixes

### Issue: "whisper: command not found"
```bash
# Activate venv first
source /Users/jguida941/new_github_projects/codex_voice/.venv/bin/activate
# Or use full path
/Users/jguida941/new_github_projects/codex_voice/.venv/bin/whisper
```

### Issue: "codex: command not found"
```bash
# Install Codex CLI
curl -L https://github.com/continuedev/continue/releases/latest/download/codex-darwin-arm64 -o codex
chmod +x codex
sudo mv codex /usr/local/bin/
```

### Issue: No audio recorded
```bash
# Check microphone permissions
# System Preferences → Security & Privacy → Microphone
# Allow Terminal/iTerm

# Test different audio device
ffmpeg -f avfoundation -list_devices true -i ""
# Try :1 or :2 instead of :0
```

### Issue: TUI crashes on voice capture
```bash
# Check the log
cat $TMPDIR/codex_voice_tui.log

# Reset terminal if corrupted
reset
clear
```

## 📊 Current Status Summary

| Feature | Status | Notes |
|---------|--------|-------|
| Voice Recording | ✅ Works | 5 seconds via FFmpeg |
| Transcription | ✅ Works | Via Whisper in venv |
| First Send | ✅ Works | Enter key works |
| Second Send | ❌ Broken | Enter key doesn't work |
| Multiple Keys | ❌ F2, Alt+R broken | Only Ctrl+R works |
| Scrolling | ✅ Works | Arrow keys work |
| Exit | ✅ Works | Ctrl+C works |

## 🎯 Next Steps

1. **Fix Enter key issue** after second voice capture
2. **Debug F2 and Alt+R** key detection
3. **Add persistent session** to avoid restarts
4. **Implement streaming** for real-time feedback

## 📝 Quick Test Commands

```bash
# One-liner to test everything
cd /Users/jguida941/new_github_projects/codex_voice && \
> $TMPDIR/codex_voice_tui.log && \
./scripts/run_tui.sh

# Then:
# 1. Press Ctrl+R, speak, press Enter (should work)
# 2. Press Ctrl+R, speak, press Enter (will fail)
# 3. Check log: cat $TMPDIR/codex_voice_tui.log
```

## Contact

File issues at: `/Users/jguida941/new_github_projects/codex_voice/`

---

Last tested: November 5, 2024
Working keys: Ctrl+R (first capture only)
Known issues: Second Enter key press fails