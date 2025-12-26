use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::Context;
use crossbeam_channel::Sender;

use crate::audio::Segmenter;
use crate::config::{Cli, Engine, OutputLanguage};
use crate::macos_capture::start_macos_system_audio_capture;
use crate::streaming::{Stabilizer, StreamingConfig, StreamingEvent, StreamingSegmenter};
use crate::transcribe::{OpenAiTranscriber, Transcriber, TranscriberConfig, WhisperLocalTranscriber};

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

#[derive(Debug, Clone)]
pub enum CaptionEvent {
    Update { text: String, is_final: bool },
    Clear,
}

pub struct EngineHandle {
    pub stop: Arc<AtomicBool>,
    pub output_language: SharedOutputLanguage,
    capture_handle: std::thread::JoinHandle<()>,
    processing_handle: std::thread::JoinHandle<()>,
    transcription_handle: std::thread::JoinHandle<()>,
}

impl EngineHandle {
    pub fn stop_and_join(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.capture_handle.join();
        let _ = self.processing_handle.join();
        let _ = self.transcription_handle.join();
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

fn maybe_send_update(
    caption_tx: &Sender<CaptionEvent>,
    last_caption: &mut String,
    last_final: &mut bool,
    text: String,
    is_final: bool,
) {
    if text != *last_caption || is_final != *last_final {
        *last_caption = text.clone();
        *last_final = is_final;
        if caption_tx
            .try_send(CaptionEvent::Update { text, is_final })
            .is_err()
        {
            tracing::warn!("caption queue full; dropping update");
        }
    }
}

pub fn start_engine(cli: Cli, caption_tx: Sender<CaptionEvent>) -> anyhow::Result<EngineHandle> {
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("This MVP only supports macOS for now.");
    }

    #[cfg(target_os = "macos")]
    {
        let stop = Arc::new(AtomicBool::new(false));
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
                    cli.whisper_threads,
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

        let output_language_for_worker = output_language.clone();
        let stop_transcribe = stop.clone();
        let partial_stable_iters = cli.partial_stable_iters;

        let transcription_handle = std::thread::spawn(move || {
            let mut stabilizer = Stabilizer::new(partial_stable_iters);
            let mut last_caption = String::new();
            let mut last_final = true;

            while !stop_transcribe.load(Ordering::Relaxed) {
                match event_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(mut event) => {
                        // Coalesce queued partials to the newest audio to avoid redundant decode work.
                        if matches!(event, StreamingEvent::Partial(_)) {
                            while let Ok(next) = event_rx.try_recv() {
                                match next {
                                    StreamingEvent::Partial(audio) => {
                                        event = StreamingEvent::Partial(audio);
                                    }
                                    StreamingEvent::Final(audio) => {
                                        event = StreamingEvent::Final(audio);
                                        break;
                                    }
                                    StreamingEvent::Reset => {
                                        event = StreamingEvent::Reset;
                                        break;
                                    }
                                }
                            }
                        }

                        match event {
                            StreamingEvent::Partial(audio) => {
                                let transcribe_cfg = TranscriberConfig {
                                    input_language: input_language.clone(),
                                    output_language: output_language_for_worker.get(),
                                    is_partial: true,
                                };
                                match transcriber.transcribe(&audio, &transcribe_cfg) {
                                    Ok(text) => {
                                        let (committed, partial) = stabilizer.update(&text);
                                        let display =
                                            combine_committed_partial(&committed, &partial);
                                        maybe_send_update(
                                            &caption_tx,
                                            &mut last_caption,
                                            &mut last_final,
                                            display,
                                            false,
                                        );
                                    }
                                    Err(err) => {
                                        tracing::warn!("transcription failed: {err:#}");
                                    }
                                }
                            }
                            StreamingEvent::Final(audio) => {
                                let transcribe_cfg = TranscriberConfig {
                                    input_language: input_language.clone(),
                                    output_language: output_language_for_worker.get(),
                                    is_partial: false,
                                };
                                match transcriber.transcribe(&audio, &transcribe_cfg) {
                                    Ok(text) => {
                                        let final_text = stabilizer.finalize(&text);
                                        if !final_text.trim().is_empty() {
                                            maybe_send_update(
                                                &caption_tx,
                                                &mut last_caption,
                                                &mut last_final,
                                                final_text,
                                                true,
                                            );
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
                                    last_caption.clear();
                                    last_final = true;
                                    let _ = caption_tx.try_send(CaptionEvent::Clear);
                                }
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(EngineHandle {
            stop,
            output_language,
            capture_handle,
            processing_handle,
            transcription_handle,
        })
    }
}

pub fn run_headless(cli: Cli) -> anyhow::Result<()> {
    if !cli.no_ui {
        anyhow::bail!(
            "The overlay UI is now provided by the Tauri app. Run the Tauri frontend or pass --no-ui for headless output."
        );
    }

    let (caption_tx, caption_rx) = crossbeam_channel::bounded::<CaptionEvent>(64);
    let engine = start_engine(cli, caption_tx)?;
    let stop = engine.stop.clone();

    let stop_for_handler = stop.clone();
    ctrlc::set_handler(move || {
        stop_for_handler.store(true, Ordering::Relaxed);
    })
    .context("failed to set Ctrl-C handler")?;

    while !stop.load(Ordering::Relaxed) {
        match caption_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(CaptionEvent::Update { text, is_final }) => {
                if is_final && !text.trim().is_empty() {
                    println!("{text}");
                }
            }
            Ok(CaptionEvent::Clear) => {}
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    engine.stop_and_join();
    Ok(())
}
