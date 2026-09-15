//! Acceptance criteria tests defined in architecture.md Section 68:
//! - Phase 3 — Noise Analyzer
//!
//! Acceptance criteria:
//! 1. fan recording -> high stationarity (S >= 0.75)
//! 2. hum recording -> tonal peaks at 50 Hz / 60 Hz (+/- 5 Hz)
//! 3. changing traffic -> low stationarity (S <= 0.40)
//! 4. clean recording -> low estimated noise (<= -55 dBFS)

use audio_core::{read_wav_canonical_f32, write_wav_f32};
use dsp::analyzer::NoiseAnalyzer;
use dsp::tonal::find_mains_hum_peaks;
use std::f32::consts::PI;

#[test]
fn acceptance_criterion_1_fan_recording_high_stationarity() {
    let sample_rate = 48000;
    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();

    // Steady fan simulation: constant broadband noise with low-frequency emphasis
    let total_samples = 48000 * 2; // 2 seconds
    let mut fan_signal = Vec::with_capacity(total_samples);
    let mut rng = 42424242u32;
    for i in 0..total_samples {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let white = (rng as f32 / u32::MAX as f32) - 0.5;
        let t = i as f32 / sample_rate as f32;
        let fan_blade_hum = (2.0 * PI * 120.0 * t).sin() * 0.05;
        fan_signal.push((white * 0.08) + fan_blade_hum);
    }

    let profile = analyzer.analyze_noise_reference(&fan_signal).unwrap();
    println!(
        "Criterion 1 (Fan Recording): stationarity={:.3} (acceptance >= 0.75)",
        profile.stationarity_score
    );
    assert!(
        profile.stationarity_score >= 0.75,
        "Fan recording must exhibit high stationarity S >= 0.75 (got {:.3})",
        profile.stationarity_score
    );
    assert!(profile.is_stationary());
}

#[test]
fn acceptance_criterion_2_hum_recording_tonal_peaks() {
    let sample_rate = 48000;
    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();

    // Electrical hum simulation: strong 50 Hz line with 100 Hz harmonic
    let total_samples = 48000 * 2;
    let mut hum_signal = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let hum50 = (2.0 * PI * 50.0 * t).sin() * 0.35;
        let hum100 = (2.0 * PI * 100.0 * t).sin() * 0.15;
        let noise = ((i % 13) as f32 / 13.0 - 0.5) * 0.01;
        hum_signal.push(hum50 + hum100 + noise);
    }

    let profile = analyzer.analyze_noise_reference(&hum_signal).unwrap();
    assert!(!profile.tonal_peaks.is_empty(), "Tonal peaks must be detected");

    let mains_hum = find_mains_hum_peaks(&profile.tonal_peaks, 5.0);
    assert!(!mains_hum.is_empty(), "Must detect mains hum peak within 5 Hz of 50 Hz");

    let peak = mains_hum[0];
    println!(
        "Criterion 2 (Hum Recording): detected mains peak at {:.2} Hz (prominence: {:.2} dB, conf: {:.2})",
        peak.frequency_hz, peak.strength_db, peak.confidence
    );
    assert!(
        (peak.frequency_hz - 50.0).abs() <= 5.0,
        "Peak frequency {:.2} Hz must be within 5 Hz of 50 Hz",
        peak.frequency_hz
    );
}

#[test]
fn acceptance_criterion_3_changing_traffic_low_stationarity() {
    let sample_rate = 48000;
    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();

    // Non-stationary changing traffic simulation: intermittent vehicle acceleration
    let total_samples = 48000 * 2;
    let mut traffic_signal = vec![0.0f32; total_samples];
    for (i, s) in traffic_signal.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        // Two cars passing by at different times with variable revving
        let car1 = (-40.0 * (t - 0.6).powi(2)).exp() * ((2.0 * PI * (60.0 + 80.0 * t) * t).sin() * 0.5);
        let car2 = (-60.0 * (t - 1.5).powi(2)).exp() * ((2.0 * PI * 180.0 * t).sin() * 0.4);
        *s = car1 + car2;
    }

    let profile = analyzer.analyze_noise_reference(&traffic_signal).unwrap();
    println!(
        "Criterion 3 (Changing Traffic): stationarity={:.3} (acceptance <= 0.40)",
        profile.stationarity_score
    );
    assert!(
        profile.stationarity_score <= 0.40,
        "Changing traffic must exhibit low stationarity S <= 0.40 (got {:.3})",
        profile.stationarity_score
    );
    assert!(!profile.is_stationary());
}

#[test]
fn acceptance_criterion_4_clean_recording_low_estimated_noise() {
    let sample_rate = 48000;
    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();

    // Clean recording: studio room tone / low-level quiet floor (-65 dBFS RMS)
    let total_samples = 48000 * 2;
    let mut clean_signal = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let val = ((i % 11) as f32 / 11.0 - 0.5) * 0.0005; // RMS ~ -70 dBFS
        clean_signal.push(val);
    }

    let profile = analyzer.analyze_noise_reference(&clean_signal).unwrap();
    println!(
        "Criterion 4 (Clean Recording): noise floor={:.2} dBFS (acceptance <= -55 dBFS)",
        profile.noise_floor_dbfs
    );
    assert!(
        profile.noise_floor_dbfs <= -55.0,
        "Clean recording must have noise floor <= -55 dBFS (got {:.2} dBFS)",
        profile.noise_floor_dbfs
    );
}

#[test]
fn acceptance_criterion_end_to_end_wav_pipeline() {
    let temp_dir = std::env::temp_dir().join("test_accept_phase3");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let calib_path = temp_dir.join("noise_reference.wav");
    let speech_path = temp_dir.join("original.wav");

    let sample_rate = 48000;
    let total_samples = 48000 * 2; // 2 seconds

    // 1. Write noise_reference.wav (stationary air conditioner noise)
    let mut calib_data = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let val = ((i % 17) as f32 / 17.0 - 0.5) * 0.03;
        calib_data.push(val);
    }
    write_wav_f32(&calib_path, &calib_data, sample_rate, 1).unwrap();

    // 2. Write original.wav (vocal phrases over air conditioner noise)
    let mut speech_data = calib_data.clone();
    for (offset, s) in speech_data[12000..60000].iter_mut().enumerate() {
        let i = 12000 + offset;
        let t = i as f32 / sample_rate as f32;
        let voice = (2.0 * PI * 350.0 * t).sin() * 0.35 + (2.0 * PI * 1050.0 * t).sin() * 0.15;
        *s += voice;
    }
    write_wav_f32(&speech_path, &speech_data, sample_rate, 1).unwrap();

    // 3. Read back via canonical audio-core loaders
    let (read_calib, calib_spec) = read_wav_canonical_f32(&calib_path).unwrap();
    let (read_speech, speech_spec) = read_wav_canonical_f32(&speech_path).unwrap();

    assert_eq!(calib_spec.sample_rate, 48000);
    assert_eq!(speech_spec.sample_rate, 48000);

    // 4. Run unified NoiseAnalyzer
    let analyzer = NoiseAnalyzer::new(48000).unwrap();
    let noise_profile = analyzer.analyze_noise_reference(&read_calib).unwrap();
    assert!(noise_profile.is_stationary());

    let signal_report = analyzer.analyze_signal(&read_speech, &noise_profile).unwrap();
    assert!(signal_report.activity.overall_confidence > 0.35);
    assert!(signal_report.activity.vocal_snr_db > 6.0);
    assert!(signal_report.duration_seconds >= 1.99);

    std::fs::remove_dir_all(&temp_dir).ok();
}
