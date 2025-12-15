use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

use crate::config::WhisperModelPreset;

pub fn resolve_whisper_model_path(
    explicit_path: Option<PathBuf>,
    preset: WhisperModelPreset,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit_path {
        return Ok(path);
    }

    let (filename, url) = match preset {
        WhisperModelPreset::Tiny => (
            "ggml-tiny.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        ),
        WhisperModelPreset::Base => (
            "ggml-base.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        ),
        WhisperModelPreset::Small => (
            "ggml-small.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        ),
    };

    let model_dir = PathBuf::from("models");
    fs::create_dir_all(&model_dir).context("failed to create models/ directory")?;
    let model_path = model_dir.join(filename);

    if model_path.exists() {
        return Ok(model_path);
    }

    tracing::info!(
        "downloading whisper model ({}) to {}",
        filename,
        model_path.display()
    );
    download_file(url, &model_path).with_context(|| format!("failed to download model from {url}"))?;
    Ok(model_path)
}

fn download_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60 * 30))
        .user_agent("subtitles/0.1")
        .build()
        .context("failed to build HTTP client")?;

    let mut resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} returned error"))?;

    let tmp_path = dest.with_extension("download");
    let mut tmp = fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create temp file {}", tmp_path.display()))?;

    io::copy(&mut resp, &mut tmp).context("failed downloading model file")?;

    tmp.flush().ok();
    fs::rename(&tmp_path, dest).with_context(|| {
        format!(
            "failed to move {} to {}",
            tmp_path.display(),
            dest.display()
        )
    })?;
    Ok(())
}
