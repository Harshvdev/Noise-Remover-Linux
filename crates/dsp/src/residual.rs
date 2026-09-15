//! Residual Noise Analysis and AI Dispatch Decision (Phase 5).
//!
//! Evaluates the cleaned audio post-DSP to determine whether classical suppression
//! was sufficient or if the neural backend (Phase 6: DPDFNet2) must be invoked:
//!
//! - DSP residual = Low  -> Finish without AI (preserves CPU & battery).
//! - DSP residual = Moderate / High -> Invoke Neural Backend (Phase 6).

use crate::activity::ActivityReport;
use crate::error::DspError;
use crate::noise_profile::NoiseProfile;

/// Classification of remaining background noise post-DSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidualLevel {
    /// Noise is well below audible distraction threshold (<= -50 dBFS or SNR >= 25 dB).
    Low,
    /// Noticeable noise remains during pauses or under speech (-50 dBFS to -40 dBFS, SNR 15 to 25 dB).
    Moderate,
    /// Significant noise remains (> -40 dBFS or SNR < 15 dB).
    High,
}

impl ResidualLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "Low (Clean)",
            Self::Moderate => "Moderate (Noticeable)",
            Self::High => "High (Significant)",
        }
    }
}

/// Architectural decision on whether to invoke the neural AI backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenoiseDecision {
    /// Classical DSP was sufficient; finish without invoking AI.
    FinishWithoutAi,
    /// Remaining noise is noticeable; invoke Phase 6 neural backend (DPDFNet2).
    InvokeNeuralBackend,
}

impl DenoiseDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FinishWithoutAi => "Finish (AI not needed)",
            Self::InvokeNeuralBackend => "Invoke Neural Backend (Phase 6: DPDFNet2)",
        }
    }
}

/// Detailed post-DSP residual noise evaluation report.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualReport {
    /// Measured RMS of the remaining background noise in dBFS.
    pub residual_noise_dbfs: f32,
    /// Reference noise floor before DSP in dBFS.
    pub original_noise_dbfs: f32,
    /// Measured RMS of active vocal regions in dBFS.
    pub speech_rms_dbfs: f32,
    /// Post-DSP Signal-to-Noise Ratio (speech_rms - residual_noise) in dB.
    pub post_dsp_snr_db: f32,
    /// Total noise attenuation achieved by classical DSP in dB.
    pub noise_attenuation_db: f32,
    /// Normalized residual noise score in [0.0, 1.0] (0.0 = silent, 1.0 = heavy noise).
    pub residual_score: f32,
    /// Categorized residual level.
    pub level: ResidualLevel,
    /// Recommended next step decision.
    pub decision: DenoiseDecision,
    /// Human-readable explanation of the architectural recommendation.
    pub explanation: String,
}

