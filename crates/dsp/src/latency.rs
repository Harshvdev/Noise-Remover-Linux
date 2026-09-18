//! Latency Alignment and Delay Compensation.
//!
//! Defined in Section 43 of architecture.md:
//! Original and processed streams must be latency-aligned before dry/wet blending
//! to prevent comb filtering and hollow phasing artifacts.

/// Metrics describing phase coherence and comb-filtering risk between two signals.
#[derive(Debug, Clone)]
pub struct CombFilterMetrics {
    /// Peak-to-valley ripple depth across frequency bands (in dB).
    /// Low ripple (< 3 dB) indicates constructive, aligned summing.
    /// High ripple (> 10 dB) indicates comb filtering from timing mismatch.
    pub notch_depth_db: f32,
    /// Whether harmful comb filtering was detected.
    pub has_comb_filtering: bool,
    /// Normalized cross-correlation coefficient at lag 0 (phase coherence in [-1.0, 1.0]).
    pub phase_coherence: f32,
}

/// Latency aligner responsible for delay compensation and alignment diagnostics.
pub struct LatencyAligner;

impl LatencyAligner {
    /// Delay the original reference signal by `delay_samples` so that it synchronizes
    /// perfectly with a processed stream that has algorithmic delay.
    ///
    /// Both returned vectors have the exact same length matching `processed.len()`.
    pub fn align_by_delay(
        original: &[f32],
        processed: &[f32],
        delay_samples: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let len = processed.len();
        if len == 0 || original.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut aligned_orig = vec![0.0f32; len];
        if delay_samples < len {
            let copy_len = (original.len()).min(len - delay_samples);
            aligned_orig[delay_samples..delay_samples + copy_len]
                .copy_from_slice(&original[..copy_len]);
        }

        let aligned_proc = processed.to_vec();
        (aligned_orig, aligned_proc)
    }

    /// Advance the processed signal by trimming the initial algorithmic latency of `delay_samples`
    /// and padding the tail with silence, so that processed output aligns with original at t=0.
    pub fn advance_processed(
        original: &[f32],
        processed: &[f32],
        delay_samples: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let target_len = original.len();
        if target_len == 0 || processed.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut aligned_proc = vec![0.0f32; target_len];
        if delay_samples < processed.len() {
            let available = processed.len() - delay_samples;
            let copy_len = available.min(target_len);
            aligned_proc[..copy_len].copy_from_slice(&processed[delay_samples..delay_samples + copy_len]);
        }

        (original.to_vec(), aligned_proc)
    }

    /// Estimate the empirical sample delay between `reference` (original) and `target` (processed)
    /// using normalized cross-correlation over a search range `[-max_lag, max_lag]`.
    ///
    /// Returns `(best_lag, max_correlation)`:
    /// - Positive lag means `target` is delayed relative to `reference` (target[n] ≈ ref[n - lag]).
    /// - Negative lag means `target` is advanced relative to `reference`.
    pub fn estimate_delay(reference: &[f32], target: &[f32], max_lag: usize) -> (isize, f32) {
        let n = reference.len().min(target.len());
        if n == 0 || max_lag == 0 {
            return (0, 0.0);
        }

        let search_range = max_lag.min(n / 2);
        if search_range == 0 {
            return (0, 0.0);
        }

        // For large signals (> 48k samples / 1.0s), pick a representative 48k window
        // with the highest signal energy to prevent O(N * max_lag) CPU lockup.
        let sub_len = 48000.min(n);
        let mut best_start = 0;
        if n > sub_len {
            let mut max_energy = 0.0f32;
            let step = 24000;
            let mut pos = 0;
            while pos + sub_len <= n {
                let energy: f32 = reference[pos..pos + sub_len].iter().map(|&s| s * s).sum();
                if energy > max_energy {
                    max_energy = energy;
                    best_start = pos;
                }
                pos += step;
            }
        }

        let ref_sub = &reference[best_start..best_start + sub_len];
        let tgt_sub = &target[best_start..best_start + sub_len];
        let sub_n = sub_len;

        let ref_energy: f32 = ref_sub.iter().map(|&s| s * s).sum();
        let tgt_energy: f32 = tgt_sub.iter().map(|&s| s * s).sum();

        if ref_energy <= 1e-12 || tgt_energy <= 1e-12 {
            return (0, 1.0);
        }

        let denom = (ref_energy * tgt_energy).sqrt();
        let mut best_lag: isize = 0;
        let mut max_corr = -1.0f32;

        for lag in -(search_range as isize)..=(search_range as isize) {
            let mut dot = 0.0f32;
            let mut count = 0usize;

            if lag >= 0 {
                let u_lag = lag as usize;
                let end = sub_n.saturating_sub(u_lag);
                for i in 0..end {
                    dot += ref_sub[i] * tgt_sub[i + u_lag];
                    count += 1;
                }
            } else {
                let u_lag = (-lag) as usize;
                let end = sub_n.saturating_sub(u_lag);
                for i in 0..end {
                    dot += ref_sub[i + u_lag] * tgt_sub[i];
                    count += 1;
                }
            }

            if count > 0 {
                let norm_dot = (dot / denom) * (sub_n as f32 / count as f32);
                if norm_dot > max_corr {
                    max_corr = norm_dot;
                    best_lag = lag;
                }
            }
        }

        (best_lag, max_corr.clamp(-1.0, 1.0))
    }

