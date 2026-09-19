//! Classical DSP Noise Removal Pipeline Orchestrator.
//!
//! Chains:
//!   1. DC Blocker
//!   2. Tonal Notch Filter (targeting confirmed hums, e.g. 50/60 Hz)
//!   3. STFT Forward Transform
//!   4. Decision-Directed Wiener Spectral Suppression with Vocal Protection
//!   5. 2D Mask Smoothing (Frequency 3-tap + Time Attack/Release)
//!   6. Inverse STFT Dual-Synthesis:
//!      - dsp_cleaned
//!      - removed_noise
//!   7. Energy conservation & performance reporting

use std::time::Instant;

use crate::activity::ActivityReport;
use crate::dc_blocker::DcBlocker;
use crate::declicker::Declicker;
use crate::error::DspError;
use crate::mask::{MaskSmoother, MaskSmootherConfig};
use crate::noise_profile::NoiseProfile;
use crate::notch::TonalNotchFilter;
use crate::plosive::PlosiveFilter;
use crate::preservation::{
    AdaptivePreservationConfig, PreservationLayer, PreservationMode, PreservationReport,
};
use crate::residual::{analyze_residual, ResidualReport};
use crate::stft::{Spectrogram, StftEngine};
use crate::wiener::{WienerConfig, WienerSuppressor};

/// User-selectable intensity preset for classical DSP suppression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DspIntensity {
    /// Conservative suppression (-18 dB limit). Preserves subtle singing dynamics and delicate vocal timbre.
    Gentle,
    /// Balanced suppression (-22 dB limit). Standard room/fan reduction with zero musical noise.
    #[default]
    Balanced,
    /// Strong suppression (-28 dB limit). Deeper reduction for loud room noise, wired headset hiss, or noisy fans.
    Aggressive,
}

impl DspIntensity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gentle => "Gentle (Vocal Safe)",
            Self::Balanced => "Balanced",
            Self::Aggressive => "Aggressive (Deep)",
        }
    }

    pub fn to_config(self) -> DspConfig {
        match self {
            Self::Gentle => DspConfig {
                enable_declicker: true,
                enable_plosive_filter: false,
                enable_dc_blocker: true,
                enable_tonal_notch: true,
                min_notch_confidence: 0.60,
                min_notch_strength_db: 10.0,
                wiener: WienerConfig {
                    oversubtraction: 1.15,
                    min_gain: 0.12, // ~ -18.4 dB
                    decision_directed_alpha: 0.92,
                    vocal_protection: true,
                },
                mask_smoothing: MaskSmootherConfig::default(),
                preservation: PreservationMode::Adaptive(AdaptivePreservationConfig {
                    base_alpha: 1.0,
                    min_vocal_alpha: 0.65,
                    vocal_protection_strength: 0.85,
                    harmonic_protection_strength: 0.90,
                    ..AdaptivePreservationConfig::default()
                }),
            },
            Self::Balanced => DspConfig {
                enable_declicker: true,
                enable_plosive_filter: false,
                enable_dc_blocker: true,
                enable_tonal_notch: true,
                min_notch_confidence: 0.60,
                min_notch_strength_db: 10.0,
                wiener: WienerConfig {
                    oversubtraction: 1.30,
                    min_gain: 0.08, // ~ -21.9 dB
                    decision_directed_alpha: 0.90,
                    vocal_protection: true,
                },
                mask_smoothing: MaskSmootherConfig::default(),
                preservation: PreservationMode::Global(1.0),
            },
            Self::Aggressive => DspConfig {
                enable_declicker: true,
                enable_plosive_filter: false,
                enable_dc_blocker: true,
                enable_tonal_notch: true,
                min_notch_confidence: 0.50,
                min_notch_strength_db: 8.0,
                wiener: WienerConfig {
                    oversubtraction: 1.45,
                    min_gain: 0.04, // ~ -28.0 dB
                    decision_directed_alpha: 0.88,
                    vocal_protection: true,
                },
                mask_smoothing: MaskSmootherConfig {
                    attack_factor: 0.88,
                    release_factor: 0.35,
                    center_weight: 0.65,
                },
                preservation: PreservationMode::Global(1.0),
            },
        }
    }
}

