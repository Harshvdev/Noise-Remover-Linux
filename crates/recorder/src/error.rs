//! Recorder error definitions.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RecorderError {
    #[error("CPAL error: {0}")]
    Cpal(#[from] cpal::Error),
    #[error("Device not found at index: {0}")]
    DeviceNotFound(usize),
    #[error("No supported input configuration found for device")]
    NoSupportedConfig,
    #[error("WAV I/O error: {0}")]
    WavIo(#[from] hound::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
