use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::ring::RollingBuffer;
use crate::source::{AudioError, AudioSource};
use crate::{samples_for_ms, TARGET_SAMPLE_RATE};

/// Seconds of rolling history kept for session snapshots (max utterance).
const HISTORY_SECS: usize = 30;

/// Owns continuous capture into a rolling buffer and cuts per-dictation
/// sessions out of it. Capture runs from app start (design doc §2 stage 1:
/// opening a stream cold costs 100–300 ms we can't afford on key-down), so a
/// session is just a pair of absolute sample indices into the history.
pub struct AudioEngine {
    source: Box<dyn AudioSource>,
    history: Arc<Mutex<RollingBuffer>>,
    /// Latest chunk RMS, stored as f32 bits for the UI level meter.
    level_bits: Arc<AtomicU32>,
    session_start: Option<u64>,
}

impl AudioEngine {
    pub fn new(source: Box<dyn AudioSource>) -> Self {
        Self {
            source,
            history: Arc::new(Mutex::new(RollingBuffer::new(
                TARGET_SAMPLE_RATE as usize * HISTORY_SECS,
            ))),
            level_bits: Arc::new(AtomicU32::new(0)),
            session_start: None,
        }
    }

    /// Begin continuous capture. Idempotent per engine instance.
    pub fn start(&mut self) -> Result<(), AudioError> {
        let history = self.history.clone();
        let level_bits = self.level_bits.clone();
        self.source.start(Box::new(move |chunk: &[f32]| {
            history.lock().expect("history lock").push(chunk);
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len().max(1) as f32).sqrt();
            level_bits.store(rms.to_bits(), Ordering::Relaxed);
        }))
    }

    pub fn stop(&mut self) {
        self.source.stop();
    }

    /// Mark session start at now minus pre-roll (clamped to retained audio).
    pub fn begin_session(&mut self, preroll_ms: u32) {
        let head = self.history.lock().expect("history lock").head();
        self.session_start = Some(head.saturating_sub(samples_for_ms(preroll_ms) as u64));
    }

    /// Snapshot everything from the session marker to now. Returns `None` if
    /// no session was begun (e.g. spurious hotkey release).
    pub fn end_session(&mut self) -> Option<Vec<f32>> {
        let start = self.session_start.take()?;
        let history = self.history.lock().expect("history lock");
        Some(history.snapshot(start, history.head()))
    }

    /// Latest input level (RMS of the most recent chunk), for the mic meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    /// Absolute count of samples captured so far (test/diagnostic hook).
    pub fn samples_captured(&self) -> u64 {
        self.history.lock().expect("history lock").head()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeAudioSource;
    use std::time::{Duration, Instant};

    fn wait_for(engine: &AudioEngine, at_least: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.samples_captured() < at_least {
            assert!(Instant::now() < deadline, "timed out waiting for samples");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn session_includes_preroll() {
        // Ramp 0..16000 so sample values encode their absolute index.
        let samples: Vec<f32> = (0..16_000).map(|i| i as f32).collect();
        let n = samples.len() as u64;
        let mut engine =
            AudioEngine::new(Box::new(FakeAudioSource::new(samples).loop_silence(false)));
        engine.start().unwrap();
        wait_for(&engine, n);

        engine.begin_session(250); // 250 ms = 4000 samples of pre-roll
        let audio = engine.end_session().unwrap();
        assert_eq!(audio.len(), 4000);
        assert_eq!(audio[0], 12_000.0);
        assert_eq!(*audio.last().unwrap(), 15_999.0);
    }

    #[test]
    fn end_without_begin_returns_none() {
        let mut engine = AudioEngine::new(Box::new(
            FakeAudioSource::new(vec![0.0; 100]).loop_silence(false),
        ));
        engine.start().unwrap();
        assert!(engine.end_session().is_none());
    }

    #[test]
    fn level_reflects_signal() {
        let loud = vec![1.0f32; 3200];
        let mut engine = AudioEngine::new(Box::new(FakeAudioSource::new(loud).loop_silence(false)));
        engine.start().unwrap();
        wait_for(&engine, 3200);
        assert!(engine.level() > 0.5, "level was {}", engine.level());
        engine.stop();
    }

    #[test]
    fn session_survives_ring_wraparound() {
        // More audio than a tiny window: verify snapshot clamps, not panics.
        let samples: Vec<f32> = (0..32_000).map(|i| i as f32).collect();
        let mut engine =
            AudioEngine::new(Box::new(FakeAudioSource::new(samples).loop_silence(false)));
        engine.start().unwrap();
        engine.begin_session(0);
        wait_for(&engine, 32_000);
        let audio = engine.end_session().unwrap();
        // Marker was at 0; everything still retained (30 s window) came back.
        assert_eq!(audio.len(), 32_000);
    }
}
