//! Tests covering Phase 2 Audio Foundation criteria:
//! - 44.1 kHz to 48.0 kHz resampling via Rubato
//! - 48 kHz identity pass-through
//! - Deterministic channel downmixing (interleaved to mono)
//! - Integer PCM conversions (u8, i16, i24, i32 to f32)
//! - Test tone survival across processing

use audio_core::pcm::{f32_to_i16, i16_to_f32, i24_to_f32, i32_to_f32, interleaved_to_mono, u8_to_f32};
use audio_core::resample_mono;
use std::f32::consts::PI;

#[test]
fn test_resampling_44100_to_48000() {
    let input_len = 44100; // 1.0 second
    let input = vec![0.5f32; input_len];

    let output = resample_mono(&input, 44100, 48000).expect("Resampling 44.1k to 48k must succeed");
    assert_eq!(output.len(), 48000, "Output length must scale precisely to 48000 samples");
}

#[test]
fn test_resampling_48000_identity() {
    let input = vec![0.1f32, -0.2, 0.3, -0.4, 0.5];
    let output = resample_mono(&input, 48000, 48000).expect("Identity resampling must succeed");
    assert_eq!(output, input, "48 kHz audio must remain exactly identical without alteration");
}

#[test]
fn test_multichannel_downmixing_determinism() {
    // Single-channel (Mono) pass-through
    let mono_in = vec![0.1f32, -0.5, 0.8];
    assert_eq!(interleaved_to_mono(&mono_in, 1), mono_in);

    // 2-Channel Stereo: Left = 1.0, Right = 0.0 -> Mono = 0.5
    let stereo_in = vec![1.0f32, 0.0, 0.4, 0.2];
    let expected = [0.5f32, 0.3];
    let mono_out = interleaved_to_mono(&stereo_in, 2);
    assert_eq!(mono_out.len(), 2);
    for (m, exp) in mono_out.iter().zip(expected.iter()) {
        assert!((m - exp).abs() < 1e-6);
    }

    // 6-Channel (5.1 surround) deterministic downmix
    let six_ch = vec![1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0];
    let downmixed = interleaved_to_mono(&six_ch, 6);
    assert_eq!(downmixed.len(), 1);
    assert!((downmixed[0] - 1.0).abs() < 1e-6);
}

#[test]
fn test_pcm_format_bounds_and_symmetry() {
    // 8-bit unsigned
    assert_eq!(u8_to_f32(128), 0.0);
    assert_eq!(u8_to_f32(0), -1.0);
    assert!((u8_to_f32(255) - (127.0 / 128.0)).abs() < 1e-5);

    // 16-bit signed
    assert_eq!(i16_to_f32(0), 0.0);
    assert_eq!(i16_to_f32(32767), 32767.0 / 32767.0);
    assert_eq!(i16_to_f32(-32768), -1.0);
    assert_eq!(f32_to_i16(1.0), 32767);
    assert_eq!(f32_to_i16(-1.0), -32768);

    // 24-bit signed
    assert_eq!(i24_to_f32(0), 0.0);
    assert_eq!(i24_to_f32(8388607), 1.0);
    assert_eq!(i24_to_f32(-8388608), -1.0);

    // 32-bit signed
    assert_eq!(i32_to_f32(0), 0.0);
    assert!((i32_to_f32(2147483647) - 1.0).abs() < 1e-6);
    assert_eq!(i32_to_f32(-2147483648), -1.0);
}

#[test]
fn test_sine_test_tone_preservation() {
    // Generate 440 Hz test tone at 44.1 kHz (1.0 second)
    let sample_rate_in = 44100;
    let freq = 440.0f32;
    let mut tone_in = Vec::with_capacity(sample_rate_in);
    for i in 0..sample_rate_in {
        let t = i as f32 / sample_rate_in as f32;
        tone_in.push((2.0 * PI * freq * t).sin() * 0.8);
    }

    // Resample to 48 kHz
    let tone_resampled = resample_mono(&tone_in, 44100, 48000).unwrap();
    assert_eq!(tone_resampled.len(), 48000);

    // Verify peak amplitude survived resampling without distortion/clipping
    let peak = tone_resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!((peak - 0.8).abs() < 0.02, "Peak amplitude should be preserved near 0.8");
}
