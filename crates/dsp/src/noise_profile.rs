//! Compact noise profile representation derived from calibration audio.
//!
//! Converts calibration audio (noise_reference.wav) into:
//! 1. Robust median Power Spectral Density (PSD) per frequency bin: N[k].
//! 2. Variance per bin.
//! 3. Noise floor in dBFS.
//! 4. Stationarity score S in [0.0, 1.0].
//! 5. Detected persistent tonal peaks (e.g., 50/60 Hz mains hum).

use crate::stft::Spectrogram;
use crate::stationarity::{analyze_stationarity, StationarityReport};
use crate::tonal::{detect_tonal_peaks, find_mains_hum_peaks, TonalDetectionConfig, TonalPeak};

/// Compact representation of the acoustic noise profile.
#[derive(Debug, Clone)]
pub struct NoiseProfile {
    pub sample_rate: u32,
    pub window_size: usize,
    pub hop_size: usize,

    /// Robust median noise Power Spectral Density (PSD) per bin: N[k].
    /// Using median ensures brief accidental thumps/coughs during calibration
    /// do not corrupt the noise estimate.
    pub psd: Vec<f32>,

    /// Mean noise power per bin across all calibration frames.
    pub mean_psd: Vec<f32>,

    /// Variance of power per bin across calibration frames.
    pub variance: Vec<f32>,

    /// Estimated overall noise floor in dBFS (relative to 1.0 full-scale peak).
    pub noise_floor_dbfs: f32,

    /// Stationarity score S in [0.0, 1.0] (1.0 = steady fan/hiss, 0.0 = bursty/transient).
    pub stationarity_score: f32,

    /// Detailed stationarity analysis report.
    pub stationarity_report: StationarityReport,

    /// Detected persistent tonal peaks.
    pub tonal_peaks: Vec<TonalPeak>,
}

impl NoiseProfile {
    /// Build a robust noise profile from a computed spectrogram and raw calibration samples.
    pub fn from_spectrogram(
        spectrogram: &Spectrogram,
        raw_samples: &[f32],
        sample_rate: u32,
    ) -> Self {
        let num_frames = spectrogram.num_frames();
        let num_bins = spectrogram.num_bins();

        // Calculate acoustic RMS and dBFS using a 1-pole DC blocker (R = 0.995)
        // to ignore sub-audible infrasonic air turbulence (< 35 Hz) and DC bias
        let noise_floor_dbfs = if !raw_samples.is_empty() {
            let mut prev_in = 0.0f32;
            let mut prev_out = 0.0f32;
            let mut sum_sq = 0.0f32;
            for &s in raw_samples {
                let filtered = s - prev_in + 0.995 * prev_out;
                prev_in = s;
                prev_out = filtered;
                sum_sq += filtered * filtered;
            }
            let rms = (sum_sq / raw_samples.len() as f32).sqrt();
            20.0 * (rms.max(1e-12)).log10()
        } else {
            -120.0
        };

        if num_frames == 0 {
            let empty_vec = vec![0.0f32; num_bins];
            let report = StationarityReport {
                stationarity_score: 1.0,
                spectral_flux_mean: 0.0,
                spectral_similarity: 1.0,
                energy_variation: 0.0,
                is_stationary: true,
            };
            return Self {
                sample_rate,
                window_size: spectrogram.window_size,
                hop_size: spectrogram.hop_size,
                psd: empty_vec.clone(),
                mean_psd: empty_vec.clone(),
                variance: empty_vec,
                noise_floor_dbfs,
                stationarity_score: 1.0,
                stationarity_report: report,
                tonal_peaks: Vec::new(),
            };
        }

        // Robust median PSD, mean PSD, and variance per frequency bin
        let mut median_psd = Vec::with_capacity(num_bins);
        let mut mean_psd = Vec::with_capacity(num_bins);
        let mut variance = Vec::with_capacity(num_bins);

        let mut bin_powers = Vec::with_capacity(num_frames);

        for k in 0..num_bins {
            bin_powers.clear();
            let mut sum = 0.0f32;

            for frame in &spectrogram.frames {
                let p = frame[k].norm_sqr();
                bin_powers.push(p);
                sum += p;
            }

            let mean = sum / num_frames as f32;
            mean_psd.push(mean);

            // Median calculation
            bin_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = if num_frames % 2 == 1 {
                bin_powers[num_frames / 2]
            } else {
                0.5 * (bin_powers[num_frames / 2 - 1] + bin_powers[num_frames / 2])
            };
            median_psd.push(median);

            // Variance calculation
            let var = bin_powers
                .iter()
                .map(|&p| (p - mean).powi(2))
                .sum::<f32>()
                / num_frames as f32;
            variance.push(var);
        }

        // Stationarity analysis
        let stationarity_report = analyze_stationarity(spectrogram);
        let stationarity_score = stationarity_report.stationarity_score;

        // Persistent tonal peak detection
        let tonal_config = TonalDetectionConfig::default();
        let tonal_peaks = detect_tonal_peaks(
            &median_psd,
            Some(spectrogram),
            sample_rate,
            spectrogram.window_size,
            &tonal_config,
        );

        Self {
            sample_rate,
            window_size: spectrogram.window_size,
            hop_size: spectrogram.hop_size,
            psd: median_psd,
            mean_psd,
            variance,
            noise_floor_dbfs,
            stationarity_score,
            stationarity_report,
            tonal_peaks,
        }
    }

