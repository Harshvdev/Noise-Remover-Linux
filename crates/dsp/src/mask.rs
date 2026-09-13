//! 2D Time-Frequency Mask Smoothing.
//!
//! Smooths attenuation masks across frequency bins and time frames to eliminate
//! isolated single-bin spikes (which manifest as metallic/musical noise) and prevent
//! choppy, unnatural vocal decays.

/// Configuration for 2D mask smoothing.
#[derive(Debug, Clone)]
pub struct MaskSmootherConfig {
    /// Attack factor for time smoothing (0.0 to 1.0). Higher = faster response to speech onsets.
    pub attack_factor: f32,
    /// Release factor for time smoothing (0.0 to 1.0). Lower = smoother decay for reverberant tails.
    pub release_factor: f32,
    /// Weight for center bin in 3-tap frequency smoothing (e.g. 0.60).
    pub center_weight: f32,
}

impl Default for MaskSmootherConfig {
    fn default() -> Self {
        Self {
            attack_factor: 0.85,
            release_factor: 0.40,
            center_weight: 0.60,
        }
    }
}

/// 2D Mask Smoother.
#[derive(Debug, Clone)]
pub struct MaskSmoother {
    config: MaskSmootherConfig,
    prev_gains: Vec<f32>,
}

impl MaskSmoother {
    pub fn new(config: MaskSmootherConfig) -> Self {
        Self {
            config,
            prev_gains: Vec::new(),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(MaskSmootherConfig::default())
    }

    /// Reset internal time-smoothing history.
    pub fn reset(&mut self) {
        self.prev_gains.clear();
    }

    /// Smooth a 2D matrix of gains (frames x bins) in both frequency and time dimensions.
    pub fn smooth_mask(&mut self, raw_mask: &[Vec<f32>]) -> Vec<Vec<f32>> {
        if raw_mask.is_empty() {
            return Vec::new();
        }

        let num_frames = raw_mask.len();
        let num_bins = raw_mask[0].len();
        let side_weight = (1.0 - self.config.center_weight) * 0.5;

        let mut smoothed_mask = Vec::with_capacity(num_frames);
        let mut prev_frame = if self.prev_gains.len() == num_bins {
            self.prev_gains.clone()
        } else {
            vec![1.0f32; num_bins]
        };

        for frame in raw_mask {
            // 1. Frequency smoothing: 3-tap kernel [side, center, side]
            let mut freq_smoothed = vec![0.0f32; num_bins];
            for k in 0..num_bins {
                let left = if k > 0 { frame[k - 1] } else { frame[k] };
                let right = if k + 1 < num_bins { frame[k + 1] } else { frame[k] };
                let center = frame[k];

                freq_smoothed[k] = side_weight * left + self.config.center_weight * center + side_weight * right;
            }

            // 2. Time smoothing: asymmetric attack / release
            let mut time_smoothed = vec![0.0f32; num_bins];
            for k in 0..num_bins {
                let curr = freq_smoothed[k];
                let prev = prev_frame[k];

                let coeff = if curr >= prev {
                    self.config.attack_factor
                } else {
                    self.config.release_factor
                };

                let val = coeff * curr + (1.0 - coeff) * prev;
                time_smoothed[k] = val.clamp(0.0, 1.0);
            }

            prev_frame.copy_from_slice(&time_smoothed);
            smoothed_mask.push(time_smoothed);
        }

        self.prev_gains = prev_frame;
        smoothed_mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolated_spike_smoothing() {
        let mut smoother = MaskSmoother::with_default_config();

        // 1 frame of all zeros except one isolated spike of 1.0 in bin 10
        let mut frame = vec![0.0f32; 32];
        frame[10] = 1.0;

        let smoothed = smoother.smooth_mask(&[frame]);
        assert_eq!(smoothed.len(), 1);

        // Center bin should be reduced from 1.0 down toward center_weight (~0.60)
        assert!(smoothed[0][10] < 0.90);
        // Adjacent bins should now have non-zero energy to eliminate harsh discontinuity
        assert!(smoothed[0][9] > 0.05);
        assert!(smoothed[0][11] > 0.05);
    }

    #[test]
    fn test_temporal_smoothing_attack_release() {
        let mut smoother = MaskSmoother::with_default_config();

        // Sudden drop from 1.0 to 0.0 across 5 frames
        let frames = vec![
            vec![1.0f32; 16],
            vec![0.0f32; 16],
            vec![0.0f32; 16],
            vec![0.0f32; 16],
            vec![0.0f32; 16],
        ];

        let smoothed = smoother.smooth_mask(&frames);

        // Frame 1 (first drop) should not drop instantly to 0.0 because of release factor
        assert!(smoothed[1][5] > 0.30, "Release must be smooth, got {:.3}", smoothed[1][5]);
        // Over subsequent frames it should decay gradually
        assert!(smoothed[2][5] < smoothed[1][5]);
        assert!(smoothed[3][5] < smoothed[2][5]);
    }
}
