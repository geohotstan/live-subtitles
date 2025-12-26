use std::sync::atomic::Ordering;

use subtitles::{
    app::{CaptionEvent, SharedOutputLanguage},
    config::{Cli, OutputLanguage},
    start_engine,
};
use tauri::Emitter;

#[derive(Clone)]
struct AppState {
    output_language: SharedOutputLanguage,
}

#[derive(Clone, serde::Serialize)]
struct ConfigPayload {
    font_size: f32,
    overlay_width_frac: f32,
    output_language: String,
}

#[derive(Clone, serde::Serialize)]
struct CaptionPayload {
    text: String,
    is_final: bool,
    clear: bool,
}

#[tauri::command]
fn set_output_language(language: String, state: tauri::State<AppState>) -> Result<(), String> {
    let lang = match language.trim().to_lowercase().as_str() {
        "english" => OutputLanguage::English,
        "original" => OutputLanguage::Original,
        _ => return Err("unknown output language".into()),
    };
    state.output_language.set(lang);
    Ok(())
}

fn output_language_label(lang: OutputLanguage) -> String {
    match lang {
        OutputLanguage::Original => "original".to_string(),
        OutputLanguage::English => "english".to_string(),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,subtitles=info".into()),
        )
        .init();

    let cli = <Cli as clap::Parser>::parse();
    if cli.no_ui {
        tracing::warn!("--no-ui is ignored in the Tauri app; use the CLI binary for headless output");
    }

    let (caption_tx, caption_rx) = crossbeam_channel::bounded::<CaptionEvent>(64);
    let engine = match start_engine(cli.clone(), caption_tx) {
        Ok(engine) => engine,
        Err(err) => {
            tracing::error!("failed to start engine: {err:#}");
            std::process::exit(1);
        }
    };

    let stop = engine.stop.clone();
    let app_state = AppState {
        output_language: engine.output_language.clone(),
    };

    let config_payload = ConfigPayload {
        font_size: cli.font_size,
        overlay_width_frac: cli.overlay_width_frac,
        output_language: output_language_label(cli.output_language),
    };

    let app_result = tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();
            let _ = handle.emit("config", config_payload.clone());

            std::thread::spawn(move || {
                while let Ok(event) = caption_rx.recv() {
                    let payload = match event {
                        CaptionEvent::Update { text, is_final } => CaptionPayload {
                            text,
                            is_final,
                            clear: false,
                        },
                        CaptionEvent::Clear => CaptionPayload {
                            text: String::new(),
                            is_final: true,
                            clear: true,
                        },
                    };
                    let _ = handle.emit("caption", payload);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_output_language])
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                stop.store(true, Ordering::Relaxed);
            }
        })
        .run(tauri::generate_context!());

    if let Err(err) = app_result {
        tracing::error!("tauri error: {err:#}");
    }

    engine.stop_and_join();
}