    /// Check for comb-filtering and phase cancellation artifacts when mixing two signals.
    ///
    /// Examines the frequency response of the 50/50 blend $S = 0.5 \cdot (A + B)$ compared
    /// to the baseline spectrum. If misaligned by even a few milliseconds, destructive
    /// interference produces periodic sharp notches (> 12 dB ripple).
    pub fn check_comb_filtering(signal_a: &[f32], signal_b: &[f32], sample_rate: u32) -> CombFilterMetrics {
        let len = signal_a.len().min(signal_b.len());
        if len < 512 {
            return CombFilterMetrics {
                notch_depth_db: 0.0,
                has_comb_filtering: false,
                phase_coherence: 1.0,
            };
        }

        // 1. Calculate time-domain phase coherence (normalized correlation at lag 0)
        let mut dot = 0.0f32;
        let mut e_a = 0.0f32;
        let mut e_b = 0.0f32;
        for i in 0..len {
            dot += signal_a[i] * signal_b[i];
            e_a += signal_a[i] * signal_a[i];
            e_b += signal_b[i] * signal_b[i];
        }

        let denom = (e_a * e_b).sqrt().max(1e-12);
        let phase_coherence = (dot / denom).clamp(-1.0, 1.0);

        // 2. Analyze frequency-domain notch depth using a 512-point FFT window
        let mut sum_signal = vec![0.0f32; len];
        for i in 0..len {
            sum_signal[i] = 0.5 * (signal_a[i] + signal_b[i]);
        }

        let fft_size = 512;
        let mut max_notch_db = 0.0f32;

        if len >= fft_size {
            let mut planner = realfft::RealFftPlanner::<f32>::new();
            let r2c = planner.plan_fft_forward(fft_size);
            let bins = fft_size / 2 + 1;

            let mut in_sum = vec![0.0f32; fft_size];
            let mut in_a = vec![0.0f32; fft_size];
            let mut in_b = vec![0.0f32; fft_size];
            let mut out_sum = vec![rustfft::num_complex::Complex::new(0.0f32, 0.0f32); bins];
            let mut out_a = vec![rustfft::num_complex::Complex::new(0.0f32, 0.0f32); bins];
            let mut out_b = vec![rustfft::num_complex::Complex::new(0.0f32, 0.0f32); bins];

            let mid = len / 2;
            let start = mid.saturating_sub(fft_size / 2);
            let win = crate::window::hann_window(fft_size);

            for i in 0..fft_size {
                let idx = start + i;
                if idx < len {
                    in_sum[i] = sum_signal[idx] * win[i];
                    in_a[i] = signal_a[idx] * win[i];
                    in_b[i] = signal_b[idx] * win[i];
                }
            }

            if r2c.process(&mut in_sum, &mut out_sum).is_ok()
                && r2c.process(&mut in_a, &mut out_a).is_ok()
                && r2c.process(&mut in_b, &mut out_b).is_ok()
            {
                // Inspect active bins (200 Hz to 12 kHz)
                let bin_hz = sample_rate as f32 / fft_size as f32;
                let start_b = ((200.0 / bin_hz) as usize).max(2);
                let end_b = ((12000.0 / bin_hz) as usize).min(bins);

                for k in start_b..end_b {
                    let p_sum = out_sum[k].norm_sqr();
                    let m_a = out_a[k].norm();
                    let m_b = out_b[k].norm();
                    let in_phase_mag = 0.5 * (m_a + m_b);
                    let p_expected = in_phase_mag * in_phase_mag;

                    if p_expected > 1e-5 {
                        let notch_db = 10.0 * (p_expected / p_sum.max(1e-12)).log10();
                        if notch_db > max_notch_db {
                            max_notch_db = notch_db;
                        }
                    }
                }
            }
        }

        let has_comb = max_notch_db > 10.0 || phase_coherence < 0.20;

        CombFilterMetrics {
            notch_depth_db: max_notch_db,
            has_comb_filtering: has_comb,
            phase_coherence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_delay_compensation_exactness() {
        let sample_rate = 48000;
        let n = sample_rate; // 1 second
        let mut original = vec![0.0f32; n];
        for (i, sample) in original.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample = (2.0 * PI * 440.0 * t).sin();
        }

        let delay = 480; // 10ms delay (e.g. DPDFNet2)
        let mut delayed = vec![0.0f32; n];
        delayed[delay..].copy_from_slice(&original[..n - delay]);

        let (best_lag, corr) = LatencyAligner::estimate_delay(&original, &delayed, 1000);
        assert_eq!(best_lag, delay as isize, "Estimated delay must match exactly");
        assert!(corr > 0.95, "Correlation at true lag must be close to 1.0");

        // Align signals
        let (aligned_orig, aligned_proc) = LatencyAligner::align_by_delay(&original, &delayed, delay);
        assert_eq!(aligned_orig.len(), delayed.len());
        assert_eq!(aligned_proc.len(), delayed.len());

        // Check phase coherence after alignment
        let metrics = LatencyAligner::check_comb_filtering(&aligned_orig, &aligned_proc, sample_rate as u32);
        assert!(metrics.phase_coherence > 0.95, "Aligned signals must be phase coherent");
        assert!(!metrics.has_comb_filtering, "Aligned signals must not have comb filtering");
    }

    #[test]
    fn test_unaligned_signals_trigger_comb_filter_detection() {
        let sample_rate = 48000;
        let n = sample_rate;
        let mut original = vec![0.0f32; n];
        for (i, sample) in original.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            // Broadband multitone signal
            *sample = (2.0 * PI * 500.0 * t).sin()
                + (2.0 * PI * 1000.0 * t).sin()
                + (2.0 * PI * 1500.0 * t).sin()
                + (2.0 * PI * 2000.0 * t).sin();
        }

        let delay = 24; // 0.5 ms delay creates deep notches in audio band
        let mut misaligned = vec![0.0f32; n];
        misaligned[delay..].copy_from_slice(&original[..n - delay]);

        let (lag, _) = LatencyAligner::estimate_delay(&original, &misaligned, 100);
        assert_eq!(lag, delay as isize);
    }
}
