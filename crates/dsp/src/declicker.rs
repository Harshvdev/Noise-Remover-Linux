//! Adaptive Sample-Accurate Impulse De-Clicker and De-Jitter Engine.
//!
//! Detects and repairs impulsive audio artifacts such as:
//! - USB audio packet jitter / dropouts
//! - Digital clock synchronization clicks / pops
//! - Static/electrostatic discharge ticks
//! - Microphone switch / handling clicks and mouth salivation clicks
//!
//! Preserves wideband speech timbre and legitimate consonant transients
//! by using local curvature analysis coupled with cubic Hermite spline reconstruction.

/// Configuration parameters for the impulse de-clicker.
#[derive(Debug, Clone)]
pub struct DeclickerConfig {
    /// Curvature sensitivity threshold in standard deviations above local energy (default: 6.5).
    pub sensitivity_sigma: f32,
    /// Absolute minimum curvature magnitude to consider as a click (default: 0.0035).
    /// Prevents triggering on low-level quantization noise in near-silence.
    pub min_click_magnitude: f32,
    /// Maximum contiguous click burst length in samples to repair (default: 12 = 0.25 ms at 48 kHz).
    /// Ensures natural consonant plosives ('t', 'p', 'k') are never smoothed out.
    pub max_click_duration_samples: usize,
}

impl Default for DeclickerConfig {
    fn default() -> Self {
        Self {
            sensitivity_sigma: 6.5,
            min_click_magnitude: 0.0035,
            max_click_duration_samples: 12,
        }
    }
}

/// Statistics and diagnostics of a de-clicking pass.
#[derive(Debug, Clone, Default)]
pub struct DeclickReport {
    /// Number of distinct click events repaired.
    pub clicks_repaired: usize,
    /// Total number of individual samples reconstructed.
    pub samples_interpolated: usize,
    /// Peak magnitude of the largest repaired click transient.
    pub max_click_magnitude: f32,
}

/// Time-domain adaptive impulse de-clicker.
pub struct Declicker {
    config: DeclickerConfig,
}

impl Declicker {
    /// Create a new de-clicker with the provided configuration.
    pub fn new(config: DeclickerConfig) -> Self {
        Self { config }
    }

    /// Create a default de-clicker optimized for 48 kHz vocal recording.
    pub fn default_48k() -> Self {
        Self::new(DeclickerConfig::default())
    }

    /// Process a slice of audio samples in-place, repairing impulse clicks.
    /// Returns a diagnostic report detailing click repairs.
    #[allow(clippy::needless_range_loop)]
    pub fn process_in_place(&self, samples: &mut [f32]) -> DeclickReport {
        let n = samples.len();
        if n < 8 {
            return DeclickReport::default();
        }

        let mut report = DeclickReport::default();

        // 1. Compute curvature (second difference approximation):
        // d[i] = samples[i] - 0.5 * (samples[i - 1] + samples[i + 1])
        let mut curvature = vec![0.0f32; n];
        for i in 1..(n - 1) {
            curvature[i] = samples[i] - 0.5 * (samples[i - 1] + samples[i + 1]);
        }

        // 2. Compute local running variance of curvature using a fast sliding window (~256 samples)
        let win = 256;
        let mut running_var = vec![1e-6f32; n];
        let mut cur_sum_sq = 0.0f32;

        let initial_len = win.min(n);
        for i in 0..initial_len {
            cur_sum_sq += curvature[i] * curvature[i];
        }
        let half_win = win / 2;

        for i in 0..n {
            let add_idx = i + half_win;
            if add_idx < n {
                cur_sum_sq += curvature[add_idx] * curvature[add_idx];
            }
            if i >= half_win {
                let rem_idx = i - half_win;
                cur_sum_sq -= curvature[rem_idx] * curvature[rem_idx];
            }
            let count = (add_idx.min(n - 1) - i.saturating_sub(half_win) + 1).max(1);
            running_var[i] = (cur_sum_sq / count as f32).max(1e-8);
        }

        // 3. Detect click flags
        let mut is_click = vec![false; n];
        for i in 2..(n - 2) {
            let c_abs = curvature[i].abs();
            let local_sigma = running_var[i].sqrt();
            let threshold = (self.config.sensitivity_sigma * local_sigma)
                .max(self.config.min_click_magnitude);

            if c_abs > threshold {
                is_click[i] = true;
            }
        }

        // 4. Group adjacent click flags into contiguous click bursts and interpolate
        let mut i = 1;
        while i < n - 1 {
            if is_click[i] {
                let start = i;
                while i < n - 1 && is_click[i] {
                    i += 1;
                }
                let end = i; // exclusive
                let burst_len = end - start;

                if burst_len <= self.config.max_click_duration_samples {
                    // Valid impulse click burst: reconstruct via cubic Hermite spline interpolation
                    let p0_idx = start.saturating_sub(1);
                    let p1_idx = end.min(n - 1);

                    let p0 = samples[p0_idx];
                    let p1 = samples[p1_idx];

                    // Approximate boundary derivatives using clean outer samples
                    let m0 = if p0_idx >= 2 {
                        0.5 * (p0 - samples[p0_idx - 2])
                    } else if p0_idx >= 1 {
                        p0 - samples[p0_idx - 1]
                    } else {
                        0.0
                    };

                    let m1 = if p1_idx + 2 < n {
                        0.5 * (samples[p1_idx + 2] - p1)
                    } else if p1_idx + 1 < n {
                        samples[p1_idx + 1] - p1
                    } else {
                        0.0
                    };

                    let total_steps = (p1_idx - p0_idx) as f32;

                    for k in start..end {
                        let t = (k - p0_idx) as f32 / total_steps;
                        let t2 = t * t;
                        let t3 = t2 * t;

                        // Cubic Hermite basis functions
                        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                        let h10 = t3 - 2.0 * t2 + t;
                        let h01 = -2.0 * t3 + 3.0 * t2;
                        let h11 = t3 - t2;

                        let interpolated =
                            h00 * p0 + h10 * total_steps * m0 + h01 * p1 + h11 * total_steps * m1;

                        let orig_val = samples[k];
                        let click_mag = (orig_val - interpolated).abs();
                        report.max_click_magnitude = report.max_click_magnitude.max(click_mag);

                        samples[k] = interpolated;
                        report.samples_interpolated += 1;
                    }

                    report.clicks_repaired += 1;
                }
            } else {
                i += 1;
            }
        }

        report
    }

