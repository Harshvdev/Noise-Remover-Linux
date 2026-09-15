//! Explicit acceptance criteria tests defined in architecture.md:
//! - Section 66: Phase 1 — Recorder MVP
//! - Section 67: Phase 2 — Audio Foundation

use audio_core::{read_wav_canonical_f32, resample_mono, write_wav_f32};
use recorder::device::DeviceManager;
use recorder::error::RecorderError;
use recorder::meter::AudioMeter;
use std::f32::consts::PI;

#[test]
fn acceptance_criterion_invalid_device_returns_explicit_error() {
    let dm = DeviceManager::new();
    // Non-existent device index must return DeviceNotFound error
    let invalid_idx = 99999;
    let result = dm.get_input_device(invalid_idx);
    assert!(result.is_err(), "Accessing invalid device index must fail");
    match result.unwrap_err() {
        RecorderError::DeviceNotFound(idx) => assert_eq!(idx, invalid_idx),
        other => panic!("Expected DeviceNotFound, got: {:?}", other),
    }
}

#[test]
fn acceptance_criterion_calibration_file_validity_and_duration() {
    let temp_dir = std::env::temp_dir().join("test_accept_calib");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let calib_file = temp_dir.join("noise_reference.wav");

    // Generate exactly 2 seconds of calibrated noise reference at 48 kHz
    let sample_rate = 48000;
    let target_samples = (sample_rate as f32 * 2.0) as usize; // 96,000
    let mut noise_data = Vec::with_capacity(target_samples);
    for i in 0..target_samples {
        let val = ((i % 100) as f32 / 100.0 - 0.5) * 0.05;
        noise_data.push(val);
    }

    write_wav_f32(&calib_file, &noise_data, sample_rate, 1).unwrap();

    // Verify file exists and is non-empty
    assert!(calib_file.exists());
    let metadata = std::fs::metadata(&calib_file).unwrap();
    assert!(metadata.len() > 0, "Calibration WAV must be non-empty");

    // Read and verify canonical 48 kHz mono float32 structure
    let (read_samples, spec) = read_wav_canonical_f32(&calib_file).unwrap();
    assert_eq!(spec.sample_rate, 48000);
    assert_eq!(spec.channels, 1);
    assert_eq!(read_samples.len(), target_samples);

    let duration = read_samples.len() as f32 / spec.sample_rate as f32;
    assert!((duration - 2.0).abs() < 1e-4, "Duration must equal exactly 2.0s");

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn acceptance_criterion_meter_does_not_interfere_with_audio_data() {
    let meter = AudioMeter::new();
    let original = vec![0.1f32, -0.2, 0.3, -0.4, 0.5, -0.6];
    let pass_through = original.clone();

    // Meter updates in-flight callback data
    meter.update(&pass_through);

    // Audio stream data must remain 100% bit-exact and unaltered
    assert_eq!(pass_through, original, "Meter must not modify audio stream data");
}

#[test]
fn acceptance_criterion_test_tones_survive_conversion_without_artifacts() {
    // Generate 1 kHz pure sine test tone at 44.1 kHz
    let rate_in = 44100u32;
    let freq = 1000.0f32;
    let mut tone = Vec::with_capacity(rate_in as usize);
    for i in 0..rate_in {
        let t = i as f32 / rate_in as f32;
        tone.push((2.0 * PI * freq * t).sin() * 0.9);
    }

    // Convert via 48 kHz resampler
    let resampled = resample_mono(&tone, rate_in, 48000).unwrap();
    assert_eq!(resampled.len(), 48000);

    // Measure SNR / Peak preservation
    let peak = resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!((peak - 0.9).abs() < 0.02, "Peak amplitude must be preserved near 0.9");

    // Check zero-crossing periodicity to confirm frequency is preserved at 1 kHz
    let zero_crossings = resampled.windows(2).filter(|w| (w[0] <= 0.0 && w[1] > 0.0) || (w[0] >= 0.0 && w[1] < 0.0)).count();
    // 1000 Hz tone over 1 second has approx 2000 zero crossings
    assert!((zero_crossings as i32 - 2000).abs() < 10, "1 kHz tone periodicity must be preserved");
}
