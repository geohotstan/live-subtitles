# Subtitles (macOS Sequoia+ MVP)

Live, always-on-top subtitles for **system audio output** on macOS using:

- **ScreenCaptureKit** for system audio capture
- **Local** transcription via `whisper.cpp` (Metal enabled)
- **Cloud** transcription/translation via **OpenAI-compatible** audio endpoints

This repo is an MVP focused on **macOS 15 (Sequoia) and newer**.

## Requirements

- macOS **15.0+**
- Screen Recording permission (System Settings → Privacy & Security → Screen Recording)
- For local transcription: first run downloads a Whisper model into `./models/`
- For cloud transcription: `OPENAI_API_KEY`

## Run

### Local (on-device Whisper)

```bash
cargo run --release -- --engine local
```

To download a specific model preset (`tiny`, `base`, `small`, `medium`, `large`):

```bash
cargo run --release -- --engine local --whisper-model-preset tiny
```

To use an existing model file:

```bash
cargo run --release -- --engine local --whisper-model /path/to/ggml-base.bin
```

### Cloud (OpenAI-compatible)

```bash
export OPENAI_API_KEY="..."
cargo run --release -- --engine openai
```

Configure endpoint/model if needed:

```bash
cargo run --release -- \
  --engine openai \
  --openai-endpoint https://api.openai.com/v1/audio/transcriptions \
  --openai-translation-endpoint https://api.openai.com/v1/audio/translations \
  --openai-model whisper-1
```

### Headless (no overlay)

```bash
cargo run --release -- --no-ui
```

## Using the overlay

- Press `Esc` to quit
- When controls are hidden: click-drag anywhere to move the window
- When controls are visible: `Alt` + click-drag to move the window
- Press `S` to show/hide the control bar (includes output language)

## Notes / Limitations

- Audio is segmented by a simple energy-based VAD. If it misses speech, tweak:
  - `--vad-threshold`
  - `--vad-end-silence-s`
- Default output language is **English** (`--output-language english`).
- Some audio may not be capturable (e.g. DRM-protected playback).
