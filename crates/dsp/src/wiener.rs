//! Wiener-style spectral suppression with Decision-Directed a priori SNR tracking
//! and vocal/singing harmonic protection.

use crate::activity::ActivityReport;
use crate::noise_profile::NoiseProfile;
use crate::stft::Spectrogram;

/// Configuration parameters for Wiener suppression.
#[derive(Debug, Clone)]
pub struct WienerConfig {
    /// Over-subtraction factor alpha (typically 1.1 to 1.5).
    pub oversubtraction: f32,
    /// Minimum gain floor (beta), preventing harsh gating/space silence (typically 0.10 to 0.20).
    pub min_gain: f32,
    /// Decision-directed recursive smoothing coefficient (typically 0.95 to 0.98).
    pub decision_directed_alpha: f32,
    /// Whether to apply vocal activity protection to prevent vocal hollowing.
    pub vocal_protection: bool,
}

impl Default for WienerConfig {
    fn default() -> Self {
        Self {
            oversubtraction: 1.30,
            min_gain: 0.10, // ~ -20 dB attenuation limit for steady suppression
            decision_directed_alpha: 0.90, // Responsive tracking (prevents Bluetooth volume dips)
            vocal_protection: true,
        }
    }
}

/// Wiener spectral suppressor.
#[derive(Debug, Clone)]
pub struct WienerSuppressor {
    config: WienerConfig,
}

impl WienerSuppressor {
    pub fn new(config: WienerConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(WienerConfig::default())
    }

    /// Compute 2D attenuation mask (frames x bins) for a noisy spectrogram given a noise profile.
    pub fn compute_gain_mask(
        &self,
        spectrogram: &Spectrogram,
        noise_profile: &NoiseProfile,
        activity: Option<&ActivityReport>,
    ) -> Vec<Vec<f32>> {
        let num_frames = spectrogram.num_frames();
        let num_bins = spectrogram.num_bins();
        if num_frames == 0 || num_bins == 0 {
            return Vec::new();
        }

        let noise_psd = &noise_profile.psd;
        let mut gain_mask = Vec::with_capacity(num_frames);
        let mut prev_clean_power = vec![0.0f32; num_bins];
        let dd_alpha = self.config.decision_directed_alpha.clamp(0.80, 0.99);
        let nyquist = (noise_profile.sample_rate as f32) * 0.5;

        for (t, frame) in spectrogram.frames.iter().enumerate() {
            let mut frame_gains = Vec::with_capacity(num_bins);

            let voice_confidence = if let Some(act) = activity {
                if t < act.frame_confidences.len() {
                    act.frame_confidences[t]
                } else {
                    act.overall_confidence
                }
            } else {
                0.5
            };

            for k in 0..num_bins {
                let noisy_power = frame[k].norm_sqr();
                let noise_p = if k < noise_psd.len() {
                    noise_psd[k].max(1e-12)
                } else {
                    1e-12
                };

                // A posteriori SNR gamma = P_noisy / P_noise
                let _gamma = noisy_power / noise_p;

                // Instantaneous SNR estimate with oversubtraction
                let oversub_noise = self.config.oversubtraction * noise_p;
                let instant_snr = ((noisy_power - oversub_noise) / noise_p).max(0.0);

                // Decision-Directed a priori SNR xi
                let xi = if t == 0 {
                    instant_snr
                } else {
                    let prior_estimate = prev_clean_power[k] / noise_p;
                    dd_alpha * prior_estimate + (1.0 - dd_alpha) * instant_snr
                };

                // Classic Wiener gain: G = xi / (1 + xi)
                let wiener_gain = xi / (1.0 + xi);

                // Frequency-selective vocal protection:
                // Only lift the floor in speech frequency bands (120 Hz - 4500 Hz) where local SNR is positive.
                // High-frequency fan hiss (> 4.5 kHz) and sub-bass (< 120 Hz) stay continuously suppressed
                // at `min_gain` so noise does not pump or rush in behind the voice.
                let freq_hz = (k as f32) * nyquist / ((num_bins - 1).max(1) as f32);
                let bin_min_gain = if self.config.vocal_protection
                    && voice_confidence > 0.20
                    && (120.0..=4500.0).contains(&freq_hz)
                {
                    let local_snr = noisy_power / noise_p;
                    if local_snr > 1.5 {
                        self.config.min_gain
                            + 0.35 * voice_confidence * ((local_snr - 1.5) / 3.0).clamp(0.0, 1.0)
                    } else {
                        self.config.min_gain
                    }
                } else {
                    self.config.min_gain
                };

                let final_gain = wiener_gain.clamp(bin_min_gain, 1.0);
                frame_gains.push(final_gain);

                // Store estimated clean power for next frame's decision-directed recursion
                prev_clean_power[k] = final_gain * final_gain * noisy_power;
            }

            gain_mask.push(frame_gains);
        }

        gain_mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;

    #[test]
    fn test_wiener_suppression_reduces_noise() {
        let engine = StftEngine::default_48k().unwrap();
        let sample_rate = 48000;
        let total_samples = 48000; // 1 second

        // 1. Synthesize stationary noise (ceiling fan simulation)
        let mut noise = Vec::with_capacity(total_samples);
        let mut seed = 42u32;
        for _ in 0..total_samples {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = ((seed as f32 / u32::MAX as f32) - 0.5) * 0.05; // -26 dBFS
            noise.push(val);
        }

        let noise_spec = engine.forward(&noise).unwrap();
        let profile = NoiseProfile::from_spectrogram(&noise_spec, &noise, sample_rate);

        // 2. Synthesize noisy voice signal: quiet noise + loud tone in middle
        let mut noisy_signal = noise.clone();
        for (offset, sample) in noisy_signal[12000..36000].iter_mut().enumerate() {
            let i = 12000 + offset;
            let t = i as f32 / sample_rate as f32;
            *sample += (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.4;
        }

        let noisy_spec = engine.forward(&noisy_signal).unwrap();
        let suppressor = WienerSuppressor::with_default_config();
        let mask = suppressor.compute_gain_mask(&noisy_spec, &profile, None);

        assert_eq!(mask.len(), noisy_spec.num_frames());

        // In pure noise regions (frame 10), gain should be clamped near min_gain (~0.15)
        let noise_frame_gain = mask[10].iter().sum::<f32>() / mask[10].len() as f32;
        println!("Noise region average gain: {:.3}", noise_frame_gain);
        assert!(
            noise_frame_gain < 0.35,
            "Noise region must be attenuated, got {:.3}",
            noise_frame_gain
        );

        // In voice tone region (frame 80), gain at 440 Hz bin should be close to 1.0
        let bin_440 = (440.0 / (sample_rate as f32 / 2.0) * noisy_spec.num_bins() as f32) as usize;
        let tone_bin_gain = mask[80][bin_440];
        println!("440 Hz voice tone gain: {:.3}", tone_bin_gain);
        assert!(
            tone_bin_gain > 0.85,
            "Tone bin must be preserved, got {:.3}",
            tone_bin_gain
        );
    }
}