/// Configuration for the DSP noise removal pass.
#[derive(Debug, Clone)]
pub struct DspConfig {
    /// Enable sample-accurate impulse de-clicker for USB jitter and mic clicks.
    pub enable_declicker: bool,
    /// Enable 4th-order 80 Hz acoustic low-cut and breath puff dampener.
    pub enable_plosive_filter: bool,
    /// Enable 1-pole infrasonic DC blocker (< 38 Hz).
    pub enable_dc_blocker: bool,
    /// Enable biquad notch filtering for confirmed tonal hums.
    pub enable_tonal_notch: bool,
    /// Minimum confidence threshold for hum notching (default: 0.60).
    pub min_notch_confidence: f32,
    /// Minimum prominence threshold for hum notching in dB (default: 10.0 dB).
    pub min_notch_strength_db: f32,
    /// Wiener suppression settings.
    pub wiener: WienerConfig,
    /// 2D mask smoothing settings.
    pub mask_smoothing: MaskSmootherConfig,
    /// Preservation layer blending settings (Phase 7).
    pub preservation: PreservationMode,
}

impl Default for DspConfig {
    fn default() -> Self {
        DspIntensity::Balanced.to_config()
    }
}

/// Diagnostic report generated after completing a DSP cleaning pass.
#[derive(Debug, Clone)]
pub struct DspProcessingReport {
    /// Input RMS in dBFS.
    pub input_rms_dbfs: f32,
    /// Cleaned audio RMS in dBFS.
    pub cleaned_rms_dbfs: f32,
    /// Removed noise RMS in dBFS.
    pub removed_noise_rms_dbfs: f32,
    /// Effective noise reduction in dB.
    pub attenuation_db: f32,
    /// Frequencies notched by the biquad hum filter in Hz.
    pub notched_frequencies: Vec<f32>,
    /// Elapsed wall-clock processing time in milliseconds.
    pub processing_time_ms: f32,
}

/// Output of a completed DSP processing pass.
pub struct DspProcessResult {
    /// Cleaned vocal audio samples.
    pub cleaned_samples: Vec<f32>,
    /// Extracted noise samples (original minus cleaned).
    pub removed_noise_samples: Vec<f32>,
    /// Detailed diagnostic report.
    pub report: DspProcessingReport,
    /// Post-DSP residual noise evaluation and AI dispatch decision (Phase 5).
    pub residual: ResidualReport,
    /// Phase 7 Preservation layer report.
    pub preservation: PreservationReport,
}

/// Classical DSP Noise Removal Pipeline.
pub struct DspProcessor {
    sample_rate: u32,
    stft_engine: StftEngine,
}

impl DspProcessor {
    /// Create a new processor for the given sample rate (e.g. 48000).
    pub fn new(sample_rate: u32) -> Result<Self, DspError> {
        let stft_engine = StftEngine::default_48k()?;
        Ok(Self {
            sample_rate,
            stft_engine,
        })
    }