/// Analyze post-DSP cleaned audio to evaluate residual noise and determine whether AI is needed.
pub fn analyze_residual(
    cleaned_samples: &[f32],
    noise_profile: &NoiseProfile,
    activity: Option<&ActivityReport>,
    sample_rate: u32,
) -> Result<ResidualReport, DspError> {
    if cleaned_samples.is_empty() {
        return Ok(ResidualReport {
            residual_noise_dbfs: -120.0,
            original_noise_dbfs: noise_profile.noise_floor_dbfs,
            speech_rms_dbfs: -120.0,
            post_dsp_snr_db: 0.0,
            noise_attenuation_db: 0.0,
            residual_score: 0.0,
            level: ResidualLevel::Low,
            decision: DenoiseDecision::FinishWithoutAi,
            explanation: "Audio is empty.".to_string(),
        });
    }

    let frame_len = (sample_rate as usize * 20) / 1000; // 20 ms window
    let hop_len = frame_len / 2; // 10 ms hop
    let num_frames = if cleaned_samples.len() >= frame_len {
        (cleaned_samples.len() - frame_len) / hop_len + 1
    } else {
        1
    };

    let mut speech_powers = Vec::new();
    let mut pause_powers = Vec::new();

    // Segment frames into active speech vs pause frames
    for f in 0..num_frames {
        let start = f * hop_len;
        let end = (start + frame_len).min(cleaned_samples.len());
        let seg = &cleaned_samples[start..end];
        let p: f32 = seg.iter().map(|&s| s * s).sum::<f32>() / seg.len() as f32;

        let is_speech = if let Some(act) = activity {
            let act_idx = (f * act.frame_confidences.len()) / num_frames.max(1);
            if act_idx < act.frame_confidences.len() {
                act.frame_confidences[act_idx] >= 0.35
            } else {
                act.overall_confidence >= 0.35
            }
        } else {
            // Adaptive relative fallback: +6 dB (approx 4x power) above calibrated noise floor
            let noise_floor_linear = 10.0f32.powf(noise_profile.noise_floor_dbfs / 10.0);
            let speech_thresh = (noise_floor_linear * 3.98).max(1e-6);
            p > speech_thresh
        };

        if is_speech {
            speech_powers.push(p);
        } else {
            pause_powers.push(p);
        }
    }

    // Determine whether active speech is present in the recording
    let speech_detected = if let Some(act) = activity {
        act.active_frame_ratio >= 0.05 && !speech_powers.is_empty()
    } else {
        !speech_powers.is_empty() && (speech_powers.len() as f32 / num_frames as f32 >= 0.05)
    };

    // Measure residual noise RMS from pause frames (or quietest 10% if continuous sound)
    let residual_power = if !pause_powers.is_empty() {
        pause_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // Use median of pause frames to resist sudden mic bumps
        pause_powers[pause_powers.len() / 2]
    } else {
        // If sound was continuous without pauses, estimate from lowest 10% of frames
        speech_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        speech_powers[speech_powers.len() / 10]
    };

    let residual_rms = residual_power.max(1e-12).sqrt();
    let residual_noise_dbfs = 20.0 * residual_rms.log10();

    // Measure speech RMS and SNR when speech is detected; otherwise report clean floor
    let (speech_rms_dbfs, post_dsp_snr_db) = if speech_detected {
        let speech_power = speech_powers.iter().sum::<f32>() / speech_powers.len() as f32;
        let s_rms = speech_power.max(1e-12).sqrt();
        let s_dbfs = 20.0 * s_rms.log10();
        let snr = (s_dbfs - residual_noise_dbfs).max(0.0);
        (s_dbfs, snr)
    } else {
        (-120.0, 0.0)
    };

    let original_noise_dbfs = noise_profile.noise_floor_dbfs;
    let noise_attenuation_db = (original_noise_dbfs - residual_noise_dbfs).max(0.0);

    // Calculate normalized residual score in [0.0, 1.0]:
    // -60 dBFS -> 0.0 (near silence)
    // -25 dBFS -> 1.0 (very loud noise)
    let residual_score = ((residual_noise_dbfs + 60.0) / 35.0).clamp(0.0, 1.0);

    // Decision Logic based on Section 35 & 70 of architecture.md:
    // When speech is present:
    //   Low: Residual noise is studio quiet (<= -62 dBFS, or <= -58 dBFS when original was <= -46 dBFS with high SNR) -> Finish without AI
    //   Moderate: Residual noise between -58 dBFS and -42 dBFS, or original noise was loud (> -46 dBFS) -> Neural AI recommended
    //   High: Residual noise > -42 dBFS or SNR < 14 dB -> Neural AI required
    // When no speech is present (pure ambient / quiet room):
    //   Low: Residual noise <= -58 dBFS -> Finish without AI
    //   Moderate: Residual noise between -58 dBFS and -42 dBFS -> Neural AI recommended
    //   High: Residual noise > -42 dBFS -> Neural AI required
    let (level, decision, explanation) = if speech_detected {
        let is_clean_without_ai = (residual_noise_dbfs <= -62.0 && post_dsp_snr_db >= 28.0)
            || (residual_noise_dbfs <= -58.0 && original_noise_dbfs <= -46.0 && post_dsp_snr_db >= 25.0);

        if is_clean_without_ai {
            (
                ResidualLevel::Low,
                DenoiseDecision::FinishWithoutAi,
                format!(
                    "Residual noise is very quiet ({:.1} dBFS, SNR: {:.1} dB). Classical DSP is sufficient; AI not needed.",
                    residual_noise_dbfs, post_dsp_snr_db
                ),
            )
        } else if residual_noise_dbfs <= -42.0 && post_dsp_snr_db >= 14.0 {
            (
                ResidualLevel::Moderate,
                DenoiseDecision::InvokeNeuralBackend,
                format!(
                    "Noticeable residual noise remains ({:.1} dBFS post-DSP, original was {:.1} dBFS). Neural AI backend (Phase 6: DPDFNet2) recommended to eliminate in-band noise.",
                    residual_noise_dbfs, original_noise_dbfs
                ),
            )
        } else {
            (
                ResidualLevel::High,
                DenoiseDecision::InvokeNeuralBackend,
                format!(
                    "Significant background noise remains ({:.1} dBFS, SNR: {:.1} dB). Neural AI backend (Phase 6: DPDFNet2) required to eliminate in-band noise.",
                    residual_noise_dbfs, post_dsp_snr_db
                ),
            )
        }
    } else if residual_noise_dbfs <= -58.0 {
        (
            ResidualLevel::Low,
            DenoiseDecision::FinishWithoutAi,
            format!(
                "No vocal activity detected. Residual background noise is very quiet ({:.1} dBFS). Classical DSP is sufficient; AI not needed.",
                residual_noise_dbfs
            ),
        )
    } else if residual_noise_dbfs <= -42.0 {
        (
            ResidualLevel::Moderate,
            DenoiseDecision::InvokeNeuralBackend,
            format!(
                "No vocal activity detected. Moderate residual noise remains ({:.1} dBFS). Neural AI backend recommended.",
                residual_noise_dbfs
            ),
        )
    } else {
        (
            ResidualLevel::High,
            DenoiseDecision::InvokeNeuralBackend,
            format!(
                "No vocal activity detected. Significant residual noise remains ({:.1} dBFS). Neural AI backend required.",
                residual_noise_dbfs
            ),
        )
    };

    Ok(ResidualReport {
        residual_noise_dbfs,
        original_noise_dbfs,
        speech_rms_dbfs,
        post_dsp_snr_db,
        noise_attenuation_db,
        residual_score,
        level,
        decision,
        explanation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;
    use std::path::Path;

    #[test]
    fn test_residual_decision_low_noise_finishes_without_ai() {
        let sample_rate = 48000;
        let total_samples = 48000; // 1 second

        // Clean audio with very quiet noise (-65 dBFS) and clear voice (-18 dBFS)
        let mut clean_audio = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let hiss = 0.0005 * ((i % 100) as f32 / 100.0 - 0.5); // ~ -70 dBFS
            let voice = if (12000..36000).contains(&i) {
                0.25 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            } else {
                0.0
            };
            clean_audio.push(voice + hiss);
        }

        let engine = StftEngine::default_48k().unwrap();
        let noise_spec = engine.forward(&clean_audio[..12000]).unwrap();
        let profile = NoiseProfile::from_spectrogram(&noise_spec, &clean_audio[..12000], sample_rate);

        let report = analyze_residual(&clean_audio, &profile, None, sample_rate).unwrap();

        println!(
            "Clean test: residual={:.1} dBFS, SNR={:.1} dB, level={:?}, decision={:?}",
            report.residual_noise_dbfs, report.post_dsp_snr_db, report.level, report.decision
        );

        assert_eq!(report.level, ResidualLevel::Low);
        assert_eq!(report.decision, DenoiseDecision::FinishWithoutAi);
    }

    #[test]
    fn test_residual_decision_high_noise_invokes_neural_ai() {
        let sample_rate = 48000;
        let total_samples = 48000;

        // Noisy audio where residual noise is still loud (-35 dBFS)
        let mut noisy_audio = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let hiss = 0.035 * ((i % 100) as f32 / 100.0 - 0.5); // ~ -35 dBFS
            let voice = if (12000..36000).contains(&i) {
                0.15 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            } else {
                0.0
            };
            noisy_audio.push(voice + hiss);
        }

        let engine = StftEngine::default_48k().unwrap();
        let noise_spec = engine.forward(&noisy_audio[..12000]).unwrap();
        let profile = NoiseProfile::from_spectrogram(&noise_spec, &noisy_audio[..12000], sample_rate);

        let report = analyze_residual(&noisy_audio, &profile, None, sample_rate).unwrap();

        println!(
            "Noisy test: residual={:.1} dBFS, SNR={:.1} dB, level={:?}, decision={:?}",
            report.residual_noise_dbfs, report.post_dsp_snr_db, report.level, report.decision
        );

        assert_eq!(report.decision, DenoiseDecision::InvokeNeuralBackend);
    }

    #[test]
    fn test_residual_decision_speech_free_quiet_audio_finishes_without_ai() {
        let sample_rate = 48000;
        let total_samples = 48000;

        // Quiet background recording with NO speech (-58 dBFS)
        let mut quiet_audio = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let hiss = 0.001 * ((i % 53) as f32 / 53.0 - 0.5); // ~ -60 dBFS
            quiet_audio.push(hiss);
        }

        let engine = StftEngine::default_48k().unwrap();
        let noise_spec = engine.forward(&quiet_audio[..12000]).unwrap();
        let profile = NoiseProfile::from_spectrogram(&noise_spec, &quiet_audio[..12000], sample_rate);

        let report = analyze_residual(&quiet_audio, &profile, None, sample_rate).unwrap();

        assert_eq!(report.level, ResidualLevel::Low);
        assert_eq!(report.decision, DenoiseDecision::FinishWithoutAi);
        assert!(report.explanation.contains("No vocal activity detected"));
    }

    #[test]
    fn test_residual_real_audio_if_present() {
        let orig_path = Path::new("../../recordings/original.wav");
        let noise_ref_path = Path::new("../../recordings/noise_reference.wav");
        let dsp_cleaned_path = Path::new("../../recordings/dsp_cleaned.wav");

        if noise_ref_path.exists() && dsp_cleaned_path.exists() {
            let (noise_samples, _) = audio_core::read_wav_canonical_f32(noise_ref_path).unwrap();
            let (cleaned_samples, _) = audio_core::read_wav_canonical_f32(dsp_cleaned_path).unwrap();

            let engine = StftEngine::default_48k().unwrap();
            let noise_spec = engine.forward(&noise_samples).unwrap();
            let profile = NoiseProfile::from_spectrogram(&noise_spec, &noise_samples, 48000);

            let activity = if orig_path.exists() {
                let (orig_samples, _) = audio_core::read_wav_canonical_f32(orig_path).unwrap();
                let orig_spec = engine.forward(&orig_samples).unwrap();
                Some(crate::activity::detect_activity(&orig_spec, &profile, &crate::activity::ActivityConfig::default()))
            } else {
                None
            };

            let report = analyze_residual(&cleaned_samples, &profile, activity.as_ref(), 48000).unwrap();
            println!(
                "Real audio residual test:\n  Floor: {:.1} dBFS\n  Speech RMS: {:.1} dBFS\n  SNR: {:.1} dB\n  Attenuation: {:.1} dB\n  Level: {:?}\n  Decision: {:?}\n  Explanation: {}",
                report.residual_noise_dbfs, report.speech_rms_dbfs, report.post_dsp_snr_db,
                report.noise_attenuation_db, report.level, report.decision, report.explanation
            );
            assert!(
                report.decision == DenoiseDecision::InvokeNeuralBackend
                    || report.decision == DenoiseDecision::FinishWithoutAi
            );
        }
    }
}
