//! Tests for stationarity detection (stationary fan/hiss vs bursty/traffic noise).

use dsp::stationarity::analyze_stationarity;
use dsp::stft::StftEngine;

#[test]
fn test_stationarity_steady_fan_and_hiss() {
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    let mut signal = Vec::with_capacity(total_samples);
    let mut state = 987654321u32;
    for _ in 0..total_samples {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let val = ((state as f32 / u32::MAX as f32) - 0.5) * 0.05;
        signal.push(val);
    }

    let spec = engine.forward(&signal).unwrap();
    let report = analyze_stationarity(&spec);

    println!(
        "Steady hiss stationarity score: {:.3} (expected >= 0.75)",
        report.stationarity_score
    );
    assert!(
        report.stationarity_score >= 0.75,
        "Steady fan/hiss must achieve stationarity score >= 0.75"
    );
    assert!(report.is_stationary);
}

#[test]
fn test_stationarity_changing_traffic() {
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // Simulate changing traffic: low rumble that slowly surges and fades
    let mut signal = vec![0.0f32; total_samples];
    for (i, s) in signal.iter_mut().enumerate() {
        let t = i as f32 / 48000.0;
        // Traffic envelope: low baseline, sudden surge in middle (car passing)
        let envelope = (-50.0 * (t - 0.5).powi(2)).exp();
        let rumble = ((i as f32 * 0.02).sin()) * 0.4 * envelope;
        *s = rumble;
    }

    let spec = engine.forward(&signal).unwrap();
    let report = analyze_stationarity(&spec);

    println!(
        "Traffic passing stationarity score: {:.3} (expected <= 0.40)",
        report.stationarity_score
    );
    assert!(
        report.stationarity_score <= 0.40,
        "Changing traffic must have stationarity score <= 0.40"
    );
    assert!(!report.is_stationary);
}

#[test]
fn test_stationarity_transient_bursts() {
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // Simulate sharp transient keyboard clicks/bursts
    let mut signal = vec![0.001f32; total_samples];
    for burst_pos in [4000, 12000, 24000, 36000] {
        for j in 0..400 {
            if burst_pos + j < total_samples {
                signal[burst_pos + j] = ((j as f32 * 0.3).sin()) * 0.7;
            }
        }
    }

    let spec = engine.forward(&signal).unwrap();
    let report = analyze_stationarity(&spec);

    println!(
        "Transient bursts stationarity score: {:.3} (expected <= 0.40)",
        report.stationarity_score
    );
    assert!(
        report.stationarity_score <= 0.40,
        "Transient bursts must have stationarity score <= 0.40"
    );
    assert!(!report.is_stationary);
}

#[test]
fn test_stationarity_digital_silence() {
    let engine = StftEngine::default_48k().unwrap();
    let signal = vec![0.0f32; 48000];

    let spec = engine.forward(&signal).unwrap();
    let report = analyze_stationarity(&spec);

    assert!(report.is_stationary);
    assert_eq!(report.stationarity_score, 1.0);
}
