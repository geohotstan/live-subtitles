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
use crate::streaming::{Stabilizer, StreamingConfig, StreamingEvent, StreamingSegmenter};
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

fn combine_committed_partial(committed: &str, partial: &str) -> String {
    match (committed.trim().is_empty(), partial.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => committed.trim().to_string(),
        (true, false) => partial.trim().to_string(),
        (false, false) => format!("{} {}", committed.trim(), partial.trim()),
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
        let (event_tx, event_rx) = crossbeam_channel::bounded::<StreamingEvent>(32);

        let streaming_enabled = cli.streaming && matches!(cli.engine, Engine::Local);
        if cli.streaming && matches!(cli.engine, Engine::OpenAI) {
            tracing::warn!(
                "streaming partials are disabled for OpenAI engine; use --streaming=false to silence"
            );
        }

        let segmenter_cfg = crate::audio::SegmenterConfig {
            vad_threshold: cli.vad_threshold,
            vad_end_silence_s: cli.vad_end_silence_s,
            max_segment_s: cli.max_segment_s,
            pre_roll_s: cli.pre_roll_s,
            sample_rate_hz: 16_000,
        };

        let streaming_cfg = StreamingConfig {
            sample_rate_hz: 16_000,
            vad_threshold: cli.vad_threshold,
            vad_end_silence_s: cli.vad_end_silence_s,
            max_segment_s: cli.max_segment_s,
            pre_roll_s: cli.pre_roll_s,
            min_speech_ms: cli.min_speech_ms,
            asr_step_ms: cli.asr_step_ms,
            max_window_s: cli.max_window_s,
        };

        let stop_processing = stop.clone();
        let processing_handle = std::thread::spawn(move || {
            if streaming_enabled {
                let mut segmenter = StreamingSegmenter::new(streaming_cfg);
                while !stop_processing.load(Ordering::Relaxed) {
                    match audio_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(chunk) => {
                            for event in segmenter.push_audio(&chunk) {
                                if event_tx.try_send(event).is_err() {
                                    tracing::warn!("segment queue full; dropping event");
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            } else {
                let mut segmenter = Segmenter::new(segmenter_cfg);
                while !stop_processing.load(Ordering::Relaxed) {
                    match audio_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(chunk) => {
                            for segment in segmenter.push_audio(&chunk) {
                                if event_tx
                                    .try_send(StreamingEvent::Final(segment))
                                    .is_err()
                                {
                                    tracing::warn!("segment queue full; dropping segment");
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
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
        let partial_stable_iters = cli.partial_stable_iters;

        let transcription_handle = std::thread::spawn(move || {
            let mut stabilizer = Stabilizer::new(partial_stable_iters);
            let mut last_caption = String::new();

            while !stop_transcribe.load(Ordering::Relaxed) {
                match event_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => {
                        let transcribe_cfg = TranscriberConfig {
                            input_language: input_language.clone(),
                            output_language: output_language_for_worker.get(),
                        };

                        match event {
                            StreamingEvent::Partial(audio) => {
                                match transcriber.transcribe(&audio, &transcribe_cfg) {
                                    Ok(text) => {
                                        let (committed, partial) = stabilizer.update(&text);
                                        let display = combine_committed_partial(&committed, &partial);
                                        if display != last_caption {
                                            captions_for_worker.set_text(display.clone());
                                            last_caption = display;
                                        }
                                    }
                                    Err(err) => {
                                        tracing::warn!("transcription failed: {err:#}");
                                    }
                                }
                            }
                            StreamingEvent::Final(audio) => {
                                match transcriber.transcribe(&audio, &transcribe_cfg) {
                                    Ok(text) => {
                                        let final_text = stabilizer.finalize(&text);
                                        if !final_text.trim().is_empty() {
                                            captions_for_worker.set_text(final_text.clone());
                                            last_caption = final_text.clone();
                                            if no_ui {
                                                println!("{final_text}");
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        tracing::warn!("transcription failed: {err:#}");
                                    }
                                }
                            }
                            StreamingEvent::Reset => {
                                stabilizer.reset();
                                if !last_caption.is_empty() {
                                    captions_for_worker.set_text(String::new());
                                    last_caption.clear();
                                }
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
