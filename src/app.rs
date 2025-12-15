use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;

use crate::audio::Segmenter;
use crate::config::{Cli, Engine, OutputLanguage};
use crate::macos_capture::start_macos_system_audio_capture;
use crate::transcribe::{OpenAiTranscriber, Transcriber, TranscriberConfig, WhisperLocalTranscriber};
use crate::ui::run_overlay;

#[derive(Debug, Clone)]
pub struct SharedCaption {
    inner: Arc<Mutex<CaptionState>>,
}

#[derive(Debug, Clone)]
pub struct SharedOutputLanguage {
    inner: Arc<std::sync::atomic::AtomicU8>,
}

impl SharedOutputLanguage {
    pub fn new(initial: OutputLanguage) -> Self {
        Self {
            inner: Arc::new(std::sync::atomic::AtomicU8::new(initial as u8)),
        }
    }

    pub fn get(&self) -> OutputLanguage {
        match self.inner.load(Ordering::Relaxed) {
            0 => OutputLanguage::Original,
            _ => OutputLanguage::English,
        }
    }

    pub fn set(&self, value: OutputLanguage) {
        self.inner.store(value as u8, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct CaptionState {
    text: String,
    updated_at: std::time::Instant,
}

impl SharedCaption {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CaptionState {
                text: String::new(),
                updated_at: std::time::Instant::now(),
            })),
        }
    }

    pub fn set_text(&self, text: impl Into<String>) {
        let mut guard = self.inner.lock();
        guard.text = text.into();
        guard.updated_at = std::time::Instant::now();
    }

    pub fn snapshot(&self) -> (String, std::time::Instant) {
        let guard = self.inner.lock();
        (guard.text.clone(), guard.updated_at)
    }
}

pub fn run(cli: Cli) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("This MVP only supports macOS for now.");
    }

    #[cfg(target_os = "macos")]
    {
        let stop = Arc::new(AtomicBool::new(false));
        let captions = SharedCaption::new();
        let output_language = SharedOutputLanguage::new(cli.output_language);

        let (audio_tx, audio_rx) = crossbeam_channel::bounded::<Vec<f32>>(256);
        let (segment_tx, segment_rx) = crossbeam_channel::bounded::<Vec<f32>>(16);

        let segmenter_cfg = crate::audio::SegmenterConfig {
            vad_threshold: cli.vad_threshold,
            vad_end_silence_s: cli.vad_end_silence_s,
            max_segment_s: cli.max_segment_s,
            pre_roll_s: cli.pre_roll_s,
            sample_rate_hz: 16_000,
        };

        let stop_processing = stop.clone();
        let processing_handle = std::thread::spawn(move || {
            let mut segmenter = Segmenter::new(segmenter_cfg);
            while !stop_processing.load(Ordering::Relaxed) {
                match audio_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(chunk) => {
                        for segment in segmenter.push_audio(&chunk) {
                            if segment_tx.try_send(segment).is_err() {
                                tracing::warn!("segment queue full; dropping segment");
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        let input_language = if cli.input_language.trim().eq_ignore_ascii_case("auto") {
            None
        } else {
            Some(cli.input_language.trim().to_string())
        };

        let mut transcriber: Box<dyn Transcriber> = match cli.engine.clone() {
            Engine::Local => Box::new(
                WhisperLocalTranscriber::new(
                    cli.whisper_model.clone(),
                    cli.whisper_model_preset.clone(),
                )
                .context("failed to initialize local whisper")?,
            ),
            Engine::OpenAI => Box::new(
                OpenAiTranscriber::new(
                    cli.openai_api_key.clone(),
                    cli.openai_model.clone(),
                    cli.openai_endpoint.clone(),
                    cli.openai_translation_endpoint.clone(),
                )
                .context("failed to initialize OpenAI transcriber")?,
            ),
        };

        let capture_handle = start_macos_system_audio_capture(audio_tx, stop.clone())
            .context("failed to start ScreenCaptureKit audio capture")?;

        let captions_for_worker = captions.clone();
        let output_language_for_worker = output_language.clone();
        let stop_transcribe = stop.clone();
        let no_ui = cli.no_ui;

        let transcription_handle = std::thread::spawn(move || {
            while !stop_transcribe.load(Ordering::Relaxed) {
                match segment_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(segment) => {
                        let transcribe_cfg = TranscriberConfig {
                            input_language: input_language.clone(),
                            output_language: output_language_for_worker.get(),
                        };

                        match transcriber.transcribe(&segment, &transcribe_cfg) {
                        Ok(text) => {
                            if !text.trim().is_empty() {
                                captions_for_worker.set_text(text.clone());
                                if no_ui {
                                    println!("{text}");
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!("transcription failed: {err:#}");
                        }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        if cli.no_ui {
            {
                let stop = stop.clone();
                ctrlc::set_handler(move || {
                    stop.store(true, Ordering::Relaxed);
                })
                .context("failed to set Ctrl-C handler")?;
            }

            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
            }
        } else {
            run_overlay(
                captions.clone(),
                output_language.clone(),
                stop.clone(),
                cli.font_size,
                cli.overlay_width_frac,
            )?;
            stop.store(true, Ordering::Relaxed);
        }

        let _ = capture_handle.join();
        let _ = processing_handle.join();
        let _ = transcription_handle.join();
        Ok(())
    }
}
