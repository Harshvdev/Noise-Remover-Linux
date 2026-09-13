//! Unified noise and signal analyzer facade.
//!
//! Provides high-level analysis workflows:
//! - Analyzing calibration audio (noise_reference.wav) into a `NoiseProfile`.
//! - Analyzing voice audio (original.wav) against the noise profile.

use crate::activity::{detect_activity, ActivityConfig, ActivityReport};
use crate::error::DspError;
use crate::noise_profile::NoiseProfile;
use crate::stationarity::{analyze_stationarity, StationarityReport};
use crate::stft::StftEngine;

/// Comprehensive analysis report for a recorded speech/audio signal.
#[derive(Debug, Clone)]
pub struct SignalAnalysisReport {
    pub sample_rate: u32,
    pub duration_seconds: f32,
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
    pub activity: ActivityReport,
    pub stationarity: StationarityReport,
}

/// Unified analyzer engine managing STFT computation and DSP diagnostic extractors.
pub struct NoiseAnalyzer {
    stft: StftEngine,
    sample_rate: u32,
}

impl NoiseAnalyzer {
    /// Create a new analyzer with default STFT parameters (1024-point window, 256 hop size for 75% overlap).
    pub fn new(sample_rate: u32) -> Result<Self, DspError> {
        let stft = StftEngine::default_48k()?;
        Ok(Self { stft, sample_rate })
    }

    /// Create an analyzer with custom STFT configuration.
    pub fn with_stft(stft: StftEngine, sample_rate: u32) -> Self {
        Self { stft, sample_rate }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stft(&self) -> &StftEngine {
        &self.stft
    }

    /// Analyze a calibration audio segment (noise_reference.wav) to produce a `NoiseProfile`.
    pub fn analyze_noise_reference(&self, samples: &[f32]) -> Result<NoiseProfile, DspError> {
        let spec = self.stft.forward(samples)?;
        Ok(NoiseProfile::from_spectrogram(&spec, samples, self.sample_rate))
    }

    /// Analyze an audio signal (e.g. original speech recording) in reference to a calibrated `NoiseProfile`.
    pub fn analyze_signal(
        &self,
        signal: &[f32],
        profile: &NoiseProfile,
    ) -> Result<SignalAnalysisReport, DspError> {
        let spec = self.stft.forward(signal)?;

        let sum_sq: f32 = signal.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / signal.len().max(1) as f32).sqrt();
        let rms_dbfs = 20.0 * (rms.max(1e-12)).log10();

        let peak = signal.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));
        let peak_dbfs = 20.0 * (peak.max(1e-12)).log10();

        let activity_config = ActivityConfig::default();
        let activity = detect_activity(&spec, profile, &activity_config);
        let stationarity = analyze_stationarity(&spec);

        let duration_seconds = signal.len() as f32 / self.sample_rate as f32;

        Ok(SignalAnalysisReport {
            sample_rate: self.sample_rate,
            duration_seconds,
            rms_dbfs,
            peak_dbfs,
            activity,
            stationarity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_noise_analyzer_end_to_end() {
        let sample_rate = 48000;
        let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();

        // 1. Calibration noise: 50 Hz hum + fan hiss
        let total_samples = 48000;
        let mut noise = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let hum = (2.0 * PI * 50.0 * t).sin() * 0.3;
            let hiss = ((i % 17) as f32 / 17.0 - 0.5) * 0.02;
            noise.push(hum + hiss);
        }

        let profile = analyzer.analyze_noise_reference(&noise).unwrap();
        assert!(profile.stationarity_score >= 0.70, "Hum + hiss must be stationary");
        assert!(!profile.tonal_peaks.is_empty(), "Must detect 50 Hz hum peak");
        assert!(!profile.mains_hum_peaks(5.0).is_empty(), "Must detect mains hum within 5 Hz");

        // 2. Speech recording over background noise
        let mut vocal_recording = noise.clone();
        for (offset, sample) in vocal_recording[10000..35000].iter_mut().enumerate() {
            let i = 10000 + offset;
            let t = i as f32 / sample_rate as f32;
            let voice = (2.0 * PI * 400.0 * t).sin() * 0.4;
            *sample += voice;
        }

        let report = analyzer.analyze_signal(&vocal_recording, &profile).unwrap();
        assert_eq!(report.sample_rate, sample_rate);
        assert!(report.duration_seconds >= 0.99);
        assert!(report.activity.overall_confidence > 0.35);
        assert!(report.activity.vocal_snr_db > 5.0);
    }
}

