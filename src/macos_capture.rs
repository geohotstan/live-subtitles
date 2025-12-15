use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::Context;
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use screencapturekit::dispatch_queue::{DispatchQueue, DispatchQoS};
use screencapturekit::prelude::*;

pub fn start_macos_system_audio_capture(
    audio_tx: Sender<Vec<f32>>,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::spawn(move || {
        if let Err(err) = capture_thread_main(audio_tx, stop.clone()) {
            tracing::error!("{err:#}");
            stop.store(true, Ordering::Relaxed);
        }
    });
    Ok(handle)
}

fn capture_thread_main(audio_tx: Sender<Vec<f32>>, stop: Arc<AtomicBool>) -> anyhow::Result<()> {
    tracing::info!("starting ScreenCaptureKit system audio capture (requires Screen Recording permission)");

    let content = SCShareableContent::get().map_err(|e| anyhow::anyhow!("{e}")).context(
        "failed to query shareable content (grant Screen Recording permission to this app/Terminal)",
    )?;

    let displays = content.displays();
    let display = displays
        .first()
        .context("no displays found via ScreenCaptureKit")?;

    let filter = SCContentFilter::builder()
        .display(display)
        .exclude_windows(&[])
        .build();

    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_fps(1)
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(2)
        .with_excludes_current_process_audio(true);

    let handler = AudioHandler::new(audio_tx);
    let queue = DispatchQueue::new("subtitles.capture.audio", DispatchQoS::UserInitiated);

    let mut stream = SCStream::new(&filter, &config);
    if stream
        .add_output_handler_with_queue(handler, SCStreamOutputType::Audio, Some(&queue))
        .is_none()
    {
        anyhow::bail!("failed to add audio output handler");
    }

    stream
        .start_capture()
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("failed to start capture")?;

    tracing::info!("capture started");

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }

    tracing::info!("stopping capture");
    let _ = stream.stop_capture();
    Ok(())
}

struct AudioHandler {
    tx: Sender<Vec<f32>>,
    decimator: Mutex<Decimator3>,
    warned_decode_error: AtomicBool,
}

impl AudioHandler {
    fn new(tx: Sender<Vec<f32>>) -> Self {
        Self {
            tx,
            decimator: Mutex::new(Decimator3::new()),
            warned_decode_error: AtomicBool::new(false),
        }
    }
}

impl SCStreamOutputTrait for AudioHandler {
    fn did_output_sample_buffer(&self, sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Audio {
            return;
        }

        let out_16k = match decode_and_resample_16k_mono(&sample_buffer, &self.decimator) {
            Ok(v) => v,
            Err(err) => {
                if !self.warned_decode_error.swap(true, Ordering::Relaxed) {
                    tracing::warn!("audio decode/resample error (suppressing further): {err:#}");
                }
                return;
            }
        };

        if out_16k.is_empty() {
            return;
        }

        let _ = self.tx.try_send(out_16k);
    }
}

