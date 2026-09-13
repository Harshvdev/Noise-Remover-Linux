//! Detection of persistent tonal peaks (e.g. 50/60 Hz electrical hum and harmonics).

use crate::stft::Spectrogram;

/// Represents a detected tonal peak in the frequency spectrum.
#[derive(Debug, Clone, PartialEq)]
pub struct TonalPeak {
    /// Estimated frequency in Hz (with parabolic sub-bin interpolation).
    pub frequency_hz: f32,
    /// Nearest FFT bin index.
    pub bin_index: usize,
    /// Peak strength/prominence over the local spectral floor in dB.
    pub strength_db: f32,
    /// Detection confidence in [0.0, 1.0] based on prominence and temporal persistence.
    pub confidence: f32,
}

/// Parameters for tonal peak detection.
#[derive(Debug, Clone)]
pub struct TonalDetectionConfig {
    /// Minimum prominence above local spectral floor in dB (default: 6.0 dB).
    pub min_prominence_db: f32,
    /// Number of neighboring bins on each side to estimate local noise floor (default: 6).
    pub neighbor_window_bins: usize,
    /// Minimum fraction of frames where peak must be active for persistence (default: 0.60).
    pub min_persistence_ratio: f32,
    /// Minimum frequency to inspect (default: 30.0 Hz to ignore sub-audible DC drift).
    pub min_freq_hz: f32,
    /// Maximum frequency to inspect (default: 4000.0 Hz).
    pub max_freq_hz: f32,
}

impl Default for TonalDetectionConfig {
    fn default() -> Self {
        Self {
            min_prominence_db: 6.0,
            neighbor_window_bins: 6,
            min_persistence_ratio: 0.60,
            min_freq_hz: 30.0,
            max_freq_hz: 4000.0,
        }
    }
}

