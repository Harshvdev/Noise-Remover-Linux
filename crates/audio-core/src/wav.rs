//! Lossless WAV file reader and writer using Hound.

use std::path::Path;
use hound::{WavReader, WavSpec, WavWriter};
use crate::format::{AudioSpec, SampleFormat};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WavIoError {
    #[error("Hound error: {0}")]
    Hound(#[from] hound::Error),
    #[error("Unsupported bit depth: {0}")]
    UnsupportedBitDepth(u16),
}

/// Write 32-bit floating point PCM audio to a WAV file.
pub fn write_wav_f32<P: AsRef<Path>>(
    path: P,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(), WavIoError> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Read audio file to 32-bit floating point samples.
pub fn read_wav_f32<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, AudioSpec), WavIoError> {
    let mut reader = WavReader::open(path)?;
    let hound_spec = reader.spec();

    let sample_format = match hound_spec.sample_format {
        hound::SampleFormat::Float => SampleFormat::F32,
        hound::SampleFormat::Int => {
            if hound_spec.bits_per_sample <= 16 {
                SampleFormat::I16
            } else {
                SampleFormat::I32
            }
        }
    };

    let spec = AudioSpec {
        sample_rate: hound_spec.sample_rate,
        channels: hound_spec.channels,
        sample_format,
    };

    let samples: Vec<f32> = match hound_spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            if hound_spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .map(|s| crate::pcm::i16_to_f32(s.unwrap_or(0)))
                    .collect()
            } else {
                reader
                    .samples::<i32>()
                    .map(|s| crate::pcm::i32_to_f32(s.unwrap_or(0)))
                    .collect()
            }
        }
    };

    Ok((samples, spec))
}
