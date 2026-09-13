//! Noise stationarity analysis.
//!
//! Evaluates the temporal stability and spectral consistency of an audio signal
//! to determine whether background noise is stationary (e.g. steady fan, AC, computer hiss)
//! or non-stationary (e.g. changing traffic, transient bursts, intermittent speech).

use crate::stft::Spectrogram;

/// Detailed report on audio signal stationarity.
#[derive(Debug, Clone, PartialEq)]
pub struct StationarityReport {
    /// Stationarity score S in [0.0, 1.0], where 1.0 is perfectly stationary and 0.0 is highly bursty/transient.
    pub stationarity_score: f32,
    /// Average frame-to-frame spectral flux (normalized sub-band distance).
    pub spectral_flux_mean: f32,
    /// Spectral shape stability (mean cosine similarity across sub-band envelopes, 0.0 to 1.0).
    pub spectral_similarity: f32,
    /// Coefficient of variation of frame energy (std_dev / mean).
    pub energy_variation: f32,
    /// Boolean flag indicating whether the noise is considered stationary (score >= 0.60).
    pub is_stationary: bool,
}

/// Computes the stationarity metric from a spectrogram using sub-band spectral envelope tracking
/// and frame energy stability.
///
/// Acceptance criteria:
/// - Fan recording / steady hiss -> high stationarity (S >= 0.75).
/// - Changing traffic / transient bursts -> low stationarity (S <= 0.40).
pub fn analyze_stationarity(spectrogram: &Spectrogram) -> StationarityReport {
    let num_frames = spectrogram.num_frames();
    if num_frames < 2 {
        return StationarityReport {
            stationarity_score: 1.0,
            spectral_flux_mean: 0.0,
            spectral_similarity: 1.0,
            energy_variation: 0.0,
            is_stationary: true,
        };
    }

    let num_bins = spectrogram.num_bins();
    const NUM_SUBBANDS: usize = 32;

    let mut frame_energies_db = Vec::with_capacity(num_frames);
    let mut normalized_envelopes = Vec::with_capacity(num_frames);

    for frame in &spectrogram.frames {
        let mut total_energy = 0.0f32;
        let mut subband_energies = vec![0.0f32; NUM_SUBBANDS];

        // Skip bin 0 (DC component) to prevent static/infrasonic drift from corrupting acoustic analysis
        for (k, c) in frame.iter().enumerate().skip(1) {
            let p = c.norm_sqr();
            total_energy += p;

            let band_idx = (k * NUM_SUBBANDS / num_bins).min(NUM_SUBBANDS - 1);
            subband_energies[band_idx] += p;
        }

        let energy_db = 10.0 * (total_energy.max(1e-12)).log10();
        frame_energies_db.push(energy_db);

        // Compute Euclidean L2 norm of the sub-band energy envelope
        let env_norm_sq: f32 = subband_energies.iter().map(|&e| e * e).sum();
        let env_norm = env_norm_sq.sqrt().max(1e-10);

        for e in &mut subband_energies {
            *e /= env_norm;
        }
        normalized_envelopes.push(subband_energies);
    }

    // Frame energy mean in dB
    let mean_db = frame_energies_db.iter().sum::<f32>() / num_frames as f32;

    // Digital silence is stationary by definition
    if mean_db < -110.0 {
        return StationarityReport {
            stationarity_score: 1.0,
            spectral_flux_mean: 0.0,
            spectral_similarity: 1.0,
            energy_variation: 0.0,
            is_stationary: true,
        };
    }

    let var_db = frame_energies_db
        .iter()
        .map(|&e| (e - mean_db).powi(2))
        .sum::<f32>()
        / num_frames as f32;
    let std_dev_db = var_db.sqrt();

    // Envelope similarity & flux between consecutive frames
    let mut similarity_sum = 0.0f32;
    let mut flux_sum = 0.0f32;

    for t in 1..num_frames {
        let prev = &normalized_envelopes[t - 1];
        let curr = &normalized_envelopes[t];

        let mut dot = 0.0f32;
        let mut diff_sqr = 0.0f32;
        for b in 0..NUM_SUBBANDS {
            dot += prev[b] * curr[b];
            diff_sqr += (curr[b] - prev[b]).powi(2);
        }

        similarity_sum += dot.clamp(0.0, 1.0);
        flux_sum += diff_sqr.sqrt();
    }

    let frame_transitions = (num_frames - 1) as f32;
    let mean_similarity = (similarity_sum / frame_transitions).clamp(0.0, 1.0);
    let mean_flux = flux_sum / frame_transitions;

    // Logarithmic energy stability:
    // Natural stationary environments (ceiling fan, hiss, AC) typically have std_dev_db in [1.0, 3.5] dB -> energy_stability >= 0.75
    // Transient bursts or passing traffic have std_dev_db in [15.0, 50.0] dB -> energy_stability <= 0.20
    let excess_std = (std_dev_db - 2.0).max(0.0);
    let energy_stability = (1.0 / (1.0 + 0.15 * excess_std.powf(1.4))).clamp(0.0, 1.0);

    // Stationarity score S = S_envelope * S_energy
    let score = (mean_similarity * energy_stability).clamp(0.0, 1.0);
    let is_stationary = score >= 0.60;

    StationarityReport {
        stationarity_score: score,
        spectral_flux_mean: mean_flux,
        spectral_similarity: mean_similarity,
        energy_variation: std_dev_db,
        is_stationary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;

    #[test]
    fn test_stationary_noise_high_score() {
        let engine = StftEngine::default_48k().unwrap();
        let total_samples = 48000; // 1 second

        // Steady noise (stationary fan simulation)
        let mut signal = Vec::with_capacity(total_samples);
        let mut seed = 123456789u32;
        for _ in 0..total_samples {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = ((seed as f32 / u32::MAX as f32) - 0.5) * 0.1;
            signal.push(val);
        }

        let spec = engine.forward(&signal).unwrap();
        let report = analyze_stationarity(&spec);

        println!(
            "Stationary noise score: {:.3}, similarity: {:.3}, cv_energy: {:.3}",
            report.stationarity_score, report.spectral_similarity, report.energy_variation
        );
        assert!(
            report.stationarity_score >= 0.75,
            "Fan/steady hiss must have high stationarity (got {:.3})",
            report.stationarity_score
        );
        assert!(report.is_stationary);
    }

    #[test]
    fn test_transient_bursts_low_score() {
        let engine = StftEngine::default_48k().unwrap();
        let total_samples = 48000; // 1 second

        // Mostly silence with sharp transient bursts (traffic / taps simulation)
        let mut signal = vec![0.0f32; total_samples];
        // Inject sudden loud bursts
        for burst_idx in [5000, 15000, 30000] {
            for i in 0..800 {
                if burst_idx + i < total_samples {
                    signal[burst_idx + i] = ((i as f32 * 0.1).sin()) * 0.8;
                }
            }
        }

        let spec = engine.forward(&signal).unwrap();
        let report = analyze_stationarity(&spec);

        println!(
            "Transient bursts score: {:.3}, similarity: {:.3}, cv_energy: {:.3}",
            report.stationarity_score, report.spectral_similarity, report.energy_variation
        );
        assert!(
            report.stationarity_score <= 0.40,
            "Transient bursts/changing traffic must have low stationarity (got {:.3})",
            report.stationarity_score
        );
        assert!(!report.is_stationary);
    }
}
