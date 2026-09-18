//! Singing and Voiced Speech Harmonicity Detection and Harmonic Protection Masking.
//!
//! Defined in Section 33 and Section 72 of architecture.md:
//! Protects structured harmonic series (fundamental + integer harmonics) of
//! singing and voice from being degraded or hollowed out by noise suppression.

use crate::stft::Spectrogram;

/// Configuration for pitch detection and harmonic series protection.
#[derive(Debug, Clone)]
pub struct HarmonicityConfig {
    /// Minimum pitch search frequency in Hz (default: 75 Hz).
    pub min_pitch_hz: f32,
    /// Maximum pitch search frequency in Hz (default: 880 Hz, high soprano / head voice).
    pub max_pitch_hz: f32,
    /// Maximum number of harmonics to track (default: 10).
    pub max_harmonics: usize,
    /// Width of harmonic protection band around each peak bin (default: 1 bin).
    pub harmonic_half_width_bins: usize,
    /// Minimum harmonicity threshold to declare voiced content (default: 0.30).
    pub min_voiced_harmonicity: f32,
}

impl Default for HarmonicityConfig {
    fn default() -> Self {
        Self {
            min_pitch_hz: 80.0,
            max_pitch_hz: 880.0,
            max_harmonics: 10,
            harmonic_half_width_bins: 1,
            min_voiced_harmonicity: 0.35,
        }
    }
}

/// Analysis result for a single STFT frame.
#[derive(Debug, Clone)]
pub struct FrameHarmonicity {
    /// Estimated fundamental pitch in Hz, or None if unvoiced/silence.
    pub f0_hz: Option<f32>,
    /// Harmonicity index in [0.0, 1.0].
    pub harmonicity: f32,
    /// Voiced decision flag.
    pub is_voiced: bool,
}

/// Harmonic protection analyzer.
pub struct HarmonicityEstimator {
    config: HarmonicityConfig,
    sample_rate: u32,
}

impl HarmonicityEstimator {
    pub fn new(sample_rate: u32, config: HarmonicityConfig) -> Self {
        Self { config, sample_rate }
    }

