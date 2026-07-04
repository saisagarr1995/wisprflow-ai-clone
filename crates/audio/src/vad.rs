use std::ops::Range;
use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::samples_for_ms;

/// Silero v5 operates on 512-sample frames at 16 kHz (32 ms).
pub const VAD_FRAME_SAMPLES: usize = 512;

/// Default speech probability threshold (Silero's recommended 0.5).
pub const SPEECH_THRESHOLD: f32 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum VadError {
    #[error("vad model error: {0}")]
    Model(String),
    #[error("frame must be {VAD_FRAME_SAMPLES} samples, got {0}")]
    BadFrameSize(usize),
}

// ort's Error is generic over a "recovery" payload (e.g. the SessionBuilder
// you can get back after a failed commit); we only care about the message.
impl<T> From<ort::Error<T>> for VadError {
    fn from(e: ort::Error<T>) -> Self {
        VadError::Model(e.to_string())
    }
}

/// Frame-level voice activity detection. Behind a trait so Silero can be
/// swapped (TEN-VAD) and tests can use a canned implementation.
pub trait Vad: Send {
    /// Speech probability (0..1) for one 512-sample 16 kHz mono frame.
    /// Stateful: frames must be fed in order.
    fn speech_prob(&mut self, frame: &[f32]) -> Result<f32, VadError>;
    /// Clear recurrent state between independent audio streams.
    fn reset(&mut self);
}

/// Silero v5's ONNX graph wants each 512-sample window prefixed with the last
/// 64 samples of the previous window (input shape `[1, 576]`). Feeding a bare
/// `[1, 512]` is accepted by the dynamic axis but yields nonsense scores.
const CONTEXT_SAMPLES: usize = 64;

/// Silero VAD v5 via ONNX Runtime. ~1 ms per frame on CPU.
pub struct SileroVad {
    session: Session,
    /// Recurrent state, shape [2, 1, 128].
    state: Vec<f32>,
    /// Trailing 64 samples of the previous frame.
    context: Vec<f32>,
}

impl SileroVad {
    pub fn from_file(model_path: &Path) -> Result<Self, VadError> {
        let session = Session::builder()?
            .with_intra_threads(1)?
            .commit_from_file(model_path)?;
        Ok(Self {
            session,
            state: vec![0.0; 2 * 128],
            context: vec![0.0; CONTEXT_SAMPLES],
        })
    }
}

impl Vad for SileroVad {
    fn speech_prob(&mut self, frame: &[f32]) -> Result<f32, VadError> {
        if frame.len() != VAD_FRAME_SAMPLES {
            return Err(VadError::BadFrameSize(frame.len()));
        }
        let mut window = Vec::with_capacity(CONTEXT_SAMPLES + VAD_FRAME_SAMPLES);
        window.extend_from_slice(&self.context);
        window.extend_from_slice(frame);

        let input = Tensor::from_array(([1usize, window.len()], window))?;
        let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()))?;
        let sr = Tensor::from_array(([1usize], vec![16_000i64]))?;

        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "state" => state, "sr" => sr])?;

        let (_, new_state) = outputs["stateN"].try_extract_tensor::<f32>()?;
        self.state.copy_from_slice(new_state);
        self.context.copy_from_slice(&frame[VAD_FRAME_SAMPLES - CONTEXT_SAMPLES..]);
        let (_, prob) = outputs["output"].try_extract_tensor::<f32>()?;
        Ok(prob[0])
    }

    fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }
}

/// Trim leading/trailing silence off a finished utterance, keeping `pad_ms`
/// of margin on each side. Returns a sample range into `audio`; empty range
/// means no speech was detected at all.
pub fn trim_silence(
    vad: &mut dyn Vad,
    audio: &[f32],
    threshold: f32,
    pad_ms: u32,
) -> Result<Range<usize>, VadError> {
    vad.reset();
    let mut first = None;
    let mut last = None;
    for (i, frame) in audio.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        if vad.speech_prob(frame)? >= threshold {
            first.get_or_insert(i);
            last = Some(i);
        }
    }
    vad.reset();
    let (Some(first), Some(last)) = (first, last) else {
        return Ok(0..0);
    };
    let pad = samples_for_ms(pad_ms);
    let start = (first * VAD_FRAME_SAMPLES).saturating_sub(pad);
    let end = ((last + 1) * VAD_FRAME_SAMPLES + pad).min(audio.len());
    Ok(start..end)
}

/// Hands-free end-pointing: after speech has been observed, `timeout_ms` of
/// consecutive silence fires exactly one `true`. Reset for the next utterance.
pub struct EndPointer {
    silence_frames_needed: usize,
    threshold: f32,
    seen_speech: bool,
    silence_run: usize,
    fired: bool,
}

impl EndPointer {
    pub fn new(timeout_ms: u32, threshold: f32) -> Self {
        let frame_ms = VAD_FRAME_SAMPLES * 1000 / 16_000; // 32 ms
        Self {
            silence_frames_needed: (timeout_ms as usize).div_ceil(frame_ms),
            threshold,
            seen_speech: false,
            silence_run: 0,
            fired: false,
        }
    }

    /// Feed one frame's speech probability; returns `true` exactly once, when
    /// the utterance has ended.
    pub fn on_frame(&mut self, prob: f32) -> bool {
        if self.fired {
            return false;
        }
        if prob >= self.threshold {
            self.seen_speech = true;
            self.silence_run = 0;
            return false;
        }
        if !self.seen_speech {
            return false;
        }
        self.silence_run += 1;
        if self.silence_run >= self.silence_frames_needed {
            self.fired = true;
            return true;
        }
        false
    }

    pub fn reset(&mut self) {
        self.seen_speech = false;
        self.silence_run = 0;
        self.fired = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpointer_fires_once_after_trailing_silence() {
        // 700 ms / 32 ms = 22 frames of silence needed.
        let mut ep = EndPointer::new(700, 0.5);
        // Leading silence never fires.
        for _ in 0..100 {
            assert!(!ep.on_frame(0.0));
        }
        // Speech.
        for _ in 0..10 {
            assert!(!ep.on_frame(0.9));
        }
        // Trailing silence: fires exactly at the 22nd frame, then never again.
        let fires: Vec<bool> = (0..50).map(|_| ep.on_frame(0.05)).collect();
        assert_eq!(fires.iter().filter(|&&f| f).count(), 1);
        assert!(fires[21], "should fire on the 22nd silent frame");

        ep.reset();
        assert!(!ep.on_frame(0.0));
    }

    #[test]
    fn endpointer_speech_resets_silence_run() {
        let mut ep = EndPointer::new(96, 0.5); // 3 frames
        assert!(!ep.on_frame(0.9));
        assert!(!ep.on_frame(0.1));
        assert!(!ep.on_frame(0.1));
        assert!(!ep.on_frame(0.9)); // speech again — run resets
        assert!(!ep.on_frame(0.1));
        assert!(!ep.on_frame(0.1));
        assert!(ep.on_frame(0.1)); // third consecutive silent frame
    }
}
