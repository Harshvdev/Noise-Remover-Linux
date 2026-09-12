//! Audio recording subsystem with lock-free buffering and calibration.

pub mod callback;
pub mod device;
pub mod error;
pub mod meter;
pub mod recorder;

pub use device::{DeviceInfo, DeviceManager};
pub use error::RecorderError;
pub use meter::AudioMeter;
pub use recorder::{AudioRecorder, RecorderMode, RecorderStatus};
