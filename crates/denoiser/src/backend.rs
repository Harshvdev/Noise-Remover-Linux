//! Replaceable neural denoiser backend interface.
//!
//! Defined in Section 36 of architecture.md to ensure application logic does not
//! depend directly on ONNX tensor shapes or model-specific C APIs.

use crate::error::DenoiserError;

/// Performance and diagnostic report for a completed neural denoising pass.
#[derive(Debug, Clone)]
pub struct DenoiseReport {
    /// Backend model name.
    pub model_name: &'static str,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Model latency in samples.
    pub latency_samples: usize,
    /// Model latency in milliseconds.
    pub latency_ms: f32,
    /// Input duration in seconds.
    pub duration_seconds: f32,
    /// Elapsed inference duration in milliseconds.
    pub inference_time_ms: f32,
    /// Real-Time Factor (inference_time / audio_duration).
    /// RTF < 1.0 means faster than real-time.
    pub rtf: f32,
    /// Input RMS in dBFS.
    pub input_rms_dbfs: f32,
    /// Cleaned RMS in dBFS.
    pub cleaned_rms_dbfs: f32,
    /// Removed noise RMS in dBFS.
    pub removed_noise_rms_dbfs: f32,
    /// Total attenuation achieved in dB.
    pub attenuation_db: f32,
    /// Mean preservation blending alpha applied across all samples (Phase 7).
    pub mean_preservation_alpha: f32,
    /// Vocal leakage attenuation in dB for the removed noise track (Phase 7).
    pub vocal_leakage_attenuation_db: f32,
}

/// Generic interface for neural speech and singing enhancement backends.
pub trait DenoiserBackend: Send + Sync {
    /// Human-readable backend and model identifier.
    fn name(&self) -> &'static str;

    /// Supported sample rate (e.g. 48000 Hz).
    fn sample_rate(&self) -> u32;

    /// Algorithmic / structural latency of the model in samples.
    fn latency_samples(&self) -> usize;

    /// Reset internal state (for new takes or discontinuous streams).
    fn reset(&mut self) -> Result<(), DenoiserError>;

    /// Process a slice of 48 kHz canonical mono samples and return enhanced audio.
    fn process(&mut self, input: &[f32]) -> Result<Vec<f32>, DenoiserError>;
}
