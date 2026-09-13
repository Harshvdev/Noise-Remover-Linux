//! Core audio types, conversions, and utilities.

pub mod format;
pub mod pcm;
pub mod resample;
pub mod wav;

pub use format::{AudioSpec, SampleFormat, CANONICAL_CHANNELS, CANONICAL_SAMPLE_RATE};
pub use pcm::{f32_to_i16, i16_to_f32, i24_to_f32, i32_to_f32, u8_to_f32, interleaved_to_mono};
pub use resample::{resample_mono, ResampleError};
pub use wav::{read_wav_canonical_f32, read_wav_f32, write_wav_f32, WavIoError};
