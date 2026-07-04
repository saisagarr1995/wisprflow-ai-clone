use rubato::{Resampler as _, SincFixedIn, SincInterpolationParameters, SincInterpolationType};

use crate::TARGET_SAMPLE_RATE;

/// Converts interleaved multi-channel audio at a device's native rate into
/// 16 kHz mono. Downmix (channel average) happens before resampling.
pub struct Resampler {
    inner: Option<SincFixedIn<f32>>,
    channels: usize,
    /// Mono samples waiting until a full resampler input chunk accumulates.
    pending: Vec<f32>,
    chunk_in: usize,
}

impl Resampler {
    pub fn new(input_rate: u32, channels: usize) -> Self {
        assert!(channels > 0);
        let inner = if input_rate == TARGET_SAMPLE_RATE {
            None // passthrough after downmix
        } else {
            let params = SincInterpolationParameters {
                sinc_len: 128,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: rubato::WindowFunction::BlackmanHarris2,
            };
            let chunk = (input_rate as usize / 50).max(64); // ~20 ms
            Some(
                SincFixedIn::<f32>::new(
                    TARGET_SAMPLE_RATE as f64 / input_rate as f64,
                    1.0,
                    params,
                    chunk,
                    1,
                )
                .expect("valid resampler config"),
            )
        };
        let chunk_in = inner.as_ref().map(|r| r.input_frames_next()).unwrap_or(0);
        Self { inner, channels, pending: Vec::new(), chunk_in }
    }

    /// Feed interleaved input; returns whatever 16 kHz mono output is ready.
    /// Output lags input by up to one chunk (~20 ms) plus filter delay.
    pub fn process(&mut self, interleaved: &[f32]) -> Vec<f32> {
        // Downmix to mono by averaging channels.
        let mono: Vec<f32> = interleaved
            .chunks_exact(self.channels)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect();

        let Some(resampler) = self.inner.as_mut() else {
            return mono;
        };

        self.pending.extend_from_slice(&mono);
        let mut out = Vec::new();
        while self.pending.len() >= self.chunk_in {
            let input: Vec<f32> = self.pending.drain(..self.chunk_in).collect();
            let result = resampler
                .process(&[input], None)
                .expect("resampler process cannot fail with correct chunk size");
            out.extend_from_slice(&result[0]);
            self.chunk_in = resampler.input_frames_next();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn dominant_freq_by_zero_crossings(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f32 / (samples.len() as f32 / rate as f32)
    }

    #[test]
    fn downsamples_48k_to_16k_preserving_frequency() {
        let mut rs = Resampler::new(48_000, 1);
        let out = rs.process(&sine(48_000, 440.0, 1.0));
        // ~16000 samples out for 1 s in, modulo one internal chunk of latency.
        assert!((14_000..=16_100).contains(&out.len()), "got {} samples", out.len());
        let f = dominant_freq_by_zero_crossings(&out, TARGET_SAMPLE_RATE);
        assert!((f - 440.0).abs() < 15.0, "dominant frequency drifted: {f} Hz");
    }

    #[test]
    fn passthrough_at_16k_downmixes_stereo() {
        let mut rs = Resampler::new(16_000, 2);
        // L = 1.0, R = 0.0 → mono 0.5
        let interleaved: Vec<f32> = [1.0f32, 0.0].repeat(100);
        let out = rs.process(&interleaved);
        assert_eq!(out.len(), 100);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < f32::EPSILON));
    }
}
