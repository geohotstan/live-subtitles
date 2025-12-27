# Subtitles (Swift)

Native macOS subtitles overlay for **system audio** using:

- ScreenCaptureKit for desktop audio capture
- Speech for on-device transcription
- Translation for English + Chinese output

## Requirements

- macOS 15.0+
- Screen Recording permission (System Settings → Privacy & Security → Screen Recording)
- Speech Recognition permission (System Settings → Privacy & Security → Speech Recognition)

## Run (dev)

```bash
swift run
```

## Build an app bundle

```bash
./scripts/build_app.sh
open build/Subtitles.app
```

## Flags

- `--input-locale <id>`: input speech locale, e.g. `ja-JP`, `zh-CN`, `en-US`
- `--input-language <name>`: shorthand for locale (e.g. `japanese`, `english`, `chinese`)
- `--output-mode <mode>`: `english`, `chinese`, `english-chinese`, `original`, `bilingual` (default: bilingual)

When `--input-language` or `--input-locale` is supplied, translation will use that language as the fixed source (no auto-detect).
- `--max-history <n>`: number of recent finalized subtitle cards to show (default: 2)
- `--partial-debounce-ms <ms>`: throttle partial updates (default: 200)
- `--include-self-audio`: include this app's audio in capture
- `--allow-cloud-recognition`: allow cloud recognition if on-device is unavailable
- `--debug-overlay`: show a more visible overlay background for troubleshooting

## Shortcuts

- `Ctrl + Q`: quit the app

## Notes

- Translation relies on Apple on-device models. If a language pair is missing, macOS may prompt to download it.
- Some audio is not capturable (e.g. DRM-protected playback).
