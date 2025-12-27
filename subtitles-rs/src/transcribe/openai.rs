use std::io::Cursor;
use std::time::Duration;

use anyhow::Context;
use reqwest::blocking::multipart;
use serde::Deserialize;

use crate::config::OutputLanguage;
use crate::transcribe::{Transcriber, TranscriberConfig};

pub struct OpenAiTranscriber {
    api_key: String,
    model: String,
    transcription_endpoint: String,
    translation_endpoint: String,
    client: reqwest::blocking::Client,
}

impl OpenAiTranscriber {
    pub fn new(
        api_key: Option<String>,
        model: String,
        transcription_endpoint: String,
        translation_endpoint: String,
    ) -> anyhow::Result<Self> {
        let api_key = api_key.context("missing OpenAI API key (set --openai-api-key or OPENAI_API_KEY)")?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("subtitles/0.1")
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            api_key,
            model,
            transcription_endpoint,
            translation_endpoint,
            client,
        })
    }
}

impl Transcriber for OpenAiTranscriber {
    fn transcribe(
        &mut self,
        audio_16k_mono: &[f32],
        cfg: &TranscriberConfig,
    ) -> anyhow::Result<String> {
        if audio_16k_mono.is_empty() {
            return Ok(String::new());
        }

        let wav = encode_wav_16k_mono_i16(audio_16k_mono)?;

        let file_part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .context("invalid mime")?;

        let translate = cfg.output_language == OutputLanguage::English;
        let endpoint = if translate {
            &self.translation_endpoint
        } else {
            &self.transcription_endpoint
        };

        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .part("file", file_part);

        if let Some(lang) = cfg.input_language.as_ref() {
            form = form.text("language", lang.clone());
        }

        let resp = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .with_context(|| format!("POST {}", endpoint))?;

        let status = resp.status();
        let body = resp.text().context("failed to read response body")?;
        if !status.is_success() {
            anyhow::bail!("transcription API error ({status}): {body}");
        }

        let parsed: OpenAiTranscriptionResponse =
            serde_json::from_str(&body).context("failed to parse transcription response")?;
        Ok(parsed.text)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiTranscriptionResponse {
    text: String,
}

fn encode_wav_16k_mono_i16(audio_16k_mono: &[f32]) -> anyhow::Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let mut writer =
            hound::WavWriter::new(cursor, spec).context("failed to create WAV writer")?;

        for &s in audio_16k_mono {
            let s = s.clamp(-1.0, 1.0);
            let v = (s * i16::MAX as f32) as i16;
            writer
                .write_sample(v)
                .context("failed writing WAV sample")?;
        }
        writer.finalize().context("failed finalizing WAV")?;
    }
    Ok(bytes)
}
