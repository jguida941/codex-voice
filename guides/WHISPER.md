# Whisper Speech-to-Text

VoxTerm uses [Whisper](https://github.com/openai/whisper) for local speech-to-text.
All transcription happens on your machine - no audio is sent to the cloud.

## Contents

- [How It Works](#how-it-works)
- [Choosing a Model](#choosing-a-model)
- [Language Support](#language-support)
- [Model Download](#model-download)
- [Performance Tips](#performance-tips)

## How It Works

1. **You speak** → VoxTerm captures audio from your microphone
2. **Voice Activity Detection (VAD)** → Detects when you start/stop speaking
3. **Whisper transcribes** → Converts speech to text locally using whisper.cpp
4. **Text sent to CLI** → Transcript is typed into your AI CLI

The entire pipeline runs locally with ~250ms latency on modern hardware.

## Choosing a Model

| Model | Size | Speed | Accuracy | Best For |
|-------|------|-------|----------|----------|
| `tiny` | 75 MB | Fastest | Lower | Quick testing, low-end hardware |
| `base` | 142 MB | Fast | Good | **Recommended for most users** |
| `small` | 466 MB | Medium | Better | Default, good balance |
| `medium` | 1.5 GB | Slower | High | Non-English languages |
| `large` | 2.9 GB | Slowest | Highest | Maximum accuracy needed |

### Recommendations

- **Start with `base`** - Good accuracy, fast transcription, small download
- **Use `small`** if you need better accuracy and have the disk space
- **Use `medium` or `large`** for non-English languages or accented speech
- **Use `tiny`** only for testing or very low-end hardware

### Switching Models

Download a different model:
```bash
./scripts/setup.sh models --base    # 142 MB, recommended
./scripts/setup.sh models --small   # 466 MB, default
./scripts/setup.sh models --medium  # 1.5 GB
./scripts/setup.sh models --tiny    # 75 MB, fastest
```

Or specify at runtime:
```bash
voxterm --whisper-model base
voxterm --whisper-model-path /path/to/ggml-medium.en.bin
```

## Language Support

Whisper supports 99 languages. VoxTerm defaults to English but works with any supported language.

### Setting Your Language

```bash
# Explicit language (faster, more accurate)
voxterm --lang es        # Spanish
voxterm --lang fr        # French
voxterm --lang de        # German
voxterm --lang ja        # Japanese
voxterm --lang zh        # Chinese

# Auto-detect (slightly slower)
voxterm --lang auto
```

### Language-Specific Models

Models ending in `.en` are English-only and slightly faster/smaller:
- `ggml-base.en.bin` - English only
- `ggml-base.bin` - Multilingual

For non-English languages, use the multilingual models (without `.en`):
```bash
./scripts/setup.sh models --base  # Downloads base.en by default
```

To download multilingual models manually:
```bash
curl -L https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin \
  -o whisper_models/ggml-base.bin
```

### Tested Languages

| Language | Status | Notes |
|----------|--------|-------|
| English | Tested | Works great with `.en` models |
| Others | Should work | Use multilingual models, `--lang <code>` |

Full language list:
[Whisper supported languages](https://github.com/openai/whisper#available-models-and-languages)

## Model Download

Models are downloaded automatically on first run. Manual download:

```bash
# Using setup script (recommended)
./scripts/setup.sh models --base

# Direct download
curl -L https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin \
  -o whisper_models/ggml-base.en.bin
```

### Model Locations

VoxTerm looks for models in this order:
1. Path specified via `--whisper-model-path`
2. `whisper_models/` in the project directory
3. `~/.local/share/voxterm/models/` (Homebrew installs)

Override with environment variable:
```bash
export VOXTERM_MODEL_DIR=/path/to/models
```

## Performance Tips

### Reduce Latency

1. **Use a smaller model** - `base` is 2-3x faster than `small`
2. **Speak in shorter phrases** - Transcription time scales with audio length
3. **Use English-only models** - `.en` models are slightly faster
4. **Set explicit language** - Avoids auto-detection overhead

### Improve Accuracy

1. **Use a larger model** - `small` or `medium` for better results
2. **Speak clearly** - Pause between sentences
3. **Reduce background noise** - Adjust mic sensitivity with `Ctrl+]` / `Ctrl+\`
4. **Set the correct language** - Don't rely on auto-detect

### Troubleshooting

**Transcription is slow:**
- Switch to a smaller model (`--whisper-model base`)
- Check CPU usage - Whisper is CPU-intensive

**Wrong language detected:**
- Set language explicitly (`--lang en`)
- Use language-specific model (`.en` for English)

**Poor accuracy:**
- Try a larger model (`--whisper-model medium`)
- Adjust mic sensitivity
- Speak closer to the microphone

## See Also

| Topic | Link |
|-------|------|
| CLI Flags | [CLI_FLAGS.md](CLI_FLAGS.md) |
| Troubleshooting | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
