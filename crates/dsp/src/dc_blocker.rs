//! Infrasonic DC blocker filter.
//!
//! Implements a 1-pole recursive high-pass filter:
//!   y[n] = x[n] - x[n-1] + R * y[n-1]
//!
//! With R = 0.995 at 48 kHz, the cutoff frequency is approximately:
//!   fc = (1 - R) * fs / (2 * pi) ~= 38.2 Hz
//!
//! This removes unwanted DC bias and subsonic air turbulence/blade flutter
//! without touching the fundamental frequencies of speech and singing (> 70 Hz).

/// 1-pole recursive DC blocker filter.
#[derive(Debug, Clone)]
pub struct DcBlocker {
    r: f32,
    prev_in: f32,
    prev_out: f32,
}

impl DcBlocker {
    /// Create a new DC blocker with a specified pole radius R (e.g. 0.995).
    pub fn new(r: f32) -> Self {
        Self {
            r: r.clamp(0.90, 0.9999),
            prev_in: 0.0,
            prev_out: 0.0,
        }
    }

    /// Default DC blocker tuned for 48 kHz voice audio (R = 0.995, fc ~ 38.2 Hz).
    pub fn default_48k() -> Self {
        Self::new(0.995)
    }

    /// Reset filter memory.
    pub fn reset(&mut self) {
        self.prev_in = 0.0;
        self.prev_out = 0.0;
    }

    /// Process a single audio sample in-place.
    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let y = x - self.prev_in + self.r * self.prev_out;
        self.prev_in = x;
        self.prev_out = y;
        y
    }

    /// Process an entire slice of samples in-place.
    pub fn process_slice(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

/// Helper function to apply DC blocking to a buffer, returning a new allocated vector.
pub fn apply_dc_blocker(samples: &[f32], r: f32) -> Vec<f32> {
    let mut blocker = DcBlocker::new(r);
    let mut out = samples.to_vec();
    blocker.process_slice(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dc_offset_elimination() {
        let mut blocker = DcBlocker::default_48k();
        // Constant DC offset of +0.5 over 1 second (48000 samples)
        let mut signal = vec![0.5f32; 48000];
        blocker.process_slice(&mut signal);

        // After initial transient settling (e.g. 5000 samples ~ 100ms),
        // the DC offset must be suppressed to near zero.
        let tail = &signal[10000..];
        let max_tail = tail.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_tail < 0.001,
            "DC offset should be eliminated, but tail max was {}",
            max_tail
        );
    }

    #[test]
    fn test_passband_transparency_at_1khz() {
        let mut blocker = DcBlocker::default_48k();
        let sample_rate = 48000.0f32;
        let freq = 1000.0f32;
        let num_samples = 48000;

        let mut signal: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * std::f32::consts::PI * freq * (i as f32) / sample_rate).sin())
            .collect();

        blocker.process_slice(&mut signal);

        // Check steady-state amplitude in the second half
        let tail = &signal[24000..];
        let peak = tail.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let attenuation_db = 20.0 * peak.log10().abs();
        assert!(
            attenuation_db < 0.05,
            "1 kHz passband must be essentially untouched (attenuation: {:.3} dB)",
            attenuation_db
        );
    }
}
