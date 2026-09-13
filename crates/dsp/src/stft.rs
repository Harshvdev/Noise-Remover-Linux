//! Short-Time Fourier Transform (STFT) and Inverse STFT (ISTFT) engine.

use std::sync::Arc;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;

use crate::error::DspError;
use crate::window::{compute_ola_normalization, hann_window};

/// Spectrogram holding the complex time-frequency representation of an audio signal.
#[derive(Debug, Clone)]
pub struct Spectrogram {
    /// Vector of frames, each containing (window_size / 2 + 1) complex frequency bins.
    pub frames: Vec<Vec<Complex<f32>>>,
    pub window_size: usize,
    pub hop_size: usize,
    pub original_len: usize,
}

impl Spectrogram {
    pub fn num_frames(&self) -> usize {
        self.frames.len()
    }

    pub fn num_bins(&self) -> usize {
        self.window_size / 2 + 1
    }

    /// Calculate magnitude $|X[k]|$ for a specific frame index.
    pub fn frame_magnitude(&self, frame_idx: usize) -> Vec<f32> {
        if frame_idx >= self.frames.len() {
            return Vec::new();
        }
        self.frames[frame_idx].iter().map(|c| c.norm()).collect()
    }

    /// Calculate power $|X[k]|^2$ for a specific frame index.
    pub fn frame_power(&self, frame_idx: usize) -> Vec<f32> {
        if frame_idx >= self.frames.len() {
            return Vec::new();
        }
        self.frames[frame_idx].iter().map(|c| c.norm_sqr()).collect()
    }

    /// Compute the average Power Spectral Density (PSD) across all frames.
    pub fn average_psd(&self) -> Vec<f32> {
        let bins = self.num_bins();
        let total_frames = self.frames.len();
        if total_frames == 0 {
            return vec![0.0; bins];
        }

        let mut sum_power = vec![0.0f32; bins];
        for frame in &self.frames {
            for (k, c) in frame.iter().enumerate() {
                sum_power[k] += c.norm_sqr();
            }
        }

        let scale = 1.0 / total_frames as f32;
        for p in &mut sum_power {
            *p *= scale;
        }

        sum_power
    }
}

/// STFT Engine providing real-to-complex forward transform and synthesis inverse transform.
pub struct StftEngine {
    window_size: usize,
    hop_size: usize,
    window: Vec<f32>,
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
}

impl StftEngine {
    /// Create a new STFT engine with specified window size (e.g. 1024) and hop size (e.g. 256 for 75% overlap).
    pub fn new(window_size: usize, hop_size: usize) -> Result<Self, DspError> {
        if window_size == 0 || hop_size == 0 || hop_size > window_size {
            return Err(DspError::InvalidStftConfig(window_size, hop_size));
        }

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(window_size);
        let c2r = planner.plan_fft_inverse(window_size);
        let window = hann_window(window_size);

        Ok(Self {
            window_size,
            hop_size,
            window,
            r2c,
            c2r,
        })
    }

    /// Default configuration for 48 kHz voice analysis: 1024-point window (~21.3ms) with 75% overlap (hop 256).
    pub fn default_48k() -> Result<Self, DspError> {
        Self::new(1024, 256)
    }

    pub fn window_size(&self) -> usize {
        self.window_size
    }

    pub fn hop_size(&self) -> usize {
        self.hop_size
    }

    /// Forward Short-Time Fourier Transform.
    pub fn forward(&self, signal: &[f32]) -> Result<Spectrogram, DspError> {
        let original_len = signal.len();
        if original_len < self.window_size {
            return Err(DspError::BufferTooShort(original_len, self.window_size));
        }

        let bins = self.window_size / 2 + 1;
        let mut frames = Vec::new();
        let mut pos = 0;

        let mut time_scratch = vec![0.0f32; self.window_size];
        let mut freq_scratch = vec![Complex::new(0.0f32, 0.0f32); bins];

        while pos < original_len {
            for i in 0..self.window_size {
                let sample = if pos + i < original_len {
                    signal[pos + i]
                } else {
                    0.0
                };
                time_scratch[i] = sample * self.window[i];
            }

            self.r2c
                .process(&mut time_scratch, &mut freq_scratch)
                .map_err(|e| DspError::FftError(format!("{:?}", e)))?;

            frames.push(freq_scratch.clone());
            pos += self.hop_size;
        }

        Ok(Spectrogram {
            frames,
            window_size: self.window_size,
            hop_size: self.hop_size,
            original_len,
        })
    }

    /// Inverse Short-Time Fourier Transform (Synthesis) with Overlap-Add (OLA) and window normalization.
    pub fn inverse(&self, spec: &Spectrogram) -> Result<Vec<f32>, DspError> {
        if spec.frames.is_empty() {
            return Ok(Vec::new());
        }

        let num_frames = spec.frames.len();
        let total_output_len = (num_frames - 1) * self.hop_size + self.window_size;
        let mut output = vec![0.0f32; total_output_len];

        let bins = self.window_size / 2 + 1;
        let mut freq_scratch = vec![Complex::new(0.0f32, 0.0f32); bins];
        let mut time_scratch = vec![0.0f32; self.window_size];

        let fft_scale = 1.0 / self.window_size as f32;

        let mut pos = 0;
        for frame in &spec.frames {
            freq_scratch.copy_from_slice(frame);

            self.c2r
                .process(&mut freq_scratch, &mut time_scratch)
                .map_err(|e| DspError::FftError(format!("{:?}", e)))?;

            for i in 0..self.window_size {
                let sample = time_scratch[i] * fft_scale * self.window[i];
                output[pos + i] += sample;
            }

            pos += self.hop_size;
        }

        // Apply OLA normalization weights
        let weights = compute_ola_normalization(total_output_len, &self.window, self.hop_size);
        for (out, &w) in output.iter_mut().zip(weights.iter()) {
            *out /= w;
        }

        // Truncate to match original signal length if specified
        if spec.original_len > 0 && output.len() > spec.original_len {
            output.truncate(spec.original_len);
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_stft_istft_reconstruction_identity() {
        let engine = StftEngine::default_48k().unwrap();
        let sample_rate = 48000;
        let duration_secs = 0.5;
        let total_samples = (sample_rate as f32 * duration_secs) as usize;

        // Generate a synthetic test signal: sum of 440 Hz + 1000 Hz sines
        let mut signal = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let val = (2.0 * PI * 440.0 * t).sin() * 0.5 + (2.0 * PI * 1000.0 * t).sin() * 0.3;
            signal.push(val);
        }

        let spec = engine.forward(&signal).unwrap();
        assert!(spec.num_frames() > 10);
        assert_eq!(spec.num_bins(), 513);

        let reconstructed = engine.inverse(&spec).unwrap();
        assert_eq!(reconstructed.len(), signal.len());

        // Interior frames (avoiding boundary window edge effects) should have relative error < 1e-4
        let margin = engine.window_size();
        let mut max_err = 0.0f32;
        for i in margin..(signal.len() - margin) {
            let err = (signal[i] - reconstructed[i]).abs();
            if err > max_err {
                max_err = err;
            }
        }

        println!("Max reconstruction error in interior: {:.6}", max_err);
        assert!(max_err < 1e-3, "STFT -> ISTFT must reconstruct with high fidelity");
    }
}
