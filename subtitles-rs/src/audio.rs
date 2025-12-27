use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct SegmenterConfig {
    pub sample_rate_hz: u32,
    pub vad_threshold: f32,
    pub vad_end_silence_s: f32,
    pub max_segment_s: f32,
    pub pre_roll_s: f32,
}

pub struct Segmenter {
    cfg: SegmenterConfig,
    frame_size: usize,
    end_silence_frames: usize,
    max_segment_samples: usize,
    pre_roll_samples: usize,

    stash: Vec<f32>,
    stash_pos: usize,

    in_speech: bool,
    silent_frames: usize,
    pre_roll: std::collections::VecDeque<f32>,
    current: Vec<f32>,
}

impl Segmenter {
    pub fn new(cfg: SegmenterConfig) -> Self {
        let frame_dur = Duration::from_millis(20);
        let frame_size = ((cfg.sample_rate_hz as f32) * frame_dur.as_secs_f32()).round() as usize;

        let end_silence_frames =
            ((cfg.vad_end_silence_s / frame_dur.as_secs_f32()).max(1.0)).round() as usize;

        let max_segment_samples = ((cfg.max_segment_s * cfg.sample_rate_hz as f32).max(1.0))
            .round() as usize;
        let pre_roll_samples =
            ((cfg.pre_roll_s * cfg.sample_rate_hz as f32).max(0.0)).round() as usize;

        Self {
            cfg,
            frame_size: frame_size.max(1),
            end_silence_frames,
            max_segment_samples,
            pre_roll_samples,
            stash: Vec::new(),
            stash_pos: 0,
            in_speech: false,
            silent_frames: 0,
            pre_roll: std::collections::VecDeque::new(),
            current: Vec::new(),
        }
    }

    pub fn push_audio(&mut self, audio: &[f32]) -> Vec<Vec<f32>> {
        self.stash.extend_from_slice(audio);

        let mut out = Vec::new();
        while self.stash.len().saturating_sub(self.stash_pos) >= self.frame_size {
            let start = self.stash_pos;
            let end = self.stash_pos + self.frame_size;
            let frame = &self.stash[start..end];
            self.stash_pos = end;

            let rms = rms(frame);
            let is_voice = rms >= self.cfg.vad_threshold;

            if self.in_speech {
                self.current.extend_from_slice(frame);
                if is_voice {
                    self.silent_frames = 0;
                } else {
                    self.silent_frames += 1;
                }

                if self.silent_frames >= self.end_silence_frames
                    || self.current.len() >= self.max_segment_samples
                {
                    out.push(self.flush_segment());
                }
            } else {
                push_pre_roll(&mut self.pre_roll, self.pre_roll_samples, frame);
                if is_voice {
                    self.in_speech = true;
                    self.silent_frames = 0;
                    self.current.extend(self.pre_roll.drain(..));
                }
            }
        }

        // keep stash from growing without bound
        if self.stash_pos > self.frame_size * 128 {
            self.stash.drain(..self.stash_pos);
            self.stash_pos = 0;
        }

        out
    }

    fn flush_segment(&mut self) -> Vec<f32> {
        self.in_speech = false;
        self.silent_frames = 0;
        self.pre_roll.clear();
        std::mem::take(&mut self.current)
    }

}

fn push_pre_roll(
    pre_roll: &mut std::collections::VecDeque<f32>,
    pre_roll_samples: usize,
    frame: &[f32],
) {
    if pre_roll_samples == 0 {
        return;
    }

    for &s in frame {
        pre_roll.push_back(s);
    }
    while pre_roll.len() > pre_roll_samples {
        pre_roll.pop_front();
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }

    let mut sum = 0.0f32;
    for &s in frame {
        sum += s * s;
    }
    (sum / (frame.len() as f32)).sqrt()
}
