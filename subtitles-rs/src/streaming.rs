use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct StreamingConfig {
    pub sample_rate_hz: u32,
    pub vad_threshold: f32,
    pub vad_end_silence_s: f32,
    pub max_segment_s: f32,
    pub pre_roll_s: f32,
    pub min_speech_ms: u64,
    pub asr_step_ms: u64,
    pub max_window_s: f32,
}

#[derive(Debug)]
pub enum StreamingEvent {
    Partial(Vec<f32>),
    Final(Vec<f32>),
    Reset,
}

pub struct StreamingSegmenter {
    cfg: StreamingConfig,
    frame_size: usize,
    end_silence_frames: usize,
    min_speech_samples: usize,
    max_segment_samples: usize,
    pre_roll_samples: usize,
    asr_step_samples: usize,
    max_window_samples: usize,

    stash: Vec<f32>,
    stash_pos: usize,

    in_speech: bool,
    silent_frames: usize,
    pre_roll: VecDeque<f32>,
    utterance: Vec<f32>,
    last_asr_samples: usize,
}

impl StreamingSegmenter {
    pub fn new(cfg: StreamingConfig) -> Self {
        let frame_dur = Duration::from_millis(20);
        let frame_size = ((cfg.sample_rate_hz as f32) * frame_dur.as_secs_f32()).round() as usize;

        let end_silence_frames =
            ((cfg.vad_end_silence_s / frame_dur.as_secs_f32()).max(1.0)).round() as usize;

        let max_segment_samples = ((cfg.max_segment_s * cfg.sample_rate_hz as f32).max(1.0))
            .round() as usize;
        let pre_roll_samples =
            ((cfg.pre_roll_s * cfg.sample_rate_hz as f32).max(0.0)).round() as usize;

        let min_speech_samples =
            ((cfg.min_speech_ms as f32 / 1000.0) * cfg.sample_rate_hz as f32)
                .round()
                .max(1.0) as usize;

        let asr_step_samples = ((cfg.asr_step_ms as f32 / 1000.0) * cfg.sample_rate_hz as f32)
            .round()
            .max(1.0) as usize;

        let mut max_window_samples =
            ((cfg.max_window_s * cfg.sample_rate_hz as f32).max(0.0)).round() as usize;
        if max_window_samples == 0 {
            max_window_samples = max_segment_samples;
        }
        max_window_samples = max_window_samples.min(max_segment_samples);

        Self {
            cfg,
            frame_size: frame_size.max(1),
            end_silence_frames,
            min_speech_samples,
            max_segment_samples,
            pre_roll_samples,
            asr_step_samples,
            max_window_samples,
            stash: Vec::new(),
            stash_pos: 0,
            in_speech: false,
            silent_frames: 0,
            pre_roll: VecDeque::new(),
            utterance: Vec::new(),
            last_asr_samples: 0,
        }
    }

