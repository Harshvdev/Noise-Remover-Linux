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
}

impl AudioMeter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            peak_bits: AtomicU32::new(0.0f32.to_bits()),
            rms_bits: AtomicU32::new(0.0f32.to_bits()),
            clipping_count: AtomicU32::new(0),
        })
    }

    /// Update meter metrics with a buffer of samples. Real-time safe: no allocations or blocking.
    #[inline]
    pub fn update(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut clipped = 0u32;

        for &s in samples {
            let abs = s.abs();
            if abs > peak {
                peak = abs;
            }
            if abs >= 0.999 {
                clipped += 1;
            }
            sum_sq += s * s;
        }

        let rms = (sum_sq / samples.len() as f32).sqrt();

        // Smooth decay or fast update
        self.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
        self.rms_bits.store(rms.to_bits(), Ordering::Relaxed);
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
