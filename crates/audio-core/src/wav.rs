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
            if hound_spec.bits_per_sample <= 8 {
                SampleFormat::U8
            } else if hound_spec.bits_per_sample <= 16 {
                SampleFormat::I16
            } else if hound_spec.bits_per_sample <= 24 {
                SampleFormat::I24
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
            if hound_spec.bits_per_sample <= 8 {
                reader
                    .samples::<i8>()
                    .map(|s| crate::pcm::i16_to_f32((s.unwrap_or(0) as i16) << 8))
                    .collect()
            } else if hound_spec.bits_per_sample <= 16 {
                reader
                    .samples::<i16>()
                    .map(|s| crate::pcm::i16_to_f32(s.unwrap_or(0)))
                    .collect()
            } else if hound_spec.bits_per_sample <= 24 {
                reader
                    .samples::<i32>()
                    .map(|s| crate::pcm::i24_to_f32(s.unwrap_or(0)))
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

/// Read audio file to 32-bit floating point mono samples (downmixing if necessary).
pub fn read_wav_canonical_f32<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, AudioSpec), WavIoError> {
    let (samples, spec) = read_wav_f32(path)?;
    if spec.channels == 1 {
        Ok((samples, spec))
    } else {
        let mono = crate::pcm::interleaved_to_mono(&samples, spec.channels as usize);
        let mono_spec = AudioSpec {
            channels: 1,
            ..spec
        };
        Ok((mono, mono_spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_wav_f32() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_audio_core.wav");
        let original_samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];

        write_wav_f32(&test_file, &original_samples, 48000, 1).unwrap();
        let (read_samples, spec) = read_wav_f32(&test_file).unwrap();

        assert_eq!(spec.sample_rate, 48000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_format, SampleFormat::F32);
        assert_eq!(read_samples.len(), original_samples.len());

        for (a, b) in original_samples.iter().zip(read_samples.iter()) {
            assert!((a - b).abs() < 1e-6);
        }

        std::fs::remove_file(&test_file).ok();
    }
}
