//! Error types for neural audio denoisers.

use std::path::PathBuf;
use thiserror::Error;

/// Errors arising during neural denoiser initialization or inference.
#[derive(Debug, Error)]
pub enum DenoiserError {
    #[error("Neural model file not found: {0}")]
    ModelNotFound(PathBuf),

    #[error("Failed to initialize neural denoiser model: {0}")]
    ModelInitFailed(String),

    #[error("Neural inference failed: {0}")]
    InferenceFailed(String),

    #[error("Unsupported sample rate: {0} Hz (expected 48000 Hz)")]
    UnsupportedSampleRate(u32),

    #[error("Audio I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("DSP processing error: {0}")]
    Dsp(#[from] dsp::DspError),

    #[error("Processing was cancelled by user")]
    Cancelled,
}