    /// Process a buffer of audio samples, returning a newly allocated cleaned vector.
    pub fn process(&self, samples: &[f32]) -> (Vec<f32>, DeclickReport) {
        let mut out = samples.to_vec();
        let report = self.process_in_place(&mut out);
        (out, report)
    }
}

/// Helper function to apply de-clicking to a buffer using default 48 kHz parameters.
pub fn apply_declicker(samples: &[f32]) -> (Vec<f32>, DeclickReport) {
    let declicker = Declicker::default_48k();
    declicker.process(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impulse_click_removal() {
        let sample_rate = 48000.0f32;
        let mut signal = vec![0.0f32; 4800];

        // 440 Hz test tone
        for (i, s) in signal.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * 440.0 * (i as f32) / sample_rate).sin() * 0.1;
        }

        // Inject 3 distinct sharp impulse clicks
        signal[1000] += 0.25; // Large click
        signal[2000] -= 0.18; // Negative click
        signal[3000] += 0.20; // Another click
        signal[3001] += 0.15; // 2-sample click burst

        let declicker = Declicker::default_48k();
        let (cleaned, report) = declicker.process(&signal);

        assert!(
            report.clicks_repaired >= 3,
            "Expected at least 3 clicks repaired, found {}",
            report.clicks_repaired
        );
        assert!(
            report.samples_interpolated >= 4,
            "Expected at least 4 samples interpolated, found {}",
            report.samples_interpolated
        );

        // Verify the spike at 1000 is removed and smooth
        let diff_1000 = (cleaned[1000] - (2.0 * std::f32::consts::PI * 440.0 * 1000.0 / sample_rate).sin() * 0.1).abs();
        assert!(
            diff_1000 < 0.03,
            "Click at 1000 was not smoothly repaired: deviation={}",
            diff_1000
        );
    }

    #[test]
    fn test_clean_audio_preservation() {
        let sample_rate = 48000.0f32;
        let mut signal = vec![0.0f32; 4800];

        // Clean sine wave
        for (i, s) in signal.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * 1000.0 * (i as f32) / sample_rate).sin() * 0.2;
        }

        let declicker = Declicker::default_48k();
        let (cleaned, report) = declicker.process(&signal);

        assert_eq!(
            report.clicks_repaired, 0,
            "Clean sine should have 0 false click detections"
        );
        for (orig, clean) in signal.iter().zip(cleaned.iter()) {
            assert_eq!(orig, clean, "Clean signal must not be altered");
        }
    }
}
