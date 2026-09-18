//! Preservation Layer Orchestration.
//!
//! Defined in Section 44, 45, 46 and Section 72 of architecture.md:
//! Combines latency alignment, dry/wet preservation blending, and dual-synthesis
//! removed-noise calculation into a unified, mathematically conserved stage.
//!
//! The required order is:
//! 1. Processed output
//! 2. Latency alignment (delay compensation)
//! 3. Preservation calculation (vocal confidence + harmonic protection)
//! 4. Dry/wet blend: Output = (1 - alpha) * Aligned_X + alpha * Y
//! 5. Removed-noise synthesis: Removed_Noise = Aligned_X - Output

use crate::activity::ActivityReport;
use crate::harmonicity::{HarmonicityConfig, HarmonicityEstimator};
use crate::latency::{CombFilterMetrics, LatencyAligner};
use crate::stft::StftEngine;

/// Configuration for the adaptive preservation blend.
#[derive(Debug, Clone)]
pub struct AdaptivePreservationConfig {
    /// Nominal noise suppression intensity when vocal is absent (default: 0.90).
    pub base_alpha: f32,
    /// Minimum blend factor during strongest vocal/singing phonemes (default: 0.50).
    pub min_vocal_alpha: f32,
    /// Vocal activity sensitivity [0.0, 1.0] (default: 0.75).
    pub vocal_protection_strength: f32,
    /// Harmonic protection sensitivity [0.0, 1.0] (default: 0.85).
    pub harmonic_protection_strength: f32,
    /// Configuration for harmonic pitch tracker.
    pub harmonicity: HarmonicityConfig,
}

impl Default for AdaptivePreservationConfig {
    fn default() -> Self {
        Self {
            base_alpha: 1.0,
            min_vocal_alpha: 0.45,
            vocal_protection_strength: 0.85,
            harmonic_protection_strength: 0.90,
            harmonicity: HarmonicityConfig::default(),
        }
    }
}

/// Blending mode for the preservation layer.
#[derive(Debug, Clone)]
pub enum PreservationMode {
    /// Static global alpha in [0.0, 1.0]: 0.0 = 100% original, 1.0 = 100% processed.
    Global(f32),
    /// Content-adaptive preservation with vocal and singing harmonic series protection.
    Adaptive(AdaptivePreservationConfig),
}

impl Default for PreservationMode {
    fn default() -> Self {
        Self::Adaptive(AdaptivePreservationConfig::default())
    }
}

/// Comprehensive preservation report detailing alignment, blending, and vocal protection.
#[derive(Debug, Clone)]
pub struct PreservationReport {
    /// Mean preservation blending factor applied across all samples (alpha in [0.0, 1.0]).
    pub mean_alpha: f32,
    /// Algorithmic delay compensated in samples.
    pub latency_samples: usize,
    /// Algorithmic delay compensated in milliseconds.
    pub latency_ms: f32,
    /// Comb-filtering verification metrics between aligned paths.
    pub comb_metrics: CombFilterMetrics,
    /// Attenuation of vocal energy in the removed noise track in dB (> 20 dB indicates clean vocal preservation).
    pub vocal_leakage_attenuation_db: f32,
    /// Percentage of voice energy preserved in cleaned output (0.0 to 100.0%).
    pub vocal_preservation_percentage: f32,
}

/// Preservation Layer Processor.
pub struct PreservationLayer {
    sample_rate: u32,
    mode: PreservationMode,
}

impl PreservationLayer {
    pub fn new(sample_rate: u32, mode: PreservationMode) -> Self {
        Self { sample_rate, mode }
    }

    /// Default preservation layer for 48 kHz voice audio.
    pub fn default_48k() -> Self {
        Self::new(48000, PreservationMode::default())
    }

    /// Set active preservation mode.
    pub fn set_mode(&mut self, mode: PreservationMode) {
        self.mode = mode;
    }