    /// Frequency corresponding to a given FFT bin index.
    pub fn bin_to_frequency(&self, bin: usize) -> f32 {
        bin as f32 * self.sample_rate as f32 / self.window_size as f32
    }

    /// Nearest FFT bin index for a given frequency in Hz.
    pub fn frequency_to_bin(&self, freq_hz: f32) -> usize {
        let bin = (freq_hz * self.window_size as f32 / self.sample_rate as f32).round() as usize;
        bin.min(self.psd.len().saturating_sub(1))
    }

    /// Whether the background noise is classified as stationary.
    pub fn is_stationary(&self) -> bool {
        self.stationarity_report.is_stationary
    }

    /// Check for detected mains hum (50 Hz or 60 Hz).
    pub fn mains_hum_peaks(&self, tolerance_hz: f32) -> Vec<&TonalPeak> {
        find_mains_hum_peaks(&self.tonal_peaks, tolerance_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;

    #[test]
    fn test_clean_recording_low_estimated_noise() {
        let sample_rate = 48000;
        let engine = StftEngine::default_48k().unwrap();
        // Clean quiet recording (RMS around -70 dBFS)
        let total_samples = 48000;
        let signal = vec![0.0001f32; total_samples];

        let spec = engine.forward(&signal).unwrap();
        let profile = NoiseProfile::from_spectrogram(&spec, &signal, sample_rate);

        println!("Clean recording noise floor: {:.2} dBFS (acceptance <= -55 dBFS)", profile.noise_floor_dbfs);
        assert!(
            profile.noise_floor_dbfs <= -55.0,
            "Clean silence must have noise floor <= -55 dBFS (got {:.2})",
            profile.noise_floor_dbfs
        );
    }

    #[test]
    fn test_median_resistance_to_accidental_thumps() {
        let sample_rate = 48000;
        let engine = StftEngine::default_48k().unwrap();
        let total_samples = 48000;

        // Base steady noise
        let mut clean_noise = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let val = ((i % 13) as f32 / 13.0 - 0.5) * 0.02;
            clean_noise.push(val);
        }

        let mut noisy_with_thump = clean_noise.clone();
        // Inject a sudden thump/cough spike in one frame (~500 samples)
        for sample in &mut noisy_with_thump[12000..12500] {
            *sample += 0.9;
        }

        let spec_clean = engine.forward(&clean_noise).unwrap();
        let prof_clean = NoiseProfile::from_spectrogram(&spec_clean, &clean_noise, sample_rate);

        let spec_thump = engine.forward(&noisy_with_thump).unwrap();
        let prof_thump = NoiseProfile::from_spectrogram(&spec_thump, &noisy_with_thump, sample_rate);

        // Robust median PSD should be virtually identical between clean noise and thump-corrupted noise
        let mut max_psd_diff = 0.0f32;
        for (a, b) in prof_clean.psd.iter().zip(prof_thump.psd.iter()) {
            let diff = (a - b).abs();
            if diff > max_psd_diff {
                max_psd_diff = diff;
            }
        }

        println!("Max median PSD difference with thump: {:.6}", max_psd_diff);
        assert!(max_psd_diff < 0.01, "Median PSD must resist transient thump artifacts");
    }
}

