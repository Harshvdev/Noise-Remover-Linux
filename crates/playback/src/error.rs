//! Playback error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("CPAL error: {0}")]
    Cpal(#[from] cpal::Error),
    #[error("No output device available")]
    NoOutputDevice,
    #[error("Audio file error: {0}")]
    Wav(#[from] audio_core::WavIoError),
    #[error("No supported stream configuration")]
    NoSupportedConfig,
}


