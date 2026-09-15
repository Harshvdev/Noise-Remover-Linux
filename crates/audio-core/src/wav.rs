//! Lossless WAV file reader and writer using Hound.

use crate::format::{AudioSpec, SampleFormat};
use hound::{WavReader, WavSpec, WavWriter};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WavIoError {
    #[error("Hound error: {0}")]
    Hound(#[from] hound::Error),
    #[error("Unsupported bit depth: {0}")]
    UnsupportedBitDepth(u16),
}

#[derive(Error, Debug)]
pub enum AudioLoadError {
    #[error("WAV I/O error: {0}")]
    WavIo(#[from] WavIoError),
    #[error("Resampling error: {0}")]
    Resample(#[from] crate::resample::ResampleError),
    #[error("Failed to decode audio: {0}")]
    DecodeFailed(String),
    #[error("Audio too short: expected at least {expected_secs:.1}s ({expected_samples} samples), got {actual_samples} samples ({actual_secs:.1}s)")]
    AudioTooShort {
        expected_secs: f32,
        expected_samples: usize,
        actual_secs: f32,
        actual_samples: usize,
    },
    #[error("File not found: {0}")]
    FileNotFound(String),
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
pub fn read_wav_canonical_f32<P: AsRef<Path>>(
    path: P,
) -> Result<(Vec<f32>, AudioSpec), WavIoError> {
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

/// Load any audio file (WAV, MP3, FLAC, OGG, M4A, etc.) and convert to canonical 48 kHz mono 32-bit float samples.
/// Native Hound is tried first for WAV files. If that fails or if the format is non-WAV, ffmpeg is used.
pub fn load_audio_canonical_48k<P: AsRef<Path>>(path: P) -> Result<Vec<f32>, AudioLoadError> {
    let path_ref = path.as_ref();
    if !path_ref.exists() {
        return Err(AudioLoadError::FileNotFound(path_ref.display().to_string()));
    }

    // 1. If it's a WAV file, first attempt native Hound reading
    let is_wav = path_ref
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase() == "wav")
        .unwrap_or(false);

    if is_wav {
        if let Ok((samples, spec)) = read_wav_canonical_f32(path_ref) {
            let samples_48k = if spec.sample_rate != 48000 {
                crate::resample::resample_mono(&samples, spec.sample_rate, 48000)?
            } else {
                samples
            };
            return Ok(samples_48k);
        }
    }

    // 2. If Hound fails or the file is non-WAV (mp3, flac, ogg, m4a, etc.),
    // decode via ffmpeg outputting raw 32-bit float little-endian mono PCM at 48 kHz.
    let output = std::process::Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path_ref)
        .arg("-vn")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("1")
        .arg("-f")
        .arg("f32le")
        .arg("pipe:1")
        .output();

    match output {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            let bytes = out.stdout;
            let sample_count = bytes.len() / 4;
            let mut samples = Vec::with_capacity(sample_count);
            for chunk in bytes.as_chunks::<4>().0 {
                let s = f32::from_le_bytes(*chunk);
                samples.push(s);
            }
            Ok(samples)
        }
        Ok(out) => {
            let err_msg = String::from_utf8_lossy(&out.stderr);
            Err(AudioLoadError::DecodeFailed(format!(
                "Failed to decode '{}': {}",
                path_ref.display(),
                err_msg.trim()
            )))
        }
        Err(e) => Err(AudioLoadError::DecodeFailed(format!(
            "Could not decode audio file '{}' (native WAV reader failed and ffmpeg unavailable: {})",
            path_ref.display(),
            e
        ))),
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

    #[test]
    fn test_load_audio_canonical_48k_wav() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_load_44k.wav");
        // Create 1 second of 44.1 kHz audio
        let original_samples: Vec<f32> = (0..44100).map(|i| (i as f32 * 0.01).sin()).collect();
        write_wav_f32(&test_file, &original_samples, 44100, 1).unwrap();

        let loaded =
            load_audio_canonical_48k(&test_file).expect("Failed to load and resample 44.1k wav");
        assert_eq!(loaded.len(), 48000);

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_load_audio_canonical_48k_ffmpeg() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_ffmpeg_input.ogg");
        let res = std::process::Command::new("ffmpeg")
            .arg("-y")
            .arg("-v")
            .arg("error")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("sine=frequency=440:duration=2.5")
            .arg("-c:a")
            .arg("libvorbis")
            .arg(&test_file)
            .status();

        if let Ok(status) = res {
            if status.success() {
                let loaded =
                    load_audio_canonical_48k(&test_file).expect("Failed to load ogg via ffmpeg");
                assert_eq!(loaded.len(), 120000);
            }
        }
        let _ = std::fs::remove_file(&test_file);
    }
}
