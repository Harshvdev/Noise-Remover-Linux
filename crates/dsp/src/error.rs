//! Error types for DSP operations and noise analysis.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DspError {
    #[error("Invalid STFT configuration: window_size={0}, hop_size={1}")]
    InvalidStftConfig(usize, usize),

    #[error("Audio buffer too short for analysis: {0} samples (minimum {1} required)")]
    BufferTooShort(usize, usize),

    #[error("FFT error: {0}")]
    FftError(String),

    #[error("Audio core error: {0}")]
    AudioCore(#[from] audio_core::WavIoError),
}