    /// Execute the preservation pipeline:
    /// 1. Latency Alignment
    /// 2. Preservation Alpha Calculation (Global or Adaptive)
    /// 3. Dry/Wet Blending
    /// 4. Dual-Synthesis Removed Noise Calculation
    ///
    /// Returns: `(cleaned_output, removed_noise, report)`
    pub fn process(
        &self,
        original: &[f32],
        processed: &[f32],
        known_latency_samples: usize,
        activity: Option<&ActivityReport>,
    ) -> (Vec<f32>, Vec<f32>, PreservationReport) {
        let len = original.len().min(processed.len());
        if len == 0 {
            return (
                Vec::new(),
                Vec::new(),
                PreservationReport {
                    mean_alpha: 1.0,
                    latency_samples: 0,
                    latency_ms: 0.0,
                    comb_metrics: CombFilterMetrics {
                        notch_depth_db: 0.0,
                        has_comb_filtering: false,
                        phase_coherence: 1.0,
                    },
                    vocal_leakage_attenuation_db: 100.0,
                    vocal_preservation_percentage: 100.0,
                },
            );
        }

        // 1. Latency Alignment: Compensate for algorithmic delay
        let (aligned_orig, aligned_proc) = if known_latency_samples > 0 {
            LatencyAligner::align_by_delay(original, processed, known_latency_samples)
        } else {
            (original[..len].to_vec(), processed[..len].to_vec())
        };

        let comb_metrics =
            LatencyAligner::check_comb_filtering(&aligned_orig, &aligned_proc, self.sample_rate);

        // 2. Compute sample-wise or global blending alpha
        let n = aligned_orig.len().min(aligned_proc.len());
        let mut alphas = vec![1.0f32; n];

        match &self.mode {
            PreservationMode::Global(alpha) => {
                let clamped_alpha = alpha.clamp(0.0, 1.0);
                alphas.fill(clamped_alpha);
            }
            PreservationMode::Adaptive(cfg) => {
                // Initialize with base alpha
                alphas.fill(cfg.base_alpha);


                // Compute STFT-based harmonic protection
                if let Ok(stft) = StftEngine::default_48k() {
                    if let Ok(spec) = stft.forward(&aligned_orig) {
                        let estimator =
                            HarmonicityEstimator::new(self.sample_rate, cfg.harmonicity.clone());
                        let harm_mask = estimator.compute_protection_mask(&spec);

                        let hop = spec.hop_size;
                        for (t, frame_mask) in harm_mask.iter().enumerate() {
                            let frame_harm_max = frame_mask
                                .iter()
                                .copied()
                                .fold(0.0f32, |acc, v| acc.max(v));

                            // Vocal and harmonic protection only engages when voice/singing is present.
                            // In pure room noise or pauses, vocal_active and harm_active are 0.0 -> prot_weight is 0.0 -> alpha = 1.0 (dead silence).
                            let (vocal_active, harm_active) = if let Some(act) = activity {
                                let fv = act.frame_confidences.get(t).copied().unwrap_or(act.overall_confidence);
                                let v_act = if fv >= 0.25 { fv } else { 0.0 };
                                let h_act = if v_act > 0.0 && frame_harm_max >= 0.25 { frame_harm_max } else { 0.0 };
                                (v_act, h_act)
                            } else {
                                let h_act = if frame_harm_max >= 0.35 { frame_harm_max } else { 0.0 };
                                (0.0, h_act)
                            };

                            let prot_weight = (vocal_active * cfg.vocal_protection_strength)
                                .max(harm_active * cfg.harmonic_protection_strength)
                                .clamp(0.0, 1.0);

                            let mut frame_alpha =
                                cfg.base_alpha - prot_weight * (cfg.base_alpha - cfg.min_vocal_alpha);

                            let start_idx = t * hop;
                            let end_idx = (start_idx + hop).min(n);

                            // Over-suppression detector: if active vocal/singing is detected,
                            // but the neural processor attenuated the signal drastically (Y power << X power),
                            // adapt alpha lower to prevent vocal cutout and eliminate residual vocal leakage into the noise track.
                            if prot_weight >= 0.40 && start_idx < end_idx {
                                let x_power: f32 = aligned_orig[start_idx..end_idx]
                                    .iter()
                                    .map(|&s| s * s)
                                    .sum();
                                let y_power: f32 = aligned_proc[start_idx..end_idx]
                                    .iter()
                                    .map(|&s| s * s)
                                    .sum();

                                let frame_len = (end_idx - start_idx) as f32;
                                let mean_sample_power = x_power / frame_len.max(1.0);
                                // Ensure signal is at actual speech/singing volume (> -34 dBFS, i.e. power > 4e-4)
                                // so pauses/room noise are never mistaken for suppressed voice.
                                if mean_sample_power > 4e-4 && y_power < 0.60 * x_power {
                                    // Neural model dropped the voice: scale alpha down proportionally
                                    let suppression_ratio = (y_power / x_power).clamp(0.0, 1.0);
                                    frame_alpha = (frame_alpha * suppression_ratio).max(0.08);
                                }
                            }

                            alphas[start_idx..end_idx].fill(frame_alpha.clamp(0.0, 1.0));
                        }
                    }
                }
            }
        }

        // 3. Dry/Wet Blending: Output = Aligned_X + alpha * (Aligned_Y - Aligned_X)
        let mut cleaned_output = vec![0.0f32; n];
        let mut removed_noise = vec![0.0f32; n];

        let mut sum_alpha = 0.0f32;
        for i in 0..n {
            let x = aligned_orig[i];
            let y = aligned_proc[i];
            let a = alphas[i];
            sum_alpha += a;

            // Blended output
            let out = x + a * (y - x);
            cleaned_output[i] = out;

            // Dual-synthesis removed noise: Aligned_X - Output = a * (Aligned_X - Aligned_Y)
            removed_noise[i] = x - out;
        }

        let mean_alpha = sum_alpha / n.max(1) as f32;

        // 4. Verification Metrics: Vocal leakage attenuation and preservation ratio
        let orig_energy: f32 = aligned_orig.iter().map(|&s| s * s).sum();
        let cleaned_energy: f32 = cleaned_output.iter().map(|&s| s * s).sum();
        let removed_energy: f32 = removed_noise.iter().map(|&s| s * s).sum();

        let vocal_leakage_attenuation_db = if removed_energy > 1e-12 && orig_energy > 1e-12 {
            10.0 * (orig_energy / removed_energy).log10()
        } else {
            60.0
        };

        let vocal_preservation_percentage = if orig_energy > 1e-12 {
            (cleaned_energy / orig_energy * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        let latency_samples = known_latency_samples;
        let latency_ms = (latency_samples as f32 / self.sample_rate as f32) * 1000.0;

        let report = PreservationReport {
            mean_alpha,
            latency_samples,
            latency_ms,
            comb_metrics,
            vocal_leakage_attenuation_db,
            vocal_preservation_percentage,
        };

        (cleaned_output, removed_noise, report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_global_alpha_preservation_bounds() {
        let sample_rate: u32 = 48000;
        let n = sample_rate as usize;
        let mut orig = vec![0.0f32; n];
        let mut proc = vec![0.0f32; n];
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            orig[i] = (2.0 * PI * 440.0 * t).sin();
            proc[i] = 0.5 * orig[i]; // Processed with 6 dB attenuation
        }

        // Test alpha = 0.0 -> 100% original
        let layer_zero = PreservationLayer::new(sample_rate, PreservationMode::Global(0.0));
        let (out_zero, rem_zero, rep_zero) = layer_zero.process(&orig, &proc, 0, None);
        assert!((rep_zero.mean_alpha - 0.0).abs() < 1e-5);
        for i in 0..n {
            assert!((out_zero[i] - orig[i]).abs() < 1e-5);
            assert!(rem_zero[i].abs() < 1e-5, "Removed noise must be 0 for alpha 0.0");
        }

        // Test alpha = 1.0 -> 100% processed
        let layer_one = PreservationLayer::new(sample_rate, PreservationMode::Global(1.0));
        let (out_one, rem_one, rep_one) = layer_one.process(&orig, &proc, 0, None);
        assert!((rep_one.mean_alpha - 1.0).abs() < 1e-5);
        for i in 0..n {
            assert!((out_one[i] - proc[i]).abs() < 1e-5);
            assert!((rem_one[i] - (orig[i] - proc[i])).abs() < 1e-5);
        }
    }

    #[test]
    fn test_exact_dual_synthesis_conservation() {
        let sample_rate: u32 = 48000;
        let n = 24000;
        let mut orig = vec![0.0f32; n];
        let mut proc = vec![0.0f32; n];
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            orig[i] = (2.0 * PI * 300.0 * t).sin();
            proc[i] = 0.8 * orig[i];
        }

        let layer = PreservationLayer::new(sample_rate, PreservationMode::Global(0.7));
        let (cleaned, removed, _) = layer.process(&orig, &proc, 0, None);

        // Mathematical conservation: Cleaned + Removed = Aligned_Original
        for i in 0..n {
            let sum = cleaned[i] + removed[i];
            assert!(
                (sum - orig[i]).abs() < 1e-5,
                "Cleaned + Removed must equal original at sample {}: sum={}, orig={}",
                i,
                sum,
                orig[i]
            );
        }
    }
}
