//! Atomic real-time audio level and clipping monitor.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct AudioMeter {
    /// Peak linear amplitude stored as f32 bits.
    peak_bits: AtomicU32,
    /// RMS linear amplitude stored as f32 bits.
    rms_bits: AtomicU32,
    /// Total clipped sample counter.
    clipping_count: AtomicU32,
    /// Previous input sample for 1-pole DC blocker.
    prev_in_bits: AtomicU32,
    /// Previous output sample for 1-pole DC blocker.
    prev_out_bits: AtomicU32,
}

impl AudioMeter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            peak_bits: AtomicU32::new(0.0f32.to_bits()),
            rms_bits: AtomicU32::new(0.0f32.to_bits()),
            clipping_count: AtomicU32::new(0),
            prev_in_bits: AtomicU32::new(0.0f32.to_bits()),
            prev_out_bits: AtomicU32::new(0.0f32.to_bits()),
        })
    }

    /// Update meter metrics with a buffer of samples. Real-time safe: no allocations or blocking.
    /// Incorporates a 1-pole DC blocker (R = 0.995, fc ~ 38 Hz) to eliminate sub-audible air turbulence
    /// and static ADC offset from biasing live visual metering.
    #[inline]
    pub fn update(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut clipped = 0u32;

        let mut prev_in = f32::from_bits(self.prev_in_bits.load(Ordering::Relaxed));
        let mut prev_out = f32::from_bits(self.prev_out_bits.load(Ordering::Relaxed));

        for &s in samples {
            // 1-pole DC blocker: y[n] = x[n] - x[n-1] + 0.995 * y[n-1]
            let filtered = s - prev_in + 0.995 * prev_out;
            prev_in = s;
            prev_out = filtered;

            let abs = filtered.abs();
            if abs > peak {
                peak = abs;
            }
            if abs >= 0.999 || s.abs() >= 0.999 {
                clipped += 1;
            }
            sum_sq += filtered * filtered;
        }

        self.prev_in_bits.store(prev_in.to_bits(), Ordering::Relaxed);
        self.prev_out_bits.store(prev_out.to_bits(), Ordering::Relaxed);

        let cur_ms = sum_sq / samples.len() as f32;
        let prev_peak = f32::from_bits(self.peak_bits.load(Ordering::Relaxed));
        let prev_rms = f32::from_bits(self.rms_bits.load(Ordering::Relaxed));

        // Peak ballistics: instant rise, smooth decay
        let new_peak = if peak >= prev_peak {
            peak
        } else {
            (prev_peak * 0.92).max(peak)
        };

        // RMS leaky integrator (approx 150ms time constant)
        let prev_ms = prev_rms * prev_rms;
        let smoothed_ms = prev_ms * 0.85 + cur_ms * 0.15;
        let new_rms = smoothed_ms.sqrt();

        self.peak_bits.store(new_peak.to_bits(), Ordering::Relaxed);
        self.rms_bits.store(new_rms.to_bits(), Ordering::Relaxed);
        if clipped > 0 {
            self.clipping_count.fetch_add(clipped, Ordering::Relaxed);
        }
    }

    /// Returns peak level in dBFS (range -96.0 to 0.0).
    pub fn peak_dbfs(&self) -> f32 {
        let linear = f32::from_bits(self.peak_bits.load(Ordering::Relaxed));
        if linear <= 1e-5 {
            -96.0
        } else {
            (20.0 * linear.log10()).clamp(-96.0, 0.0)
        }
    }

    /// Returns RMS level in dBFS.
    pub fn rms_dbfs(&self) -> f32 {
        let linear = f32::from_bits(self.rms_bits.load(Ordering::Relaxed));
        if linear <= 1e-5 {
            -96.0
        } else {
            (20.0 * linear.log10()).clamp(-96.0, 0.0)
        }
    }

    /// Returns whether any clipping occurred and resets the counter.
    pub fn take_clipping(&self) -> bool {
        self.clipping_count.swap(0, Ordering::Relaxed) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meter_peak_and_decay() {
        let meter = AudioMeter::new();
        assert_eq!(meter.peak_dbfs(), -96.0);

        // Peak signal
        meter.update(&[0.5, -0.8, 0.2]);
        let peak1 = meter.peak_dbfs();
        assert!(peak1 > -3.0 && peak1 < -1.0); // 0.8 is approx -1.93 dBFS

        // Subsequent quieter signal triggers decay
        meter.update(&[0.05, -0.05]);
        let peak2 = meter.peak_dbfs();
        assert!(peak2 < peak1); // decayed
        assert!(peak2 > -6.0); // didn't drop immediately to -26 dBFS
    }

    #[test]
    fn test_meter_clipping() {
        let meter = AudioMeter::new();
        assert!(!meter.take_clipping());

        meter.update(&[0.5, 1.0, -0.2]);
        assert!(meter.take_clipping());
        // Reset after take
        assert!(!meter.take_clipping());
    }
}
