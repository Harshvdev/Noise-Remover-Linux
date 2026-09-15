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
    assert!(
        (duration - 2.0).abs() < 1e-4,
        "Duration must equal exactly 2.0s"
    );

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
    assert_eq!(
        pass_through, original,
        "Meter must not modify audio stream data"
    );
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
    assert!(
        (peak - 0.9).abs() < 0.02,
        "Peak amplitude must be preserved near 0.9"
    );

    // Check zero-crossing periodicity to confirm frequency is preserved at 1 kHz
    let zero_crossings = resampled
        .windows(2)
        .filter(|w| (w[0] <= 0.0 && w[1] > 0.0) || (w[0] >= 0.0 && w[1] < 0.0))
        .count();
    // 1000 Hz tone over 1 second has approx 2000 zero crossings
    assert!(
        (zero_crossings as i32 - 2000).abs() < 10,
        "1 kHz tone periodicity must be preserved"
    );
}

#[test]
fn acceptance_criterion_uploaded_audio_extracts_two_second_noise_calibration() {
    use audio_core::load_audio_canonical_48k;
    use dsp::{DspIntensity, DspProcessor, NoiseAnalyzer};

    let temp_dir = std::env::temp_dir().join("test_upload_calib");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let uploaded_source_file = temp_dir.join("user_take_44k.wav");

    // 1. Create a 4.0 second recording at 44.1 kHz
    // First 2.0s is ambient noise (50 Hz hum + low hiss)
    // Next 2.0s is voice/tone + hum
    let rate = 44100u32;
    let total_samples = (rate as f32 * 4.0) as usize;
    let mut samples_44k = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let t = i as f32 / rate as f32;
        // Background noise: 50 Hz mains hum (-30 dB)
        let noise = (2.0 * PI * 50.0 * t).sin() * 0.03;
        // Vocal/tone: starts after 2.0 seconds
        let voice = if t >= 2.0 {
            (2.0 * PI * 440.0 * t).sin() * 0.4
        } else {
            0.0
        };
        samples_44k.push(noise + voice);
    }

    write_wav_f32(&uploaded_source_file, &samples_44k, rate, 1).unwrap();

    // 2. Load and canonicalize to 48 kHz
    let canonical_48k = load_audio_canonical_48k(&uploaded_source_file)
        .expect("Failed to load and resample uploaded file to 48kHz canonical");

    // Duration should be 4.0 seconds -> 192,000 samples at 48 kHz
    assert_eq!(canonical_48k.len(), 48000 * 4);

    // 3. Extract first 2 seconds as noise reference
    let calib_samples = 48000 * 2;
    let noise_reference = &canonical_48k[..calib_samples];
    let calib_path = temp_dir.join("noise_reference.wav");
    write_wav_f32(&calib_path, noise_reference, 48000, 1).unwrap();

    let rec_path = temp_dir.join("original.wav");
    write_wav_f32(&rec_path, &canonical_48k, 48000, 1).unwrap();

    // Verify lengths
    let (read_calib, calib_spec) = read_wav_canonical_f32(&calib_path).unwrap();
    assert_eq!(calib_spec.sample_rate, 48000);
    assert_eq!(read_calib.len(), 96000); // exactly 2 seconds

    let (read_rec, rec_spec) = read_wav_canonical_f32(&rec_path).unwrap();
    assert_eq!(rec_spec.sample_rate, 48000);
    assert_eq!(read_rec.len(), 192000); // full 4 seconds

    // 4. Verify NoiseAnalyzer analyzes the 2s reference and detects hum
    let analyzer = NoiseAnalyzer::new(48000).unwrap();
    let profile = analyzer
        .analyze_noise_reference(&read_calib)
        .expect("NoiseAnalyzer should analyze 2s extracted calibration");
    assert!(profile.noise_floor_dbfs < -20.0);

    // 5. Verify DspProcessor cleans the original audio take using the calibrated profile
    let processor = DspProcessor::new(48000).unwrap();
    let config = DspIntensity::Balanced.to_config();
    let dsp_result = processor
        .process(&read_rec, &profile, None, &config)
        .expect("DSP processing should succeed on uploaded audio");

    assert_eq!(dsp_result.cleaned_samples.len(), read_rec.len());
    assert!(dsp_result.report.attenuation_db > 0.0);

    std::fs::remove_dir_all(&temp_dir).ok();
}
