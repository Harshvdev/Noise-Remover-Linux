//! Window functions for time-frequency STFT analysis.

use std::f32::consts::PI;

/// Generate a periodic Hann (Hanning) window of length `size`.
///
/// Periodic Hann: $w[n] = 0.5 \cdot (1 - \cos(2\pi n / N))$ for $n = 0..N$.
/// When used with 75% overlap (hop = size / 4) or 50% overlap (hop = size / 2),
/// this provides optimal spectral leakage suppression and clean overlap-add reconstruction.
pub fn hann_window(size: usize) -> Vec<f32> {
    if size == 0 {
        return Vec::new();
    }
    if size == 1 {
        return vec![1.0];
    }

    let mut window = Vec::with_capacity(size);
    let n_float = size as f32;
    for i in 0..size {
        let val = 0.5 * (1.0 - (2.0 * PI * i as f32 / n_float).cos());
        window.push(val);
    }
    window
}

/// Compute the overlap-add normalization weight buffer $\sum_m w^2[n - mH]$
/// to guarantee perfect reconstruction across the entire synthesized signal length.
pub fn compute_ola_normalization(signal_len: usize, window: &[f32], hop_size: usize) -> Vec<f32> {
    let mut weights = vec![0.0f32; signal_len];
    let win_len = window.len();
    let mut pos = 0;

    while pos + win_len <= signal_len {
        for i in 0..win_len {
            weights[pos + i] += window[i] * window[i];
        }
        pos += hop_size;
    }

    // Replace near-zero weights with 1.0 to prevent division by zero at edges
    for w in weights.iter_mut() {
        if *w < 1e-6 {
            *w = 1.0;
        }
    }

    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hann_window_bounds() {
        let win = hann_window(1024);
        assert_eq!(win.len(), 1024);
        assert_eq!(win[0], 0.0);
        // Middle should be close to 1.0
        assert!((win[512] - 1.0).abs() < 1e-3);
        for &v in &win {
            assert!((0.0..=1.0).contains(&v));
        }
    }
}
