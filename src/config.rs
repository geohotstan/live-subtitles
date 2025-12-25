use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Engine {
    /// On-device transcription via whisper.cpp (Metal enabled).
    #[value(name = "local")]
    Local,
    /// Cloud transcription via OpenAI-compatible `/v1/audio/transcriptions`.
    #[value(name = "openai", alias = "open-ai", alias = "open_ai")]
    OpenAI,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputLanguage {
    /// Show subtitles in the original language.
    Original,
    /// Show subtitles in English.
    English,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum WhisperModelPreset {
    Tiny,
    Base,
    Small,
    Medium,
    #[value(name = "large-v3", alias = "largev3", alias = "large_v3")]
    LargeV3,
}

#[derive(Debug, Parser, Clone)]
#[command(name = "subtitles", version, about = "Live subtitles for macOS (Sequoia+)")]
pub struct Cli {
    /// Transcription engine to use.
    #[arg(long, value_enum, default_value_t = Engine::Local)]
    pub engine: Engine,

    /// Input language (e.g. `en`, `zh`, `ja`) or `auto`.
    #[arg(long, alias = "language", default_value = "auto")]
    pub input_language: String,

    /// Output language (can be changed live in the overlay UI).
    #[arg(long, value_enum, default_value_t = OutputLanguage::English)]
    pub output_language: OutputLanguage,

    /// Run without the on-screen overlay (prints transcripts to stdout).
    #[arg(long)]
    pub no_ui: bool,

    /// Enable low-latency streaming partials (local engine only).
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub streaming: bool,

    /// VAD threshold (RMS) for speech detection.
    #[arg(long, default_value_t = 0.012)]
    pub vad_threshold: f32,

    /// How long (seconds) of silence ends a speech segment.
    #[arg(long, default_value_t = 0.6)]
    pub vad_end_silence_s: f32,

    /// Maximum segment length (seconds) before forcing a flush.
    #[arg(long, default_value_t = 20.0)]
    pub max_segment_s: f32,

    /// Pre-roll audio (seconds) kept before speech starts.
    #[arg(long, default_value_t = 0.25)]
    pub pre_roll_s: f32,

    /// Minimum speech duration (ms) before emitting partials/finals.
    #[arg(long, default_value_t = 300)]
    pub min_speech_ms: u64,

    /// How often (ms) to run ASR while speech is active.
    #[arg(long, default_value_t = 350)]
    pub asr_step_ms: u64,

    /// Maximum audio window (seconds) for partial decoding (0 = full segment).
    #[arg(long, default_value_t = 12.0)]
    pub max_window_s: f32,

    /// Partial stability: how many consecutive updates a token must survive to be committed.
    #[arg(long, default_value_t = 2)]
    pub partial_stable_iters: usize,

    /// Local whisper model file path. If omitted, a model will be downloaded.
    #[arg(long)]
    pub whisper_model: Option<PathBuf>,

    /// Local model preset to download when `--whisper-model` is not provided.
    #[arg(long, value_enum, default_value_t = WhisperModelPreset::Medium)]
    pub whisper_model_preset: WhisperModelPreset,

    /// OpenAI API key (or set `OPENAI_API_KEY`).
    #[arg(long, env = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    /// OpenAI model name for `/v1/audio/transcriptions` (default: `whisper-1`).
    #[arg(long, default_value = "whisper-1")]
    pub openai_model: String,

    /// OpenAI-compatible transcription endpoint.
    #[arg(long, default_value = "https://api.openai.com/v1/audio/transcriptions")]
    pub openai_endpoint: String,

    /// OpenAI-compatible translation endpoint (used when output language is English).
    #[arg(long, default_value = "https://api.openai.com/v1/audio/translations")]
    pub openai_translation_endpoint: String,

    /// Overlay font size (UI mode only).
    #[arg(long, default_value_t = 42.0)]
    pub font_size: f32,

    /// Overlay width as a fraction of screen width (0.1 - 1.0).
    #[arg(long, default_value_t = 0.85)]
    pub overlay_width_frac: f32,
}
