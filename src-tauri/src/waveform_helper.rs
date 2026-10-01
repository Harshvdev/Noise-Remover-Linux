//! Waveform downsampling helper for peak visualization.

pub fn compute_waveform_peaks(samples: &[f32], target_buckets: usize) -> Vec<f32> {
    if samples.is_empty() || target_buckets == 0 {
        return vec![0.1; target_buckets.max(1)];
    }

    let chunk_size = (samples.len() as f64 / target_buckets as f64).max(1.0);
    let mut raw_peaks = Vec::with_capacity(target_buckets);
    let mut max_val: f32 = 0.001;

    for i in 0..target_buckets {
        let start = (i as f64 * chunk_size).floor() as usize;
        let end = (((i + 1) as f64 * chunk_size).ceil() as usize).min(samples.len());

        let mut peak: f32 = 0.0;
        if start < end && start < samples.len() {
            for &s in &samples[start..end] {
                let abs = s.abs();
                if abs > peak {
                    peak = abs;
                }
            }
        }
        if peak > max_val {
            max_val = peak;
        }
        raw_peaks.push(peak);
    }

    // Normalize with floor for aesthetic display
    raw_peaks
        .into_iter()
        .map(|p| (p / max_val).clamp(0.0, 1.0) * 0.95 + 0.05)
        .collect()
}
