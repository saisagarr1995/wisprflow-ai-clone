//! Microphone capture (cpal), resampling to 16 kHz mono, rolling ring buffer
//! with pre-roll, and (Prompt 4) Silero voice activity detection.
//!
//! Threading model, per design doc §2/§5: the cpal audio callback only pushes
//! raw frames into a lock-free SPSC queue — no locks, no allocation, no
//! logging on the realtime thread. A drain thread owned by `CpalSource`
//! downmixes, resamples, and hands 16 kHz mono chunks to the engine's sink,
//! which appends to the rolling history buffer that sessions snapshot from.

mod engine;
mod fake;
mod resample;
mod ring;
mod source;
mod vad;

pub use engine::AudioEngine;
pub use fake::FakeAudioSource;
pub use resample::Resampler;
pub use ring::RollingBuffer;
pub use source::{list_input_devices, AudioError, AudioSink, AudioSource, CpalSource};
pub use vad::{
    trim_silence, EndPointer, SileroVad, Vad, VadError, SPEECH_THRESHOLD, VAD_FRAME_SAMPLES,
};

/// Everything downstream of capture (VAD, STT) expects this rate, mono, f32.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Samples for a given duration at the target rate.
pub const fn samples_for_ms(ms: u32) -> usize {
    (TARGET_SAMPLE_RATE as usize * ms as usize) / 1000
}