    /// Estimate fundamental pitch ($F_0$) and harmonicity for a single frame magnitude spectrum.
    pub fn analyze_frame(&self, mag: &[f32], window_size: usize) -> FrameHarmonicity {
        let bins = mag.len();
        let bin_hz = self.sample_rate as f32 / window_size as f32;

        let min_bin = ((self.config.min_pitch_hz / bin_hz).ceil() as usize).max(3);
        let max_bin = ((self.config.max_pitch_hz / bin_hz).floor() as usize).min(bins / 2);

        if min_bin >= max_bin {
            return FrameHarmonicity {
                f0_hz: None,
                harmonicity: 0.0,
                is_voiced: false,
            };
        }

        // Search candidate F0 bins
        let mut best_bin = 0usize;
        let mut best_harmonic_energy = 0.0f32;
        let mut best_overtone_count = 0usize;
        let voice_band_limit = ((4000.0 / bin_hz) as usize).min(bins);

        let total_band_energy: f32 = mag[min_bin..voice_band_limit].iter().map(|&s| s * s).sum();
        let norm_band_power = total_band_energy / (window_size as f32 * window_size as f32);
        // If normalized vocal band power is below -42 dBFS (~1e-4), this is silence or ambient room hiss, not singing voice
        if norm_band_power <= 1e-4 {
            return FrameHarmonicity {
                f0_hz: None,
                harmonicity: 0.0,
                is_voiced: false,
            };
        }

        for candidate_bin in min_bin..=max_bin {
            // A fundamental candidate must be a local spectral peak
            if candidate_bin > 0 && candidate_bin + 1 < bins {
                if mag[candidate_bin] <= mag[candidate_bin - 1] || mag[candidate_bin] <= mag[candidate_bin + 1] {
                    continue;
                }
            }

            let f0_energy = mag[candidate_bin] * mag[candidate_bin];
            if f0_energy <= 1e-4 {
                continue;
            }

            // Estimate refined F0 frequency via parabolic interpolation around candidate_bin
            let delta = if candidate_bin > 0 && candidate_bin + 1 < bins {
                let a = mag[candidate_bin - 1];
                let b = mag[candidate_bin];
                let c = mag[candidate_bin + 1];
                0.5 * (a - c) / (a - 2.0 * b + c + 1e-12)
            } else {
                0.0
            };
            let cand_f0_hz = (candidate_bin as f32 + delta.clamp(-0.5, 0.5)) * bin_hz;

            let mut harm_energy = f0_energy;
            let mut valley_energy = 0.0f32;
            let mut overtone_count = 0usize;

            for h in 2..=self.config.max_harmonics {
                let harm_hz = cand_f0_hz * h as f32;
                let h_center = (harm_hz / bin_hz).round() as usize;
                let valley_hz = cand_f0_hz * (h as f32 - 0.5);
                let v_center = (valley_hz / bin_hz).round() as usize;

                if h_center < voice_band_limit {
                    // Search in a local 3-bin window around the expected harmonic frequency
                    let start_b = h_center.saturating_sub(1);
                    let end_b = (h_center + 1).min(voice_band_limit - 1);
                    let p = mag[start_b..=end_b]
                        .iter()
                        .map(|&v| v * v)
                        .fold(0.0f32, |acc, v| acc.max(v));

                    // Verify that harmonic has noticeable energy relative to fundamental (> -20 dB)
                    if p > 0.01 * f0_energy {
                        harm_energy += p;
                        overtone_count += 1;
                    }
                }

                if v_center < voice_band_limit {
                    valley_energy += mag[v_center] * mag[v_center];
                }
            }

            // Real voiced speech/singing has distinct spectral peaks well above inter-harmonic valleys (contrast >= 2.5).
            // In random noise, peaks and valleys have equal average power (contrast ~ 1.0).
            let contrast = harm_energy / valley_energy.max(1e-12);
            let has_contrast = contrast >= 2.5;

            // A genuine vocal harmonic series requires high contrast and at least 2 overtones,
            // or for high-pitched singing (F0 >= 300 Hz) at least 1 overtone when fundamental is dominant.
            let is_singing_candidate = has_contrast && (
                (overtone_count >= 2)
                || (overtone_count >= 1 && cand_f0_hz >= 300.0 && (f0_energy / total_band_energy) > 0.30)
            );

            if is_singing_candidate && harm_energy > best_harmonic_energy {
                best_harmonic_energy = harm_energy;
                best_bin = candidate_bin;
                best_overtone_count = overtone_count;
            }
        }

        let harmonicity = if best_overtone_count >= 2
            || (best_overtone_count >= 1 && (best_bin as f32 * bin_hz) >= 300.0)
        {
            (best_harmonic_energy / total_band_energy).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let is_voiced = harmonicity >= self.config.min_voiced_harmonicity;

        let f0_hz = if is_voiced && best_bin > 0 {
            let delta = if best_bin > 0 && best_bin + 1 < bins {
                let a = mag[best_bin - 1];
                let b = mag[best_bin];
                let c = mag[best_bin + 1];
                0.5 * (a - c) / (a - 2.0 * b + c + 1e-12)
            } else {
                0.0
            };
            Some((best_bin as f32 + delta.clamp(-0.5, 0.5)) * bin_hz)
        } else {
            None
        };

        FrameHarmonicity {
            f0_hz,
            harmonicity,
            is_voiced,
        }
    }

    /// Compute a 2D time-frequency harmonic protection mask `[frames][bins]` in range `[0.0, 1.0]`.
    /// Values close to 1.0 protect speech and singing harmonics from suppression.
    pub fn compute_protection_mask(&self, spec: &Spectrogram) -> Vec<Vec<f32>> {
        let num_frames = spec.num_frames();
        let num_bins = spec.num_bins();
        let bin_hz = self.sample_rate as f32 / spec.window_size as f32;

        let mut mask = vec![vec![0.0f32; num_bins]; num_frames];

        for (t, frame) in spec.frames.iter().enumerate() {
            let mag: Vec<f32> = frame.iter().map(|c| c.norm()).collect();
            let analysis = self.analyze_frame(&mag, spec.window_size);

            if let Some(f0) = analysis.f0_hz {
                let strength = analysis.harmonicity.clamp(0.0, 1.0);

                for h in 1..=self.config.max_harmonics {
                    let harm_hz = f0 * h as f32;
                    let center_bin = (harm_hz / bin_hz).round() as usize;

                    if center_bin < num_bins {
                        let w = self.config.harmonic_half_width_bins;
                        let start_b = center_bin.saturating_sub(w);
                        let end_b = (center_bin + w).min(num_bins - 1);

                        for (b_offset, mask_val) in mask[t][start_b..=end_b].iter_mut().enumerate() {
                            let b = start_b + b_offset;
                            let dist = (b as isize - center_bin as isize).abs() as f32;
                            let shape = 1.0 - (dist / (w as f32 + 1.0));
                            let val = strength * shape;
                            if val > *mask_val {
                                *mask_val = val;
                            }
                        }
                    }
                }
            }
        }

        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;
    use std::f32::consts::PI;

    #[test]
    fn test_harmonic_detection_on_singing_tone() {
        let sample_rate = 48000;
        let stft = StftEngine::default_48k().unwrap();
        let estimator = HarmonicityEstimator::new(sample_rate, HarmonicityConfig::default());

        // Generate synthetic voiced note with 4 harmonics (A4 = 440 Hz)
        let n = 48000; // 1s
        let mut audio = vec![0.0f32; n];
        for (i, sample) in audio.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample = 0.3 * (2.0 * PI * 440.0 * t).sin()
                + 0.15 * (2.0 * PI * 880.0 * t).sin()
                + 0.08 * (2.0 * PI * 1320.0 * t).sin()
                + 0.04 * (2.0 * PI * 1760.0 * t).sin();
        }

        let spec = stft.forward(&audio).unwrap();
        let mask = estimator.compute_protection_mask(&spec);

        assert_eq!(mask.len(), spec.num_frames());
        assert_eq!(mask[0].len(), spec.num_bins());

        // Middle frame should have strong protection at bin corresponding to 440 Hz
        let mid_frame = spec.num_frames() / 2;
        let bin_hz: f32 = 48000.0 / 1024.0;
        let bin_440 = (440.0f32 / bin_hz).round() as usize;

        assert!(
            mask[mid_frame][bin_440] > 0.4,
            "Fundamental bin 440 Hz must have strong protection mask"
        );
    }
}
