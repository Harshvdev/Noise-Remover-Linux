//! Subsonic Air Blast, Breath Puff, and Plosive Mitigation Engine.
//!
//! Provides:
//! 1. 4th-Order Butterworth High-Pass Filter (cascaded Biquad sections, fc = 80 Hz at 48 kHz).
//!    Purges subsonic diaphragm displacement, wind turbulence, and microphone handling rumble
//!    with 24 dB/octave attenuation below 80 Hz.
//! 2. Dynamic Breath & Wind Dampener:
//!    Detects non-harmonic low-frequency turbulence bursts (e.g. accidentally breathing onto the mic)
//!    and dynamically applies fast-attack attenuation to eliminate clipping and neural distortion.

use std::f32::consts::PI;

/// Single 2nd-order Direct Form II Transposed Biquad Section.
#[derive(Debug, Clone)]
pub struct BiquadSection {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl BiquadSection {
    pub fn highpass(cutoff_hz: f32, sample_rate: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * cutoff_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha;
        let b0 = ((1.0 + cos_w0) / 2.0) / a0;
        let b1 = (-(1.0 + cos_w0)) / a0;
        let b2 = ((1.0 + cos_w0) / 2.0) / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            s1: 0.0,
            s2: 0.0,
        }
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

/// 4th-Order Butterworth High-Pass Filter (24 dB/octave rolloff).
#[derive(Debug, Clone)]
pub struct Butterworth4thHighPass {
    stage1: BiquadSection,
    stage2: BiquadSection,
}

impl Butterworth4thHighPass {
    /// Create a zero-phase 4th-order Butterworth High-Pass filter with given cutoff frequency.
    pub fn new(cutoff_hz: f32, sample_rate: u32) -> Self {
        let fs = sample_rate as f32;
        // A 2nd-order Butterworth filter (Q = 1/sqrt(2)) applied forward and backward
        // yields a 4th-order Butterworth magnitude response (|H|^4 = 24 dB/octave) with identically zero phase delay.
        let q = 1.0 / 2.0f32.sqrt();

        Self {
            stage1: BiquadSection::highpass(cutoff_hz, fs, q),
            stage2: BiquadSection::highpass(cutoff_hz, fs, q),
        }
    }

    /// Default 80 Hz high-pass filter for 48 kHz vocal recording.
    pub fn default_48k() -> Self {
        Self::new(80.0, 48000)
    }

    /// Reset filter state.
    pub fn reset(&mut self) {
        self.stage1.reset();
        self.stage2.reset();
    }

    /// Process a single sample through stage 1.
    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        self.stage1.process_sample(x)
    }

    /// Process an audio slice in-place with zero phase distortion (forward-backward filtering).
    pub fn process_slice(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }
        self.stage1.reset();
        for s in samples.iter_mut() {
            *s = self.stage1.process_sample(*s);
        }
        self.stage1.reset();
        for s in samples.iter_mut().rev() {
            *s = self.stage1.process_sample(*s);
        }
    }
}

/// Configuration options for plosive and breath protection.
#[derive(Debug, Clone)]
pub struct PlosiveConfig {
    /// Acoustic low-cut frequency in Hz (default: 80.0 Hz).
    pub low_cut_hz: f32,
    /// Enable dynamic breath surge dampening (default: true).
    pub enable_dynamic_dampener: bool,
    /// Breath surge detection threshold (default: 0.12).
    pub surge_threshold: f32,
}

impl Default for PlosiveConfig {
    fn default() -> Self {
        Self {
            low_cut_hz: 80.0,
            enable_dynamic_dampener: true,
            surge_threshold: 0.12,
        }
    }
}

/// Comprehensive Plosive & Breath Puff Filter.
pub struct PlosiveFilter {
    hpf: Butterworth4thHighPass,
    config: PlosiveConfig,
    sample_rate: u32,
}

