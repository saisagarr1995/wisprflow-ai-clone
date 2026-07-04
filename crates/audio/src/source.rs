use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::resample::Resampler;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no input device available")]
    NoDevice,
    #[error("input device '{0}' not found")]
    DeviceNotFound(String),
    #[error("audio backend error: {0}")]
    Backend(String),
}

/// A source of 16 kHz mono f32 audio. The sink is invoked with small chunks
/// (~10–30 ms) on a non-realtime thread owned by the source.
pub trait AudioSource: Send {
    fn start(&mut self, sink: Box<dyn FnMut(&[f32]) + Send>) -> Result<(), AudioError>;
    fn stop(&mut self);
}

/// Names of available input devices, for the settings UI.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Real microphone capture. The cpal callback pushes raw interleaved samples
/// into an SPSC ring; a drain thread downmixes + resamples and calls the sink.
pub struct CpalSource {
    device_name: Option<String>,
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    drain: Option<std::thread::JoinHandle<()>>,
}

impl CpalSource {
    /// `device_name: None` selects the system default input device.
    pub fn new(device_name: Option<String>) -> Self {
        Self {
            device_name,
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
            drain: None,
        }
    }

    fn pick_device(&self) -> Result<cpal::Device, AudioError> {
        let host = cpal::default_host();
        match &self.device_name {
            None => host.default_input_device().ok_or(AudioError::NoDevice),
            Some(name) => host
                .input_devices()
                .map_err(|e| AudioError::Backend(e.to_string()))?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| AudioError::DeviceNotFound(name.clone())),
        }
    }
}

impl AudioSource for CpalSource {
    fn start(&mut self, mut sink: Box<dyn FnMut(&[f32]) + Send>) -> Result<(), AudioError> {
        let device = self.pick_device()?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        tracing::info!(
            device = device.name().unwrap_or_default(),
            sample_rate,
            channels,
            "opening input stream"
        );

        // 2 s of raw audio between the realtime callback and the drain thread.
        let (mut producer, mut consumer) =
            rtrb::RingBuffer::<f32>::new((sample_rate as usize * channels * 2).next_power_of_two());

        let err_fn = |e| tracing::error!(error = %e, "input stream error");
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    // Realtime thread: copy only. If the drain thread stalls
                    // and the ring fills, samples are dropped (never block).
                    let n = producer.slots().min(data.len());
                    for &s in &data[..n] {
                        let _ = producer.push(s);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        stream.play().map_err(|e| AudioError::Backend(e.to_string()))?;

        self.running.store(true, Ordering::Release);
        let running = self.running.clone();
        self.drain = Some(std::thread::spawn(move || {
            let mut resampler = Resampler::new(sample_rate, channels);
            let mut raw = Vec::with_capacity(4096);
            while running.load(Ordering::Acquire) {
                raw.clear();
                while raw.len() < 4096 {
                    match consumer.pop() {
                        Ok(s) => raw.push(s),
                        Err(_) => break,
                    }
                }
                if raw.is_empty() {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let mono16k = resampler.process(&raw);
                if !mono16k.is_empty() {
                    sink(&mono16k);
                }
            }
        }));
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.stream = None; // drops (and closes) the cpal stream
        if let Some(handle) = self.drain.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CpalSource {
    fn drop(&mut self) {
        self.stop();
    }
}
