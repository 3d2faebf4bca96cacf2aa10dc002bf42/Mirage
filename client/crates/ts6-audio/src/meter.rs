//! Level metering — how an automated test proves that audio really arrived.

/// Accumulates samples and reports RMS/peak.
#[derive(Debug, Default, Clone)]
pub struct RmsMeter {
    sum_squares: f64,
    samples: u64,
    peak: f32,
}

impl RmsMeter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, samples: &[f32]) {
        for sample in samples {
            let value = *sample as f64;
            self.sum_squares += value * value;
            self.samples += 1;
            self.peak = self.peak.max(sample.abs());
        }
    }

    pub fn rms(&self) -> f32 {
        if self.samples == 0 {
            return 0.0;
        }
        (self.sum_squares / self.samples as f64).sqrt() as f32
    }

    pub fn peak(&self) -> f32 {
        self.peak
    }

    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Reset the accumulator, returning the measured `(rms, peak, samples)`.
    pub fn take(&mut self) -> (f32, f32, u64) {
        let result = (self.rms(), self.peak, self.samples);
        *self = Self::default();
        result
    }

    /// True when there is actual signal, not digital silence.
    pub fn has_signal(&self, threshold: f32) -> bool {
        self.rms() > threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_measures_zero() {
        let mut meter = RmsMeter::new();
        meter.push(&[0.0; 960]);
        assert_eq!(meter.rms(), 0.0);
        assert!(!meter.has_signal(0.001));
    }

    #[test]
    fn known_sine_measures_expected_rms() {
        let mut meter = RmsMeter::new();
        let frame: Vec<f32> = (0..960)
            .map(|n| {
                let t = n as f32 / 48_000.0;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        meter.push(&frame);
        assert!((meter.rms() - 0.3536).abs() < 0.01, "rms={}", meter.rms());
        assert!(meter.peak() > 0.49);
        assert_eq!(meter.samples(), 960);
    }

    #[test]
    fn take_resets_the_accumulator() {
        let mut meter = RmsMeter::new();
        meter.push(&[0.5; 100]);
        let (rms, peak, samples) = meter.take();
        assert!((rms - 0.5).abs() < 1e-6);
        assert!((peak - 0.5).abs() < 1e-6);
        assert_eq!(samples, 100);
        assert_eq!(meter.samples(), 0);
        assert_eq!(meter.rms(), 0.0);
    }
}
