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

### Tauri UI (overlay)

```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

Optional: pass the same CLI flags used by the headless binary after `--`:

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- --engine local --whisper-model-preset tiny
```

### Headless (no overlay)

Local (on-device Whisper):

```bash
cargo run --release -- --no-ui --engine local
```

Default preset is `medium`. To download a smaller model:

```bash
cargo run --release -- --no-ui --engine local --whisper-model-preset tiny
```

To use an existing model file:

```bash
cargo run --release -- --no-ui --engine local --whisper-model /path/to/ggml-medium.bin
```

To use large v3:

```bash
cargo run --release -- --no-ui --engine local --whisper-model-preset large-v3
```

Cloud (OpenAI-compatible):

```bash
export OPENAI_API_KEY="..."
cargo run --release -- --no-ui --engine openai
```

Configure endpoint/model if needed:

```bash
cargo run --release -- \\
  --no-ui \\
  --engine openai \\
  --openai-endpoint https://api.openai.com/v1/audio/transcriptions \\
  --openai-translation-endpoint https://api.openai.com/v1/audio/translations \\
  --openai-model whisper-1
```

## Using the overlay

- Press `Esc` to quit
- Drag the top bar to move the window
- Press `S` to show/hide the control bar (includes output language + sizing)

## Notes / Limitations

- Audio is segmented by a simple energy-based VAD. If it misses speech, tweak:
  - `--vad-threshold`
  - `--vad-end-silence-s`
- Local mode now emits streaming partials by default (OpenAI mode stays segment-based). You can tune latency/stability with:
  - `--asr-step-ms`
  - `--max-window-s`
  - `--partial-stable-iters`
  - `--min-speech-ms`
  - Or disable streaming with `--streaming=false`
- Default output language is **English** (`--output-language english`).
- Some audio may not be capturable (e.g. DRM-protected playback).

## Good Settings
  2. cargo run --release -- --no-ui --engine local --max-window-s 6 --asr-step-ms 600
  --input-language <INPUT_LANGUAGE>
      Input language (e.g. `en`, `zh`, `ja`) or `auto`