fn decode_and_resample_16k_mono(
    sample: &CMSampleBuffer,
    decimator: &Mutex<Decimator3>,
) -> anyhow::Result<Vec<f32>> {
    let fmt = sample
        .format_description()
        .context("missing format description")?;

    let sample_rate = fmt
        .audio_sample_rate()
        .context("missing audio sample rate")? as u32;
    let channels = fmt
        .audio_channel_count()
        .context("missing audio channel count")? as usize;

    if sample_rate != 48_000 {
        anyhow::bail!("unexpected sample rate {sample_rate} (expected 48000)");
    }
    if fmt.audio_is_big_endian() {
        anyhow::bail!("big-endian audio not supported");
    }

    let bits = fmt.audio_bits_per_channel().unwrap_or(32);
    let is_float = fmt.audio_is_float();

    let Some(abl) = sample.audio_buffer_list() else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let mut dec = decimator.lock();

    match (abl.num_buffers(), is_float, bits) {
        (1, true, 32) => {
            let buf = abl.get(0).unwrap();
            match bytemuck::try_cast_slice::<u8, f32>(buf.data()) {
                Ok(floats) => push_interleaved(&mut dec, floats, channels, &mut out),
                Err(_) => {
                    let floats = decode_f32_le(buf.data())?;
                    push_interleaved(&mut dec, &floats, channels, &mut out);
                }
            }
        }
        (1, false, 16) => {
            let buf = abl.get(0).unwrap();
            match bytemuck::try_cast_slice::<u8, i16>(buf.data()) {
                Ok(ints) => push_interleaved_i16(&mut dec, ints, channels, &mut out),
                Err(_) => {
                    let ints = decode_i16_le(buf.data())?;
                    push_interleaved_i16(&mut dec, &ints, channels, &mut out);
                }
            }
        }
        (n, true, 32) if n == channels && channels > 1 => {
            // Planar float32: one buffer per channel.
            let mut chans_owned: Vec<Vec<f32>> = Vec::with_capacity(channels);
            for i in 0..channels {
                let buf = abl.get(i).unwrap();
                let channel = match bytemuck::try_cast_slice::<u8, f32>(buf.data()) {
                    Ok(slice) => slice.to_vec(),
                    Err(_) => decode_f32_le(buf.data())?,
                };
                chans_owned.push(channel);
            }
            let chans: Vec<&[f32]> = chans_owned.iter().map(|v| v.as_slice()).collect();
            push_planar(&mut dec, &chans, &mut out);
        }
        (n, false, 16) if n == channels && channels > 1 => {
            let mut chans_owned: Vec<Vec<i16>> = Vec::with_capacity(channels);
            for i in 0..channels {
                let buf = abl.get(i).unwrap();
                let channel = match bytemuck::try_cast_slice::<u8, i16>(buf.data()) {
                    Ok(slice) => slice.to_vec(),
                    Err(_) => decode_i16_le(buf.data())?,
                };
                chans_owned.push(channel);
            }
            let chans: Vec<&[i16]> = chans_owned.iter().map(|v| v.as_slice()).collect();
            push_planar_i16(&mut dec, &chans, &mut out);
        }
        _ => {
            anyhow::bail!(
                "unsupported audio layout: num_buffers={}, channels={}, float={}, bits={}",
                abl.num_buffers(),
                channels,
                is_float,
                bits
            );
        }
    }

    Ok(out)
}

fn push_interleaved(dec: &mut Decimator3, interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels == 0 {
        return;
    }
    for frame in interleaved.chunks_exact(channels) {
        let mono = if channels == 1 {
            frame[0]
        } else {
            let mut sum = 0.0f32;
            for &s in frame {
                sum += s;
            }
            sum / (channels as f32)
        };
        if let Some(s) = dec.push(mono) {
            out.push(s);
        }
    }
}

fn push_interleaved_i16(
    dec: &mut Decimator3,
    interleaved: &[i16],
    channels: usize,
    out: &mut Vec<f32>,
) {
    if channels == 0 {
        return;
    }
    for frame in interleaved.chunks_exact(channels) {
        let mono = if channels == 1 {
            frame[0] as f32 / 32768.0
        } else {
            let mut sum = 0.0f32;
            for &s in frame {
                sum += s as f32 / 32768.0;
            }
            sum / (channels as f32)
        };
        if let Some(s) = dec.push(mono) {
            out.push(s);
        }
    }
}

fn push_planar(dec: &mut Decimator3, channels: &[&[f32]], out: &mut Vec<f32>) {
    if channels.is_empty() {
        return;
    }
    let len = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    for i in 0..len {
        let mut sum = 0.0f32;
        for ch in channels {
            sum += ch[i];
        }
        let mono = sum / (channels.len() as f32);
        if let Some(s) = dec.push(mono) {
            out.push(s);
        }
    }
}

fn push_planar_i16(dec: &mut Decimator3, channels: &[&[i16]], out: &mut Vec<f32>) {
    if channels.is_empty() {
        return;
    }
    let len = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    for i in 0..len {
        let mut sum = 0.0f32;
        for ch in channels {
            sum += ch[i] as f32 / 32768.0;
        }
        let mono = sum / (channels.len() as f32);
        if let Some(s) = dec.push(mono) {
            out.push(s);
        }
    }
}

fn decode_f32_le(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        anyhow::bail!("float32 buffer size is not a multiple of 4");
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(out)
}

fn decode_i16_le(bytes: &[u8]) -> anyhow::Result<Vec<i16>> {
    if bytes.len() % 2 != 0 {
        anyhow::bail!("i16 buffer size is not a multiple of 2");
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        out.push(i16::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(out)
}

struct Decimator3 {
    phase: u8,
    acc: f32,
}

impl Decimator3 {
    fn new() -> Self {
        Self { phase: 0, acc: 0.0 }
    }

    fn push(&mut self, s: f32) -> Option<f32> {
        self.acc += s;
        self.phase += 1;
        if self.phase == 3 {
            let out = self.acc / 3.0;
            self.phase = 0;
            self.acc = 0.0;
            Some(out)
        } else {
            None
        }
    }
}
