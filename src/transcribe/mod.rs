mod local_whisper;
mod model_download;
mod openai;

pub use local_whisper::WhisperLocalTranscriber;
pub use openai::OpenAiTranscriber;

#[derive(Debug, Clone)]
pub struct TranscriberConfig {
    pub input_language: Option<String>,
    pub output_language: crate::config::OutputLanguage,
    pub is_partial: bool,
}

pub trait Transcriber: Send {
    fn transcribe(&mut self, audio_16k_mono: &[f32], cfg: &TranscriberConfig)
        -> anyhow::Result<String>;
}
