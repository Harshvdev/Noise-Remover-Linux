//! High-Q biquad tonal notch filter for mains electrical hum removal.
//!
//! Designed to eliminate persistent narrow hums (e.g. 50 Hz or 60 Hz)
//! detected during noise calibration without affecting adjacent speech/singing frequencies.

use crate::tonal::TonalPeak;

/// 2nd-order IIR Biquad Notch Filter (Direct Form II Transposed).
#[derive(Debug, Clone)]
pub struct BiquadNotch {
    center_freq_hz: f32,
    q: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl BiquadNotch {
    /// Create a new biquad notch filter.
    ///
    /// - `center_freq`: Frequency to notch in Hz (e.g. 50.0).
    /// - `q`: Quality factor (e.g. 25.0 for a narrow ~2 Hz bandwidth).
    /// - `sample_rate`: Audio sampling rate in Hz (e.g. 48000).
    pub fn new(center_freq: f32, q: f32, sample_rate: u32) -> Self {
        let fs = sample_rate as f32;
        let w0 = 2.0 * std::f32::consts::PI * (center_freq / fs).clamp(0.0001, 0.499);
        let alpha = w0.sin() / (2.0 * q.max(1.0));
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        let b0 = 1.0 / a0;
        let b1 = (-2.0 * cos_w0) / a0;
        let b2 = 1.0 / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            center_freq_hz: center_freq,
            q,
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Reset internal filter state.
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Center frequency in Hz.
    pub fn center_freq(&self) -> f32 {
        self.center_freq_hz
    }

    /// Quality factor Q.
    pub fn q(&self) -> f32 {
        self.q
    }

    /// Process a single sample using Transposed Direct Form II.
    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Process an audio slice in-place.
    pub fn process_slice(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

/// Cascaded notch filter targeting multiple confirmed tonal peaks.
#[derive(Debug, Clone)]
pub struct TonalNotchFilter {
    notches: Vec<BiquadNotch>,
}

impl TonalNotchFilter {
    /// Build a cascaded notch filter from detected tonal peaks in the noise profile.
    ///
    /// Only notches peaks that meet strict confidence and prominence thresholds:
    /// - `min_confidence`: e.g. 0.60
    /// - `min_strength_db`: e.g. 10.0 dB
    pub fn from_tonal_peaks(
        peaks: &[TonalPeak],
        sample_rate: u32,
        min_confidence: f32,
        min_strength_db: f32,
    ) -> Self {
        let mut notches = Vec::new();

        for peak in peaks {
            if peak.confidence >= min_confidence && peak.strength_db >= min_strength_db {
                // High Q factor for narrow notch (~2-3 Hz bandwidth) to protect voice
                let q = (peak.frequency_hz / 2.5).clamp(15.0, 40.0);
                notches.push(BiquadNotch::new(peak.frequency_hz, q, sample_rate));
            }
        }

        Self { notches }
    }

    /// Returns the center frequencies of active notch filters.
    pub fn active_frequencies(&self) -> Vec<f32> {
        self.notches.iter().map(|n| n.center_freq()).collect()
    }

    /// Check if any notch filters are active.
    pub fn is_empty(&self) -> bool {
        self.notches.is_empty()
    }

    /// Reset all cascaded filters.
    pub fn reset(&mut self) {
        for notch in &mut self.notches {
            notch.reset();
        }
    }

    /// Process an audio slice in-place through all cascaded notch filters.
    pub fn process_slice(&mut self, samples: &mut [f32]) {
        for notch in &mut self.notches {
            notch.process_slice(samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hum_notch_50hz_attenuation() {
        let sample_rate = 48000;
        let mut notch = BiquadNotch::new(50.0, 25.0, sample_rate);

        let duration_secs = 2.0;
        let n_samples = (sample_rate as f32 * duration_secs) as usize;

        // Signal: 50 Hz hum (amplitude 0.5) + 500 Hz voice fundamental (amplitude 0.5)
        let mut hum_only: Vec<f32> = (0..n_samples)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 50.0 * (i as f32) / sample_rate as f32).sin())
            .collect();
        let voice_only: Vec<f32> = (0..n_samples)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 500.0 * (i as f32) / sample_rate as f32).sin())
            .collect();

        // 1. Check attenuation of 50 Hz hum
        notch.process_slice(&mut hum_only);
        let hum_tail = &hum_only[n_samples / 2..];
        let hum_tail_rms = (hum_tail.iter().map(|&s| s * s).sum::<f32>() / hum_tail.len() as f32).sqrt();
        let original_rms = 0.5 / std::f32::consts::SQRT_2;
        let hum_attenuation_db = 20.0 * (hum_tail_rms / original_rms).log10().abs();

        println!("50 Hz notch attenuation: {:.1} dB", hum_attenuation_db);
        assert!(
            hum_attenuation_db >= 25.0,
            "50 Hz hum must be attenuated by at least 25 dB (got {:.1} dB)",
            hum_attenuation_db
        );

        // 2. Check that 500 Hz voice tone is preserved
        let mut voice_test = voice_only.clone();
        let mut notch2 = BiquadNotch::new(50.0, 25.0, sample_rate);
        notch2.process_slice(&mut voice_test);
        let voice_tail = &voice_test[n_samples / 2..];
        let voice_tail_rms = (voice_tail.iter().map(|&s| s * s).sum::<f32>() / voice_tail.len() as f32).sqrt();
        let voice_diff_db = (20.0 * (voice_tail_rms / original_rms).log10()).abs();

        println!("500 Hz passband difference: {:.3} dB", voice_diff_db);
        assert!(
            voice_diff_db < 0.1,
            "500 Hz passband tone must be untouched (diff: {:.3} dB)",
            voice_diff_db
        );
    }

    #[test]
    fn test_cascaded_tonal_notch_filter() {
        let peaks = vec![
            TonalPeak {
                frequency_hz: 50.0,
                bin_index: 1,
                strength_db: 22.0,
                confidence: 0.95,
            },
            TonalPeak {
                frequency_hz: 100.0,
                bin_index: 2,
                strength_db: 15.0,
                confidence: 0.85,
            },
            TonalPeak {
                // Weak/low-confidence peak that should be ignored
                frequency_hz: 1000.0,
                bin_index: 21,
                strength_db: 4.0,
                confidence: 0.30,
            },
        ];

        let filter = TonalNotchFilter::from_tonal_peaks(&peaks, 48000, 0.60, 10.0);
        assert_eq!(filter.notches.len(), 2);
        let freqs = filter.active_frequencies();
        assert!((freqs[0] - 50.0).abs() < 1e-4);
        assert!((freqs[1] - 100.0).abs() < 1e-4);
    }
}
