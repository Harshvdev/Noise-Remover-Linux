//! PCM sample conversions and channel downmixing.

#[inline]
pub fn i16_to_f32(sample: i16) -> f32 {
    if sample < 0 {
        sample as f32 / 32768.0
    } else {
        sample as f32 / 32767.0
    }
}

#[inline]
pub fn i32_to_f32(sample: i32) -> f32 {
    if sample < 0 {
        sample as f32 / 2147483648.0
    } else {
        sample as f32 / 2147483647.0
    }
}

#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped < 0.0 {
        (clamped * 32768.0) as i16
    } else {
        (clamped * 32767.0) as i16
    }
}

/// Convert multi-channel interleaved float audio samples to single-channel mono.
pub fn interleaved_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    if channels == 1 {
        return interleaved.to_vec();
    }

    let frames = interleaved.len() / channels;
    let mut mono = Vec::with_capacity(frames);
    let inv_ch = 1.0 / channels as f32;

    for frame_idx in 0..frames {
        let start = frame_idx * channels;
        let mut sum = 0.0;
        for ch in 0..channels {
            sum += interleaved[start + ch];
        }
        mono.push(sum * inv_ch);
    }

    mono
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i16_to_f32_limits() {
        assert_eq!(i16_to_f32(0), 0.0);
        assert_eq!(i16_to_f32(32767), 1.0);
        assert_eq!(i16_to_f32(-32768), -1.0);
    }

    #[test]
    fn test_interleaved_to_mono() {
        // Stereo: L=1.0, R=0.0 -> mono 0.5
        let stereo = vec![1.0, 0.0, -0.5, -0.5];
        let mono = interleaved_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - (-0.5)).abs() < 1e-6);
    }
}
