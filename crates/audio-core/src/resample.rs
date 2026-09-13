//! Audio resampling utilities using rubato.

use rubato::{
    FastFixedIn, PolynomialDegree, Resampler,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResampleError {
    #[error("Rubato process error: {0}")]
    Process(#[from] rubato::ResampleError),
    #[error("Rubato construction error: {0}")]
    Construction(#[from] rubato::ResamplerConstructionError),
    #[error("Invalid sample rate: from {0} to {1}")]
    InvalidSampleRate(u32, u32),
}

/// Resample a single-channel slice of audio from one sample rate to another.
/// If `from_rate == to_rate`, the original samples are cloned directly.
pub fn resample_mono(
    samples: &[f32],
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, ResampleError> {
    if from_rate == 0 || to_rate == 0 {
        return Err(ResampleError::InvalidSampleRate(from_rate, to_rate));
    }
    if from_rate == to_rate || samples.is_empty() {
        return Ok(samples.to_vec());
    }

    let chunk_size = 1024;
    let mut resampler = FastFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        1.0,
        PolynomialDegree::Septic,
        chunk_size,
        1,
    )?;

    let mut output = Vec::with_capacity((samples.len() as f64 * to_rate as f64 / from_rate as f64) as usize + 2048);
    let mut pos = 0;

    while pos < samples.len() {
        let end = (pos + chunk_size).min(samples.len());
        let mut in_chunk = samples[pos..end].to_vec();
        // If final chunk is smaller than chunk_size, pad with zeros
        if in_chunk.len() < chunk_size {
            in_chunk.resize(chunk_size, 0.0);
        }

        let waves_in = vec![in_chunk];
        let waves_out = resampler.process(&waves_in, None)?;
        if let Some(channel_out) = waves_out.into_iter().next() {
            output.extend(channel_out);
        }

        pos += chunk_size;
    }

    // Trim output to expected length based on duration
    let expected_len = (samples.len() as f64 * to_rate as f64 / from_rate as f64).round() as usize;
    if output.len() > expected_len {
        output.truncate(expected_len);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_identity() {
        let input = vec![0.1, 0.2, 0.3];
        let out = resample_mono(&input, 48000, 48000).unwrap();
        assert_eq!(input, out);
    }

    #[test]
    fn test_resample_44100_to_48000() {
        let input = vec![0.0f32; 44100]; // 1 second
        let out = resample_mono(&input, 44100, 48000).unwrap();
        assert_eq!(out.len(), 48000);
    }
}
