//! Audio playback subsystem.

pub mod error;
pub mod player;

pub use error::PlaybackError;
pub use player::AudioPlayer;