    /// Run the complete Phase 4 DSP noise removal pipeline on an input recording.
    pub fn process(
        &self,
        input_samples: &[f32],
        noise_profile: &NoiseProfile,
        activity: Option<&ActivityReport>,
        config: &DspConfig,
    ) -> Result<DspProcessResult, DspError> {
        let start_time = Instant::now();

        if input_samples.is_empty() {
            let residual = analyze_residual(&[], noise_profile, activity, self.sample_rate)?;
            return Ok(DspProcessResult {
                cleaned_samples: Vec::new(),
                removed_noise_samples: Vec::new(),
                report: DspProcessingReport {
                    input_rms_dbfs: -120.0,
                    cleaned_rms_dbfs: -120.0,
                    removed_noise_rms_dbfs: -120.0,
                    attenuation_db: 0.0,
                    notched_frequencies: Vec::new(),
                    processing_time_ms: 0.0,
                },
                residual,
                preservation: PreservationReport {
                    mean_alpha: 1.0,
                    latency_samples: 0,
                    latency_ms: 0.0,
                    comb_metrics: crate::latency::CombFilterMetrics {
                        notch_depth_db: 0.0,
                        has_comb_filtering: false,
                        phase_coherence: 1.0,
                    },
                    vocal_leakage_attenuation_db: 100.0,
                    vocal_preservation_percentage: 100.0,
                },
            });
        }

        // Calculate input RMS
        let input_rms = (input_samples.iter().map(|&s| s * s).sum::<f32>() / input_samples.len() as f32).sqrt();
        let input_rms_dbfs = 20.0 * (input_rms.max(1e-12)).log10();

        // 1. Time-domain pre-filtering: De-Clicker, Plosive/Low-Cut, DC Blocker & Tonal Notches
        let mut preprocessed = input_samples.to_vec();

        if config.enable_declicker {
            let declicker = Declicker::default_48k();
            declicker.process_in_place(&mut preprocessed);
        }

        if config.enable_plosive_filter {
            let mut plosive = PlosiveFilter::default_48k();
            plosive.process_in_place(&mut preprocessed);
        }

        if config.enable_dc_blocker {
            let mut dc_blocker = DcBlocker::default_48k();
            dc_blocker.process_slice(&mut preprocessed);
        }

        let mut notched_frequencies = Vec::new();
        if config.enable_tonal_notch {
            let mut notch_filter = TonalNotchFilter::from_tonal_peaks(
                &noise_profile.tonal_peaks,
                self.sample_rate,
                config.min_notch_confidence,
                config.min_notch_strength_db,
            );
            notched_frequencies = notch_filter.active_frequencies();
            notch_filter.process_slice(&mut preprocessed);
        }

        // 2. Forward STFT
        let spectrogram = self.stft_engine.forward(&preprocessed)?;
        let num_frames = spectrogram.num_frames();
        let num_bins = spectrogram.num_bins();

        // 3. Wiener Spectral Suppression
        let suppressor = WienerSuppressor::new(config.wiener.clone());
        let raw_mask = suppressor.compute_gain_mask(&spectrogram, noise_profile, activity);

        // 4. 2D Mask Smoothing (Frequency 3-tap + Time Attack/Release)
        let mut smoother = MaskSmoother::new(config.mask_smoothing.clone());
        let smooth_mask = smoother.smooth_mask(&raw_mask);

        // 5. Dual-Spectrogram Synthesis (Cleaned vs Removed Noise)
        let mut cleaned_frames = Vec::with_capacity(num_frames);
        let mut noise_frames = Vec::with_capacity(num_frames);

        for (t, frame) in spectrogram.frames.iter().enumerate() {
            let mut c_frame = Vec::with_capacity(num_bins);
            let mut n_frame = Vec::with_capacity(num_bins);

            for k in 0..num_bins {
                let g = smooth_mask[t][k];
                let orig = frame[k];

                c_frame.push(orig * g);
                n_frame.push(orig * (1.0 - g));
            }

            cleaned_frames.push(c_frame);
            noise_frames.push(n_frame);
        }

        let cleaned_spec = Spectrogram {
            frames: cleaned_frames,
            window_size: spectrogram.window_size,
            hop_size: spectrogram.hop_size,
            original_len: spectrogram.original_len,
        };

        let noise_spec = Spectrogram {
            frames: noise_frames,
            window_size: spectrogram.window_size,
            hop_size: spectrogram.hop_size,
            original_len: spectrogram.original_len,
        };

        // 6. Inverse STFT Synthesis
        let mut cleaned_samples = self.stft_engine.inverse(&cleaned_spec)?;
        let mut removed_noise_samples = self.stft_engine.inverse(&noise_spec)?;

        // Clamp / align exact length with original input
        cleaned_samples.truncate(input_samples.len());
        if cleaned_samples.len() < input_samples.len() {
            cleaned_samples.resize(input_samples.len(), 0.0);
        }

        removed_noise_samples.truncate(input_samples.len());
        if removed_noise_samples.len() < input_samples.len() {
            removed_noise_samples.resize(input_samples.len(), 0.0);
        }

        // 7. Phase 7: Preservation Layer Orchestration (Latency Alignment, Harmonic & Vocal Protection Blending, Aligned Dual-Synthesis)
        let preservation_layer = PreservationLayer::new(self.sample_rate, config.preservation.clone());
        let (final_cleaned, final_removed_noise, preservation_report) = preservation_layer.process(
            &preprocessed,
            &cleaned_samples,
            0, // STFT synthesis is zero-latency
            activity,
        );

        // 8. Performance metrics on preserved output
        let cleaned_rms = (final_cleaned.iter().map(|&s| s * s).sum::<f32>() / final_cleaned.len().max(1) as f32).sqrt();
        let cleaned_rms_dbfs = 20.0 * (cleaned_rms.max(1e-12)).log10();

        let removed_rms = (final_removed_noise.iter().map(|&s| s * s).sum::<f32>() / final_removed_noise.len().max(1) as f32).sqrt();
        let removed_noise_rms_dbfs = 20.0 * (removed_rms.max(1e-12)).log10();

        let attenuation_db = (input_rms_dbfs - cleaned_rms_dbfs).max(0.0);
        let processing_time_ms = start_time.elapsed().as_secs_f32() * 1000.0;

        // 9. Phase 5: Post-DSP Residual Noise Analysis & AI Dispatch Decision
        let residual = analyze_residual(&final_cleaned, noise_profile, activity, self.sample_rate)?;

        Ok(DspProcessResult {
            cleaned_samples: final_cleaned,
            removed_noise_samples: final_removed_noise,
            report: DspProcessingReport {
                input_rms_dbfs,
                cleaned_rms_dbfs,
                removed_noise_rms_dbfs,
                attenuation_db,
                notched_frequencies,
                processing_time_ms,
            },
            residual,
            preservation: preservation_report,
        })

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_end_to_end_dsp_pipeline() {
        let processor = DspProcessor::new(48000).unwrap();
        let sample_rate = 48000;
        let total_samples = 48000 * 2; // 2 seconds

        // 1. Synthesize steady noise + 50 Hz hum (Noise reference)
        let mut noise_ref = Vec::with_capacity(total_samples);
        let mut seed = 12345u32;
        for i in 0..total_samples {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let hiss = ((seed as f32 / u32::MAX as f32) - 0.5) * 0.04;
            let hum = 0.05 * (2.0 * std::f32::consts::PI * 50.0 * (i as f32) / sample_rate as f32).sin();
            noise_ref.push(hiss + hum);
        }

        let stft = StftEngine::default_48k().unwrap();
        let spec_noise = stft.forward(&noise_ref).unwrap();
        let profile = NoiseProfile::from_spectrogram(&spec_noise, &noise_ref, sample_rate);

        // 2. Synthesize noisy voice signal (voice burst at 1.0s to 1.5s)
        let mut vocal_recording = noise_ref.clone();
        for (offset, sample) in vocal_recording[48000..72000].iter_mut().enumerate() {
            let i = 48000 + offset;
            let t = i as f32 / sample_rate as f32;
            let voice = 0.3 * (2.0 * std::f32::consts::PI * 400.0 * t).sin();
            *sample += voice;
        }

        // 3. Process through DSP
        let config = DspConfig::default();
        let result = processor
            .process(&vocal_recording, &profile, None, &config)
            .unwrap();

        assert_eq!(result.cleaned_samples.len(), vocal_recording.len());
        assert_eq!(result.removed_noise_samples.len(), vocal_recording.len());

        println!(
            "Processing report: input={:.1} dBFS, cleaned={:.1} dBFS, noise={:.1} dBFS, att={:.1} dB, time={:.1} ms",
            result.report.input_rms_dbfs,
            result.report.cleaned_rms_dbfs,
            result.report.removed_noise_rms_dbfs,
            result.report.attenuation_db,
            result.report.processing_time_ms
        );

        // In pure noise region (0.0s to 0.5s), energy must be substantially reduced
        let noise_region_in = &vocal_recording[..24000];
        let noise_region_out = &result.cleaned_samples[..24000];
        let rms_in = (noise_region_in.iter().map(|&s| s * s).sum::<f32>() / 24000.0).sqrt();
        let rms_out = (noise_region_out.iter().map(|&s| s * s).sum::<f32>() / 24000.0).sqrt();
        let noise_drop_db = 20.0 * (rms_in / rms_out).log10();
        println!("Noise region attenuation: {:.1} dB", noise_drop_db);
        assert!(
            noise_drop_db >= 10.0,
            "Stationary noise must be reduced by at least 10 dB (got {:.1} dB)",
            noise_drop_db
        );

        // In voice region (1.1s to 1.4s), voice must be preserved
        let voice_region_out = &result.cleaned_samples[52800..67200];
        let voice_rms = (voice_region_out.iter().map(|&s| s * s).sum::<f32>() / voice_region_out.len() as f32).sqrt();
        assert!(
            voice_rms > 0.10,
            "Voice energy must remain strong and preserved, got {:.3}",
            voice_rms
        );
    }

    #[test]
    fn test_real_recorded_audio_if_present() {
        use std::path::Path;

        let rec_path = Path::new("../../recordings/original.wav");
        let noise_path = Path::new("../../recordings/noise_reference.wav");

        if !rec_path.exists() || !noise_path.exists() {
            println!("Skipping real audio test: recording files not found at ../../recordings/");
            return;
        }

        let (orig_samples, orig_spec) = audio_core::read_wav_canonical_f32(rec_path).unwrap();
        let (noise_samples, _) = audio_core::read_wav_canonical_f32(noise_path).unwrap();

        let stft = StftEngine::default_48k().unwrap();
        let noise_spec = stft.forward(&noise_samples).unwrap();
        let profile = NoiseProfile::from_spectrogram(&noise_spec, &noise_samples, orig_spec.sample_rate);

        let processor = DspProcessor::new(orig_spec.sample_rate).unwrap();
        let config = DspConfig::default();

        let result = processor.process(&orig_samples, &profile, None, &config).unwrap();

        println!(
            "REAL AUDIO REPORT: Input={:.1} dBFS, Cleaned={:.1} dBFS, Removed={:.1} dBFS, Attenuation={:.1} dB, Notches={:?}, Time={:.1} ms",
            result.report.input_rms_dbfs,
            result.report.cleaned_rms_dbfs,
            result.report.removed_noise_rms_dbfs,
            result.report.attenuation_db,
            result.report.notched_frequencies,
            result.report.processing_time_ms
        );

        assert_eq!(result.cleaned_samples.len(), orig_samples.len());
        assert_eq!(result.removed_noise_samples.len(), orig_samples.len());

        // Write cleaned and removed noise files
        let out_clean = Path::new("../../recordings/dsp_cleaned.wav");
        let out_noise = Path::new("../../recordings/removed_noise.wav");
        audio_core::write_wav_f32(out_clean, &result.cleaned_samples, orig_spec.sample_rate, 1).unwrap();
        audio_core::write_wav_f32(out_noise, &result.removed_noise_samples, orig_spec.sample_rate, 1).unwrap();
        println!("Successfully generated dsp_cleaned.wav and removed_noise.wav!");
    }
}
