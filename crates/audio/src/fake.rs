use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::source::{AudioError, AudioSource};
use crate::TARGET_SAMPLE_RATE;

/// Test/dev source that feeds pre-recorded 16 kHz mono samples to the sink in
/// 20 ms chunks. `realtime: false` delivers as fast as possible (unit tests);
/// `realtime: true` paces at wall-clock speed (manual pipeline rehearsal).
pub struct FakeAudioSource {
    samples: Vec<f32>,
    realtime: bool,
    /// When set, keeps delivering silence after samples are exhausted, so the
    /// engine behaves like a continuously-open microphone.
    loop_silence: bool,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeAudioSource {
    pub fn new(samples: Vec<f32>) -> Self {
        Self {
            samples,
            realtime: false,
            loop_silence: true,
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    pub fn realtime(mut self, on: bool) -> Self {
        self.realtime = on;
        self
    }

    pub fn loop_silence(mut self, on: bool) -> Self {
        self.loop_silence = on;
        self
    }

    /// Load a WAV file; must be mono 16 kHz, 16-bit PCM or f32.
    pub fn from_wav(path: &Path) -> Result<Self, AudioError> {
        let mut reader = hound::WavReader::open(path)
            .map_err(|e| AudioError::Backend(format!("wav open: {e}")))?;
        let spec = reader.spec();
        if spec.channels != 1 || spec.sample_rate != TARGET_SAMPLE_RATE {
            return Err(AudioError::Backend(format!(
                "fixture must be mono {TARGET_SAMPLE_RATE} Hz, got {} ch {} Hz",
                spec.channels, spec.sample_rate
            )));
        }
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => reader
                .samples::<i16>()
                .map(|s| s.unwrap() as f32 / i16::MAX as f32)
                .collect(),
        };
        Ok(Self::new(samples))
    }
}

impl AudioSource for FakeAudioSource {
    fn start(&mut self, mut sink: Box<dyn FnMut(&[f32]) + Send>) -> Result<(), AudioError> {
        const CHUNK: usize = (TARGET_SAMPLE_RATE as usize) / 50; // 20 ms
        let samples = self.samples.clone();
        let realtime = self.realtime;
        let loop_silence = self.loop_silence;
        self.running.store(true, Ordering::Release);
        let running = self.running.clone();

        self.thread = Some(std::thread::spawn(move || {
            let silence = [0.0f32; CHUNK];
            let mut offset = 0;
            while running.load(Ordering::Acquire) {
                if offset < samples.len() {
                    let end = (offset + CHUNK).min(samples.len());
                    sink(&samples[offset..end]);
                    offset = end;
                } else if loop_silence {
                    sink(&silence);
                } else {
                    break;
                }
                if realtime {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }));
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
