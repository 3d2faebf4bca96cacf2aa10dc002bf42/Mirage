//! Cheap linear resampling between device rates and TeamSpeak's 48 kHz.
//!
//! Deliberately simple: voice is 48 kHz mono and the only common mismatch is
//! 44.1 kHz hardware. Acoustic quality has not been established by the unit tests.
//! A future quality comparison could evaluate a polyphase resampler or requesting
//! 48 kHz directly from the device.

/// Stateful mono linear resampler.
pub struct LinearResampler {
    /// Input samples consumed per output sample (`from/to`).
    step: f32,
    /// Fractional position between the previous and the current input sample.
    t: f32,
    last: f32,
    have_last: bool,
}

impl LinearResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            step: from_rate as f32 / to_rate as f32,
            t: 0.0,
            last: 0.0,
            have_last: false,
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < 1e-4
    }

    pub fn reset(&mut self) {
        self.t = 0.0;
        self.have_last = false;
    }

    /// Resample `input`, appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if self.is_passthrough() {
            out.extend_from_slice(input);
            self.last = input[input.len() - 1];
            self.have_last = true;
            return;
        }

        let mut prev = if self.have_last { self.last } else { input[0] };
        let mut idx = 0usize;
        let mut t = self.t;

        loop {
            while t < 1.0 && idx < input.len() {
                out.push(prev + (input[idx] - prev) * t);
                t += self.step;
            }
            if idx >= input.len() {
                break;
            }
            t -= 1.0;
            prev = input[idx];
            idx += 1;
        }

        self.t = t;
        self.last = input[input.len() - 1];
        self.have_last = true;
    }
}

/// Average the channels of an interleaved buffer into mono.
pub fn downmix_to_mono(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(input);
        return;
    }
    for frame in input.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        out.push(sum / channels as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, samples: usize, hz: f32) -> Vec<f32> {
        (0..samples)
            .map(|n| {
                let t = n as f32 / rate as f32;
                0.5 * (2.0 * std::f32::consts::PI * hz * t).sin()
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn passthrough_keeps_the_signal_identical() {
        let input = sine(48_000, 960, 440.0);
        let mut resampler = LinearResampler::new(48_000, 48_000);
        let mut out = Vec::new();
        resampler.process(&input, &mut out);
        assert!(resampler.is_passthrough());
        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);
    }

    #[test]
    fn upsampling_from_44100_produces_the_expected_sample_count_and_energy() {
        let input = sine(44_100, 44_100, 440.0);
        let mut resampler = LinearResampler::new(44_100, 48_000);
        let mut out = Vec::new();
        resampler.process(&input, &mut out);

        // One second in, one second out (within an output sample).
        assert!(
            (out.len() as i64 - 48_000).abs() <= 2,
            "unexpected output length: {}",
            out.len()
        );
        // Energy must be preserved (+-5%).
        let (a, b) = (rms(&input), rms(&out));
        assert!((a - b).abs() / a < 0.05, "rms changed: {a} -> {b}");
    }

    #[test]
    fn chunked_processing_matches_one_shot_processing() {
        let input = sine(44_100, 4_410, 300.0);

        let mut whole = Vec::new();
        LinearResampler::new(44_100, 48_000).process(&input, &mut whole);

        let mut chunked = Vec::new();
        let mut resampler = LinearResampler::new(44_100, 48_000);
        for chunk in input.chunks(441) {
            resampler.process(chunk, &mut chunked);
        }

        assert_eq!(whole.len(), chunked.len());
        assert!(rms(&whole) - rms(&chunked) < 1e-3);
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, 0.0, 0.0, 1.0, 0.5, 0.5];
        let mut mono = Vec::new();
        downmix_to_mono(&stereo, 2, &mut mono);
        assert_eq!(mono, vec![0.5, 0.5, 0.5]);
    }
}
