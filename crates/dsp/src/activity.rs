//! Basic vocal/activity confidence detector.
//!
//! Evaluates signal frames against the calibrated noise profile to estimate:
//! 1. A posteriori SNR in vocal bands (250 Hz – 4000 Hz).
//! 2. Per-frame vocal activity confidence in [0.0, 1.0].
//! 3. Overall voice presence confidence and active frame ratio.

use crate::noise_profile::NoiseProfile;
use crate::stft::Spectrogram;

/// Configuration for vocal activity detection.
#[derive(Debug, Clone)]
pub struct ActivityConfig {
    /// Lower frequency limit for human voice band in Hz (default: 250.0).
    pub vocal_low_hz: f32,
    /// Upper frequency limit for human voice band in Hz (default: 4000.0).
    pub vocal_high_hz: f32,
    /// SNR threshold in dB above noise floor for 50% activity probability (default: 3.0 dB).
    pub snr_threshold_db: f32,
}

impl Default for ActivityConfig {
    fn default() -> Self {
        Self {
            vocal_low_hz: 250.0,
            vocal_high_hz: 4000.0,
            snr_threshold_db: 3.0,
        }
    }
}

/// Report containing activity detection diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityReport {
    /// Overall vocal activity confidence in [0.0, 1.0].
    pub overall_confidence: f32,
    /// Estimated vocal-band Signal-to-Noise Ratio (SNR) in dB.
    pub vocal_snr_db: f32,
    /// Ratio of frames classified as active voice (0.0 to 1.0).
    pub active_frame_ratio: f32,
    /// Per-frame vocal activity confidence scores in [0.0, 1.0].
    pub frame_confidences: Vec<f32>,
}

/// Detect vocal activity in a spectrogram using a calibrated noise profile.
pub fn detect_activity(
    spectrogram: &Spectrogram,
    noise_profile: &NoiseProfile,
    config: &ActivityConfig,
) -> ActivityReport {
    let num_frames = spectrogram.num_frames();
    if num_frames == 0 {
        return ActivityReport {
            overall_confidence: 0.0,
            vocal_snr_db: 0.0,
            active_frame_ratio: 0.0,
            frame_confidences: Vec::new(),
        };
    }

    let bin_start = noise_profile.frequency_to_bin(config.vocal_low_hz).max(1);
    let bin_end = noise_profile
        .frequency_to_bin(config.vocal_high_hz)
        .min(spectrogram.num_bins().saturating_sub(1));

    if bin_start >= bin_end {
        return ActivityReport {
            overall_confidence: 0.0,
            vocal_snr_db: 0.0,
            active_frame_ratio: 0.0,
            frame_confidences: vec![0.0; num_frames],
        };
    }

    // Baseline noise power in the vocal band
    let noise_vocal_power: f32 = noise_profile.psd[bin_start..=bin_end].iter().sum();
    let noise_ref = noise_vocal_power.max(1e-12);

    let bin_sub_end = noise_profile.frequency_to_bin(80.0).min(bin_start);
    let noise_sub_power: f32 = noise_profile.psd[0..=bin_sub_end].iter().sum();
    let noise_sub_ref = noise_sub_power.max(1e-12);

    let mut frame_confidences = Vec::with_capacity(num_frames);
    let mut active_frames_count = 0;
    let mut total_active_snr = 0.0f32;

    for frame in &spectrogram.frames {
        let signal_vocal_power: f32 = frame[bin_start..=bin_end]
            .iter()
            .map(|c| c.norm_sqr())
            .sum();

        let power_ratio = signal_vocal_power / noise_ref;
        let snr_db = 10.0 * (power_ratio.max(1e-6)).log10();

        // Smooth sigmoid confidence: c = 1 / (1 + exp(-0.6 * (snr_db - threshold_db)))
        let x = 0.6 * (snr_db - config.snr_threshold_db);
        let mut conf = 1.0 / (1.0 + (-x).exp());

        // Breath / Wind rejection:
        // In breath puffs and microphone wind turbulence, infrasonic energy (< 80 Hz)
        // surges massively above the calibrated noise baseline and exceeds the vocal band.
        if bin_sub_end > 0 {
            let sub_power: f32 = frame[0..=bin_sub_end].iter().map(|c| c.norm_sqr()).sum();
            let is_uncalibrated_surge = sub_power > 4.0 * noise_sub_ref;

            if is_uncalibrated_surge && sub_power > 3.0 * signal_vocal_power.max(1e-12) && sub_power > 1e-4 {
                let breath_penalty = (signal_vocal_power / (sub_power + 1e-6)).clamp(0.0, 1.0);
                conf *= breath_penalty;
            }
        }

        if conf >= 0.5 {
            active_frames_count += 1;
            total_active_snr += snr_db;
        }

        frame_confidences.push(conf);
    }

    let active_frame_ratio = active_frames_count as f32 / num_frames as f32;
    let avg_vocal_snr = if active_frames_count > 0 {
        total_active_snr / active_frames_count as f32
    } else {
        0.0
    };

    // Overall confidence reflects both active frame density and SNR
    let snr_factor = (avg_vocal_snr / 12.0).clamp(0.0, 1.0);
    let overall_confidence = if active_frame_ratio > 0.05 {
        (0.6 * active_frame_ratio + 0.4 * snr_factor).clamp(0.0, 1.0)
    } else {
        0.0
    };

    ActivityReport {
        overall_confidence,
        vocal_snr_db: avg_vocal_snr,
        active_frame_ratio,
        frame_confidences,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;
    use std::f32::consts::PI;

    #[test]
    fn test_vocal_activity_detection() {
        let sample_rate = 48000;
        let engine = StftEngine::default_48k().unwrap();
        let total_samples = 48000; // 1 second

        // 1. Generate background noise calibration (low amplitude hiss)
        let mut noise = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let val = ((i % 19) as f32 / 19.0 - 0.5) * 0.01;
            noise.push(val);
        }
        let spec_noise = engine.forward(&noise).unwrap();
        let profile = NoiseProfile::from_spectrogram(&spec_noise, &noise, sample_rate);

        // 2. Generate signal with speech-like tones in the vocal range (500 Hz + 1500 Hz)
        let mut speech_signal = noise.clone();
        for (offset, sample) in speech_signal[12000..36000].iter_mut().enumerate() {
            let i = 12000 + offset;
            let t = i as f32 / sample_rate as f32;
            let formant1 = (2.0 * PI * 500.0 * t).sin() * 0.2;
            let formant2 = (2.0 * PI * 1500.0 * t).sin() * 0.1;
            *sample += formant1 + formant2;
        }

        let spec_speech = engine.forward(&speech_signal).unwrap();
        let config = ActivityConfig::default();
        let report = detect_activity(&spec_speech, &profile, &config);

        println!(
            "Speech detection: confidence={:.2}, SNR={:.2} dB, active_ratio={:.2}",
            report.overall_confidence, report.vocal_snr_db, report.active_frame_ratio
        );

        assert!(report.overall_confidence > 0.40, "Must detect vocal activity");
        assert!(report.vocal_snr_db > 6.0, "Vocal SNR must be positive and significant");
        assert!(report.active_frame_ratio > 0.35, "Vocal frames must be detected");

        // 3. Compare with pure noise (same as calibration)
        let report_noise = detect_activity(&spec_noise, &profile, &config);
        println!(
            "Noise only: confidence={:.2}, SNR={:.2} dB, active_ratio={:.2}",
            report_noise.overall_confidence, report_noise.vocal_snr_db, report_noise.active_frame_ratio
        );
        assert!(report_noise.overall_confidence < 0.10, "Noise only must not be flagged as speech");
    }
}

