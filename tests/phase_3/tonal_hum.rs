//! Tests for persistent tonal peak detection (50 Hz and 60 Hz mains hum and harmonics).

use dsp::stft::StftEngine;
use dsp::tonal::{detect_tonal_peaks, find_mains_hum_peaks, TonalDetectionConfig};
use std::f32::consts::PI;

#[test]
fn test_50hz_mains_hum_and_harmonics_detection() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // 50 Hz fundamental + 100 Hz harmonic + 150 Hz harmonic + background hiss
    let mut signal = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let h50 = (2.0 * PI * 50.0 * t).sin() * 0.4;
        let h100 = (2.0 * PI * 100.0 * t).sin() * 0.2;
        let h150 = (2.0 * PI * 150.0 * t).sin() * 0.1;
        let noise = ((i % 31) as f32 / 31.0 - 0.5) * 0.01;
        signal.push(h50 + h100 + h150 + noise);
    }

    let spec = engine.forward(&signal).unwrap();
    let psd = spec.average_psd();
    let config = TonalDetectionConfig::default();

    let peaks = detect_tonal_peaks(&psd, Some(&spec), sample_rate, engine.window_size(), &config);
    assert!(!peaks.is_empty(), "Peaks must be detected");

    let hum_peaks = find_mains_hum_peaks(&peaks, 5.0);
    assert!(!hum_peaks.is_empty(), "Must identify 50 Hz mains hum within 5 Hz");

    let primary = hum_peaks[0];
    println!(
        "Detected 50 Hz hum: {:.2} Hz (strength: {:.2} dB, conf: {:.2})",
        primary.frequency_hz, primary.strength_db, primary.confidence
    );
    assert!((primary.frequency_hz - 50.0).abs() <= 5.0);
}

#[test]
fn test_60hz_mains_hum_detection() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // 60 Hz fundamental + 120 Hz harmonic + background hiss
    let mut signal = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let h60 = (2.0 * PI * 60.0 * t).sin() * 0.35;
        let h120 = (2.0 * PI * 120.0 * t).sin() * 0.15;
        let noise = ((i % 29) as f32 / 29.0 - 0.5) * 0.01;
        signal.push(h60 + h120 + noise);
    }

    let spec = engine.forward(&signal).unwrap();
    let psd = spec.average_psd();
    let config = TonalDetectionConfig::default();

    let peaks = detect_tonal_peaks(&psd, Some(&spec), sample_rate, engine.window_size(), &config);
    let hum_peaks = find_mains_hum_peaks(&peaks, 5.0);

    assert!(!hum_peaks.is_empty(), "Must identify 60 Hz mains hum within 5 Hz");
    let primary = hum_peaks[0];
    println!(
        "Detected 60 Hz hum: {:.2} Hz (strength: {:.2} dB, conf: {:.2})",
        primary.frequency_hz, primary.strength_db, primary.confidence
    );
    assert!((primary.frequency_hz - 60.0).abs() <= 5.0);
}

#[test]
fn test_pure_noise_has_no_mains_hum_peaks() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // Broadband random noise without periodic tonal lines
    let mut signal = Vec::with_capacity(total_samples);
    let mut seed = 55555u32;
    for _ in 0..total_samples {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let val = ((seed as f32 / u32::MAX as f32) - 0.5) * 0.05;
        signal.push(val);
    }

    let spec = engine.forward(&signal).unwrap();
    let psd = spec.average_psd();
    let config = TonalDetectionConfig::default();

    let peaks = detect_tonal_peaks(&psd, Some(&spec), sample_rate, engine.window_size(), &config);
    let hum_peaks = find_mains_hum_peaks(&peaks, 5.0);

    assert!(hum_peaks.is_empty(), "Flat broadband noise should not detect mains hum peaks");
}