/// Detect persistent tonal peaks from a spectrogram and its average PSD.
pub fn detect_tonal_peaks(
    psd: &[f32],
    spectrogram: Option<&Spectrogram>,
    sample_rate: u32,
    window_size: usize,
    config: &TonalDetectionConfig,
) -> Vec<TonalPeak> {
    let num_bins = psd.len();
    if num_bins < 3 {
        return Vec::new();
    }

    let bin_resolution = sample_rate as f32 / window_size as f32;

    // Convert PSD to dB
    let psd_db: Vec<f32> = psd
        .iter()
        .map(|&p| 10.0 * (p.max(1e-12)).log10())
        .collect();

    let mut candidate_peaks = Vec::new();

    let min_bin = ((config.min_freq_hz / bin_resolution).floor() as usize).max(1);
    let max_bin = ((config.max_freq_hz / bin_resolution).ceil() as usize).min(num_bins - 2);

    for k in min_bin..=max_bin {
        let val = psd_db[k];
        // Check if local maximum
        if val > psd_db[k - 1] && val >= psd_db[k + 1] {
            // Compute local noise floor from surrounding bins (excluding immediate neighbors)
            let start = k.saturating_sub(config.neighbor_window_bins).max(1);
            let end = (k + config.neighbor_window_bins).min(num_bins - 1);

            let mut surrounding = Vec::with_capacity(end - start);
            for (i, &item) in psd_db.iter().enumerate().take(end + 1).skip(start) {
                // Skip immediate peak region [k-1, k, k+1]
                if i + 1 < k || i > k + 1 {
                    surrounding.push(item);
                }
            }

            if surrounding.is_empty() {
                continue;
            }

            // Use median of surrounding bins as robust local floor
            surrounding.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let local_floor = surrounding[surrounding.len() / 2];
            let prominence = val - local_floor;

            if prominence >= config.min_prominence_db {
                // Parabolic sub-bin interpolation for precise frequency estimation.
                // For bin 1 (adjacent to DC bin 0), cap alpha at psd_db[2] so arbitrary DC offset
                // cannot falsely drag 50/60 Hz mains hum down below bin 1.
                let alpha = if k == 1 {
                    psd_db[0].min(psd_db[2])
                } else {
                    psd_db[k - 1]
                };
                let beta = val;
                let gamma = psd_db[k + 1];
                let denom = alpha - 2.0 * beta + gamma;

                let delta = if denom.abs() > 1e-6 {
                    (0.5 * (alpha - gamma) / denom).clamp(-0.5, 0.5)
                } else {
                    0.0
                };

                let interpolated_bin = k as f32 + delta;
                let freq_hz = interpolated_bin * bin_resolution;

                // Frame persistence check if spectrogram is available
                let persistence = if let Some(spec) = spectrogram {
                    let total_frames = spec.num_frames();
                    if total_frames > 0 {
                        let mut active_frames = 0;
                        for frame in &spec.frames {
                            let f_val = 10.0 * (frame[k].norm_sqr().max(1e-12)).log10();
                            let f_prev = 10.0 * (frame[k - 1].norm_sqr().max(1e-12)).log10();
                            let f_next = 10.0 * (frame[k + 1].norm_sqr().max(1e-12)).log10();
                            if f_val >= f_prev && f_val >= f_next {
                                active_frames += 1;
                            }
                        }
                        active_frames as f32 / total_frames as f32
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                if persistence >= config.min_persistence_ratio {
                    // Confidence is a combination of prominence and temporal persistence
                    let prominence_factor = (prominence / 20.0).clamp(0.0, 1.0);
                    let confidence = (0.5 * prominence_factor + 0.5 * persistence).clamp(0.0, 1.0);

                    candidate_peaks.push(TonalPeak {
                        frequency_hz: freq_hz,
                        bin_index: k,
                        strength_db: prominence,
                        confidence,
                    });
                }
            }
        }
    }

    // Sort by strength descending
    candidate_peaks.sort_by(|a, b| b.strength_db.partial_cmp(&a.strength_db).unwrap_or(std::cmp::Ordering::Equal));
    candidate_peaks
}

/// Helper to check if detected peaks contain mains hum (50 Hz or 60 Hz within tolerance).
pub fn find_mains_hum_peaks(peaks: &[TonalPeak], tolerance_hz: f32) -> Vec<&TonalPeak> {
    peaks
        .iter()
        .filter(|p| {
            let is_50 = (p.frequency_hz - 50.0).abs() <= tolerance_hz;
            let is_60 = (p.frequency_hz - 60.0).abs() <= tolerance_hz;
            let is_100 = (p.frequency_hz - 100.0).abs() <= tolerance_hz;
            let is_120 = (p.frequency_hz - 120.0).abs() <= tolerance_hz;
            is_50 || is_60 || is_100 || is_120
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;
    use std::f32::consts::PI;

    #[test]
    fn test_detect_50hz_and_60hz_hum() {
        let sample_rate = 48000;
        let engine = StftEngine::default_48k().unwrap();
        let duration_secs = 1.0;
        let total_samples = (sample_rate as f32 * duration_secs) as usize;

        // Generate synthetic signal: 50 Hz sine with 100 Hz harmonic + broadband background noise
        let mut signal = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let hum_50 = (2.0 * PI * 50.0 * t).sin() * 0.4;
            let hum_100 = (2.0 * PI * 100.0 * t).sin() * 0.15;
            // Small noise
            let noise = ((i % 17) as f32 / 17.0 - 0.5) * 0.01;
            signal.push(hum_50 + hum_100 + noise);
        }

        let spec = engine.forward(&signal).unwrap();
        let psd = spec.average_psd();
        let config = TonalDetectionConfig::default();

        let peaks = detect_tonal_peaks(&psd, Some(&spec), sample_rate, engine.window_size(), &config);

        assert!(!peaks.is_empty(), "Must detect peaks");
        let hum_peaks = find_mains_hum_peaks(&peaks, 5.0);
        assert!(!hum_peaks.is_empty(), "Must identify 50 Hz hum within 5 Hz");

        let primary_peak = hum_peaks[0];
        println!(
            "Detected primary hum peak: {:.2} Hz (strength: {:.2} dB, conf: {:.2})",
            primary_peak.frequency_hz, primary_peak.strength_db, primary_peak.confidence
        );
        assert!((primary_peak.frequency_hz - 50.0).abs() <= 5.0);
    }
}