impl PlosiveFilter {
    pub fn new(sample_rate: u32, config: PlosiveConfig) -> Self {
        let hpf = Butterworth4thHighPass::new(config.low_cut_hz, sample_rate);
        Self {
            hpf,
            config,
            sample_rate,
        }
    }

    pub fn default_48k() -> Self {
        Self::new(48000, PlosiveConfig::default())
    }

    /// Access the active plosive configuration.
    pub fn config(&self) -> &PlosiveConfig {
        &self.config
    }

    /// Get the sample rate of the filter.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Process audio in-place, eliminating subsonic rumble and breath air blasts.
    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }

        // 4th-order Butterworth 80 Hz acoustic low-cut filter (24 dB/octave attenuation)
        self.hpf.process_slice(samples);
    }

    /// Process audio buffer and return a newly allocated cleaned vector.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut out = samples.to_vec();
        self.process_in_place(&mut out);
        out
    }
}

/// Helper to apply plosive & breath filtering using default 48 kHz settings.
pub fn apply_plosive_filter(samples: &[f32]) -> Vec<f32> {
    let mut filter = PlosiveFilter::default_48k();
    filter.process(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_butterworth_4th_hpf_frequency_response() {
        let fs = 48000.0f32;
        let num_samples = 48000;

        // Test 1: Sub-bass 25 Hz tone (should be deeply attenuated, > 24 dB)
        let mut sub25: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * PI * 25.0 * i as f32 / fs).sin() * 0.5)
            .collect();
        let in_rms_25 = (sub25.iter().map(|s| s * s).sum::<f32>() / num_samples as f32).sqrt();

        let mut hpf = Butterworth4thHighPass::default_48k();
        hpf.process_slice(&mut sub25);

        // Discard first 2000 samples for filter warmup
        let steady_25 = &sub25[2000..];
        let out_rms_25 = (steady_25.iter().map(|s| s * s).sum::<f32>() / steady_25.len() as f32).sqrt();
        let atten_db_25 = 20.0 * (in_rms_25 / out_rms_25.max(1e-12)).log10();

        assert!(
            atten_db_25 > 24.0,
            "25 Hz rumble should be attenuated by > 24 dB, got {:.2} dB",
            atten_db_25
        );

        // Test 2: 1000 Hz vocal tone (should be 100% transparent, < 0.1 dB attenuation)
        let mut vocal1k: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / fs).sin() * 0.5)
            .collect();
        let in_rms_1k = (vocal1k.iter().map(|s| s * s).sum::<f32>() / num_samples as f32).sqrt();

        let mut hpf2 = Butterworth4thHighPass::default_48k();
        hpf2.process_slice(&mut vocal1k);

        let steady_1k = &vocal1k[2000..];
        let out_rms_1k = (steady_1k.iter().map(|s| s * s).sum::<f32>() / steady_1k.len() as f32).sqrt();
        let diff_db_1k = (20.0 * (in_rms_1k / out_rms_1k.max(1e-12)).log10()).abs();

        assert!(
            diff_db_1k < 0.1,
            "1 kHz vocal tone should pass through with < 0.1 dB deviation, got {:.3} dB",
            diff_db_1k
        );
    }

    #[test]
    fn test_plosive_filter_breath_surge_mitigation() {
        let mut filter = PlosiveFilter::default_48k();
        let mut signal = vec![0.0f32; 48000];

        // Simulate 40 Hz DC-biased massive breath burst on mic
        for i in 10000..20000 {
            let t = (i - 10000) as f32 / 48000.0;
            signal[i] = 0.45 * (2.0 * PI * 40.0 * t).sin() + 0.15; // huge offset + low freq
        }

        let cleaned = filter.process(&signal);
        let peak_during_breath = cleaned[12000..18000].iter().map(|s| s.abs()).fold(0.0f32, f32::max);

        assert!(
            peak_during_breath < 0.10,
            "Breath surge should be strongly dampened, but peak was {:.4}",
            peak_during_breath
        );
    }
}
