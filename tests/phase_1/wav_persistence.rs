use audio_core::{read_wav_canonical_f32, read_wav_f32, write_wav_f32, SampleFormat};
use hound::WavReader;

#[test]
fn test_canonical_wav_spec_and_header() {
    let temp_dir = std::env::temp_dir().join("test_wav_spec");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("test_format.wav");

    let sample_rate = 48000;
    let channels = 1;
    let test_samples = vec![0.0f32, 0.5, -0.5, 0.99, -0.99];

    // Write file using audio_core writer
    write_wav_f32(&file_path, &test_samples, sample_rate, channels).unwrap();

    // Verify WAV headers using low-level hound reader
    let reader = WavReader::open(&file_path).unwrap();
    let spec = reader.spec();

    assert_eq!(spec.channels, 1, "Must be single-channel mono");
    assert_eq!(spec.sample_rate, 48000, "Must be canonical 48 kHz");
    assert_eq!(spec.bits_per_sample, 32, "Must be 32-bit float");
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    assert_eq!(reader.len() as usize, test_samples.len());

    // Read back through audio_core
    let (read_samples, audio_spec) = read_wav_f32(&file_path).unwrap();
    assert_eq!(audio_spec.sample_rate, 48000);
    assert_eq!(audio_spec.channels, 1);
    assert_eq!(audio_spec.sample_format, SampleFormat::F32);
    assert_eq!(read_samples.len(), test_samples.len());

    for (orig, read) in test_samples.iter().zip(read_samples.iter()) {
        assert!((orig - read).abs() < 1e-6);
    }

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_calibration_duration_exactness() {
    let temp_dir = std::env::temp_dir().join("test_calib_dur");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("noise_reference.wav");

    // 2.0 seconds at 48 kHz = 96,000 samples
    let sample_rate = 48000u32;
    let target_samples = (sample_rate as f32 * 2.0) as usize;
    let dummy_noise = vec![0.02f32; target_samples];

    write_wav_f32(&file_path, &dummy_noise, sample_rate, 1).unwrap();

    let (samples, spec) = read_wav_canonical_f32(&file_path).unwrap();
    assert_eq!(samples.len(), 96000);
    let duration = samples.len() as f32 / spec.sample_rate as f32;
    assert!((duration - 2.0).abs() < 1e-4, "Duration must be exactly 2.0s");

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_sample_bounds_sanity() {
    let temp_dir = std::env::temp_dir().join("test_bounds");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("original.wav");

    let samples = vec![-1.0f32, -0.5, 0.0, 0.5, 1.0];
    write_wav_f32(&file_path, &samples, 48000, 1).unwrap();

    let (read, _) = read_wav_canonical_f32(&file_path).unwrap();
    for &s in &read {
        assert!((-1.0..=1.0).contains(&s), "Samples must be bounded in [-1.0, 1.0]");
        assert!(!s.is_nan(), "Sample must not be NaN");
        assert!(!s.is_infinite(), "Sample must not be infinite");
    }

    std::fs::remove_dir_all(&temp_dir).ok();
}