    pub fn push_audio(&mut self, audio: &[f32]) -> Vec<StreamingEvent> {
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
                self.utterance.extend_from_slice(frame);
                if is_voice {
                    self.silent_frames = 0;
                } else {
                    self.silent_frames += 1;
                }

                let reached_silence = self.silent_frames >= self.end_silence_frames;
                let reached_max = self.utterance.len() >= self.max_segment_samples;

                if reached_silence || reached_max {
                    if self.utterance.len() >= self.min_speech_samples {
                        out.push(StreamingEvent::Final(self.flush_utterance()));
                    } else {
                        self.reset_state();
                        out.push(StreamingEvent::Reset);
                    }
                    continue;
                }

                if self.utterance.len() >= self.min_speech_samples
                    && self.utterance.len().saturating_sub(self.last_asr_samples)
                        >= self.asr_step_samples
                {
                    self.last_asr_samples = self.utterance.len();
                    out.push(StreamingEvent::Partial(self.window_audio()));
                }
            } else {
                push_pre_roll(&mut self.pre_roll, self.pre_roll_samples, frame);
                if is_voice {
                    self.in_speech = true;
                    self.silent_frames = 0;
                    self.last_asr_samples = 0;
                    self.utterance.extend(self.pre_roll.drain(..));
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

    fn flush_utterance(&mut self) -> Vec<f32> {
        self.in_speech = false;
        self.silent_frames = 0;
        self.pre_roll.clear();
        self.last_asr_samples = 0;
        std::mem::take(&mut self.utterance)
    }

    fn reset_state(&mut self) {
        self.in_speech = false;
        self.silent_frames = 0;
        self.pre_roll.clear();
        self.last_asr_samples = 0;
        self.utterance.clear();
    }

    fn window_audio(&self) -> Vec<f32> {
        if self.utterance.is_empty() {
            return Vec::new();
        }
        let keep = self.max_window_samples.min(self.utterance.len());
        let start = self.utterance.len().saturating_sub(keep);
        self.utterance[start..].to_vec()
    }
}

pub struct Stabilizer {
    stable_required: usize,
    committed: Vec<String>,
    pending_prev: Vec<String>,
    pending_counts: Vec<usize>,
}

impl Stabilizer {
    pub fn new(stable_required: usize) -> Self {
        Self {
            stable_required: stable_required.max(1),
            committed: Vec::new(),
            pending_prev: Vec::new(),
            pending_counts: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.committed.clear();
        self.pending_prev.clear();
        self.pending_counts.clear();
    }

    pub fn update(&mut self, hypothesis: &str) -> (String, String) {
        let tokens = tokenize(hypothesis);
        if tokens.is_empty() {
            return (tokens_to_text(&self.committed), String::new());
        }

        let mut pending = strip_committed_overlap(&self.committed, &tokens);
        let lcp = lcp_len(&self.pending_prev, &pending);

        let mut counts = Vec::with_capacity(pending.len());
        for i in 0..pending.len() {
            if i < lcp {
                counts.push(self.pending_counts.get(i).copied().unwrap_or(0) + 1);
            } else {
                counts.push(1);
            }
        }

        let mut commit_len = 0usize;
        for &count in &counts {
            if count >= self.stable_required {
                commit_len += 1;
            } else {
                break;
            }
        }

        if commit_len > 0 {
            self.committed
                .extend(pending.iter().take(commit_len).cloned());
            pending = pending[commit_len..].to_vec();
            counts = counts[commit_len..].to_vec();
        }

        self.pending_prev = pending.clone();
        self.pending_counts = counts;

        (tokens_to_text(&self.committed), tokens_to_text(&pending))
    }

    pub fn finalize(&mut self, hypothesis: &str) -> String {
        let tokens = tokenize(hypothesis);
        let text = tokens_to_text(&tokens);
        self.reset();
        text
    }
}

fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace().map(|s| s.to_string()).collect()
}

fn tokens_to_text(tokens: &[String]) -> String {
    if tokens.is_empty() {
        String::new()
    } else {
        tokens.join(" ")
    }
}

fn strip_committed_overlap(committed: &[String], tokens: &[String]) -> Vec<String> {
    if committed.is_empty() || tokens.is_empty() {
        return tokens.to_vec();
    }

    let max_overlap = committed.len().min(tokens.len());
    let mut overlap = 0usize;
    for k in (1..=max_overlap).rev() {
        if committed[committed.len() - k..] == tokens[..k] {
            overlap = k;
            break;
        }
    }

    tokens[overlap..].to_vec()
}

fn lcp_len(a: &[String], b: &[String]) -> usize {
    let mut n = 0usize;
    let len = a.len().min(b.len());
    while n < len && a[n] == b[n] {
        n += 1;
    }
    n
}

fn push_pre_roll(pre_roll: &mut VecDeque<f32>, pre_roll_samples: usize, frame: &[f32]) {
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
