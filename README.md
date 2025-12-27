# Subtitles

This repo provides **live, always-on-top subtitles for desktop audio on macOS**. The goal is to make any audio you’re listening to (videos, meetings, streaming, or local media) immediately readable as on-screen captions, with optional translation for bilingual viewing. It’s designed as a lightweight, local‑first overlay that can sit above full‑screen content and help you follow along in real time.

Core idea:

- Capture **system audio output** (not just microphone input)
- Transcribe speech into text with low latency
- Optionally translate into another language
- Present it as a clean, floating subtitle overlay

This repo now contains two projects:

- `subtitles-rs`: Rust + Tauri overlay (original project)
- `subtitles-swift`: Native macOS app using ScreenCaptureKit + Speech + Translation

## Permissions & dependencies (quick summary)

`subtitles-rs` (Rust + Tauri)
- **Permissions:** Screen Recording (for system audio capture).
- **Dependencies:** ScreenCaptureKit; local transcription via whisper.cpp (model downloads into `subtitles-rs/models/`); optional OpenAI‑compatible cloud transcription/translation (requires `OPENAI_API_KEY`); Tauri for the overlay UI.
- **Platform:** macOS 15.0+.

`subtitles-swift` (Native Swift)
- **Permissions:** Screen Recording, Speech Recognition (and Dictation/Siri enabled so speech assets can download).
- **Dependencies:** Apple frameworks (ScreenCaptureKit, Speech, Translation). Translation models download on demand for language pairs.
- **Platform:** macOS 15.0+.
