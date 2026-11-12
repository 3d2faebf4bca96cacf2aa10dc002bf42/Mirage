//! A deterministic test signal: keeps the voice path verifiable without hardware.

use crate::codec::{FRAME_SAMPLES, SAMPLE_RATE};

/// Generates consecutive 20 ms frames of a sine wave.
pub struct ToneSource {
    frequency_hz: f32,
    amplitude: f32,
    phase: f32,
}

impl ToneSource {
    pub fn new(frequency_hz: f32, amplitude: f32) -> Self {
        Self {
            frequency_hz,
            amplitude: amplitude.clamp(0.0, 1.0),
            phase: 0.0,
        }
    }

    /// 440 Hz at half scale — loud enough to measure, quiet enough not to clip.
    pub fn default_tone() -> Self {
        Self::new(440.0, 0.5)
    }

    /// Produce the next frame of `FRAME_SAMPLES` samples, advancing the phase.
    pub fn next_frame(&mut self) -> Vec<f32> {
        let step = 2.0 * std::f32::consts::PI * self.frequency_hz / SAMPLE_RATE as f32;
        let mut frame = Vec::with_capacity(FRAME_SAMPLES);
        for _ in 0..FRAME_SAMPLES {
            frame.push(self.amplitude * self.phase.sin());
            self.phase += step;
            if self.phase > 2.0 * std::f32::consts::PI {
                self.phase -= 2.0 * std::f32::consts::PI;
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_have_the_expected_size_and_energy() {
        let mut source = ToneSource::default_tone();
        let frame = source.next_frame();
        assert_eq!(frame.len(), FRAME_SAMPLES);

        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        // RMS of a 0.5 amplitude sine is 0.5/sqrt(2) ~= 0.3536.
        assert!((rms - 0.3536).abs() < 0.02, "unexpected tone RMS: {rms}");
    }

    #[test]
    fn consecutive_frames_are_phase_continuous() {
        let mut source = ToneSource::new(1000.0, 1.0);
        let a = source.next_frame();
        let b = source.next_frame();
        // The last sample of frame A must lead into the first sample of frame B
        // without a discontinuity: compare the slope to the in-frame slope.
        let slope_inside = a[1] - a[0];
        let slope_across = b[0] - a[a.len() - 1];
        assert!(
            (slope_inside - slope_across).abs() < 0.02,
            "phase discontinuity: {slope_inside} vs {slope_across}"
        );
    }
}
