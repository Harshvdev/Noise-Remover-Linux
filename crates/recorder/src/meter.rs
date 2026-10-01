use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use realfft::RealFftPlanner;

pub const FFT_WINDOW_SIZE: usize = 2048;

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
    /// Lock-free circular sample ring for real-time spectral analysis.
    sample_ring: Box<[AtomicU32; FFT_WINDOW_SIZE]>,
    write_pos: AtomicUsize,
}

impl AudioMeter {
    pub fn new() -> Arc<Self> {
        let mut ring = Vec::with_capacity(FFT_WINDOW_SIZE);
        for _ in 0..FFT_WINDOW_SIZE {
            ring.push(AtomicU32::new(0));
        }
        let sample_ring: Box<[AtomicU32; FFT_WINDOW_SIZE]> = ring.into_boxed_slice().try_into().unwrap();

        Arc::new(Self {
            peak_bits: AtomicU32::new(0.0f32.to_bits()),
            rms_bits: AtomicU32::new(0.0f32.to_bits()),
            clipping_count: AtomicU32::new(0),
            prev_in_bits: AtomicU32::new(0.0f32.to_bits()),
            prev_out_bits: AtomicU32::new(0.0f32.to_bits()),
            sample_ring,
            write_pos: AtomicUsize::new(0),
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

        // Store samples into lock-free circular ring buffer for real-time FFT
        for &s in samples {
            let idx = self.write_pos.fetch_add(1, Ordering::Relaxed) % FFT_WINDOW_SIZE;
            self.sample_ring[idx].store(s.to_bits(), Ordering::Relaxed);
        }

        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut clipped = 0u32;

        let mut prev_in = f32::from_bits(self.prev_in_bits.load(Ordering::Relaxed));
        let mut prev_out = f32::from_bits(self.prev_out_bits.load(Ordering::Relaxed));
        if !prev_in.is_finite() {
            prev_in = 0.0;
        }
        if !prev_out.is_finite() {
            prev_out = 0.0;
        }

        for &s in samples {
            if !s.is_finite() {
                continue;
            }
            // 1-pole DC blocker: y[n] = x[n] - x[n-1] + 0.995 * y[n-1]
            let mut filtered = s - prev_in + 0.995 * prev_out;
            if !filtered.is_finite() {
                filtered = s;
            }
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
        if !linear.is_finite() || linear <= 1e-5 {
            -96.0
        } else {
            (20.0 * linear.log10()).clamp(-96.0, 0.0)
        }
    }

    /// Returns RMS level in dBFS.
    pub fn rms_dbfs(&self) -> f32 {
        let linear = f32::from_bits(self.rms_bits.load(Ordering::Relaxed));
        if !linear.is_finite() || linear <= 1e-5 {
            -96.0
        } else {
            (20.0 * linear.log10()).clamp(-96.0, 0.0)
        }
    }

    /// Returns whether any clipping occurred and resets the counter.
    pub fn take_clipping(&self) -> bool {
        self.clipping_count.swap(0, Ordering::Relaxed) > 0
    }

    /// Computes the genuine 128-band frequency spectrum corresponding to the 128 MIDI notes (0..127)
    /// using real-time FFT over the most recent hardware microphone samples.
    pub fn spectrum_128(&self, sample_rate: u32) -> Vec<f32> {
        let sr = if sample_rate > 0 { sample_rate as f32 } else { 48000.0 };
        let write_idx = self.write_pos.load(Ordering::Relaxed);
        let mut time_buf = [0.0f32; FFT_WINDOW_SIZE];

        // Read the most recent samples in chronological order
        for i in 0..FFT_WINDOW_SIZE {
            let idx = (write_idx + i) % FFT_WINDOW_SIZE;
            let s = f32::from_bits(self.sample_ring[idx].load(Ordering::Relaxed));
            // Hann window to eliminate spectral leakage
            let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_WINDOW_SIZE as f32 - 1.0)).cos());
            time_buf[i] = if s.is_finite() { s * window } else { 0.0 };
        }

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(FFT_WINDOW_SIZE);
        let mut out_spec = r2c.make_output_vec();
        if r2c.process(&mut time_buf, &mut out_spec).is_err() {
            return vec![0.0; 128];
        }

        let bin_hz = sr / FFT_WINDOW_SIZE as f32; // ~23.44 Hz per bin
        let mut result = Vec::with_capacity(128);

        const NOISE_GATE_DB: f32 = -70.0;
        const MAX_DB: f32 = -16.0;

        for m in 0..128 {
            // 128 MIDI notes (0 to 127): f(m) = 440 * 2^((m - 69) / 12)
            let f_center = 440.0 * 2.0f32.powf((m as f32 - 69.0) / 12.0);
            let f_low = 440.0 * 2.0f32.powf((m as f32 - 0.5 - 69.0) / 12.0);
            let f_high = 440.0 * 2.0f32.powf((m as f32 + 0.5 - 69.0) / 12.0);

            let k_start = ((f_low / bin_hz).floor() as usize).max(1);
            let k_end = ((f_high / bin_hz).ceil() as usize).max(k_start + 1).min(out_spec.len() - 1);

            let mut max_mag = 0.0f32;
            let mut sum_mag = 0.0f32;
            let mut count = 0;

            for k in k_start..=k_end {
                let c = out_spec[k];
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                if mag > max_mag {
                    max_mag = mag;
                }
                sum_mag += mag;
                count += 1;
            }

            let avg_mag = if count > 0 { sum_mag / count as f32 } else { 0.0 };
            let blended = 0.75 * max_mag + 0.25 * avg_mag;
            let norm_mag = blended / 1024.0;

            // Equal-loudness tilt compensation
            let norm_m = m as f32 / 127.0;
            let weight = 0.90 + norm_m.powf(0.72) * 0.55;

            // Sub-bass filter: Notes below 35Hz (< m=24)
            let sub_filter = if f_center < 18.0 {
                0.0
            } else if f_center < 40.0 {
                ((f_center - 18.0) / 22.0).powf(1.6)
            } else {
                1.0
            };

            let note_db = if norm_mag > 1e-6 {
                20.0 * norm_mag.log10()
            } else {
                -96.0
            };

            if note_db <= NOISE_GATE_DB {
                result.push(0.0);
            } else {
                let norm = ((note_db - NOISE_GATE_DB) / (MAX_DB - NOISE_GATE_DB)).clamp(0.0, 1.0);
                let val = (norm.powf(0.78) * 1.35 * weight * sub_filter).clamp(0.0, 1.0);
                result.push(val);
            }
        }

        result
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
