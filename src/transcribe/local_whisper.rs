use std::path::PathBuf;

use anyhow::Context;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::config::OutputLanguage;
use crate::config::WhisperModelPreset;
use crate::transcribe::model_download::resolve_whisper_model_path;
use crate::transcribe::{Transcriber, TranscriberConfig};

pub struct WhisperLocalTranscriber {
    ctx: WhisperContext,
    n_threads: i32,
}

impl WhisperLocalTranscriber {
    pub fn new(
        model_path: Option<PathBuf>,
        preset: WhisperModelPreset,
    ) -> anyhow::Result<Self> {
        let model_path = resolve_whisper_model_path(model_path, preset)?;
        tracing::info!("loading whisper model: {}", model_path.display());

        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .context("model path is not valid UTF-8")?,
            WhisperContextParameters::default(),
        )
        .context("failed to load whisper model")?;

        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4)
            .clamp(1, 8);

        Ok(Self { ctx, n_threads })
    }
}

impl Transcriber for WhisperLocalTranscriber {
    fn transcribe(
        &mut self,
        audio_16k_mono: &[f32],
        cfg: &TranscriberConfig,
    ) -> anyhow::Result<String> {
        if audio_16k_mono.is_empty() {
            return Ok(String::new());
        }

        let mut state = self.ctx.create_state().context("failed to create state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 0 });

        params.set_n_threads(self.n_threads);
        let translate = cfg.output_language == OutputLanguage::English;
        params.set_translate(translate);
        // In whisper.cpp, setting `detect_language=true` performs language detection *only*
        // and returns early (no transcription). Auto-detection for transcription/translation
        // is done by passing `language=None` or `language="auto"`.
        params.set_language(cfg.input_language.as_deref());
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, audio_16k_mono)
            .context("whisper inference failed")?;

        let mut out = String::new();
        for seg in state.as_iter() {
            let s = seg.to_string();
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(s);
        }
        Ok(out)
    }
}
