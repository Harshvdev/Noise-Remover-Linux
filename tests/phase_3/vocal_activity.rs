//! Tests for vocal activity detection and SNR estimation.

use dsp::activity::{detect_activity, ActivityConfig};
use dsp::noise_profile::NoiseProfile;
use dsp::stft::StftEngine;
use std::f32::consts::PI;

#[test]
fn test_vocal_activity_discrimination() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // 1. Calibration noise reference (steady quiet hiss)
    let mut calib_noise = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let val = ((i % 17) as f32 / 17.0 - 0.5) * 0.01;
        calib_noise.push(val);
    }
    let spec_calib = engine.forward(&calib_noise).unwrap();
    let profile = NoiseProfile::from_spectrogram(&spec_calib, &calib_noise, sample_rate);

    // 2. Audio with speech bursts in vocal range (300 Hz, 800 Hz, 2200 Hz)
    let mut speech_audio = calib_noise.clone();
    for (offset, sample) in speech_audio[10000..38000].iter_mut().enumerate() {
        let i = 10000 + offset;
        let t = i as f32 / sample_rate as f32;
        let s = (2.0 * PI * 300.0 * t).sin() * 0.3
            + (2.0 * PI * 800.0 * t).sin() * 0.2
            + (2.0 * PI * 2200.0 * t).sin() * 0.1;
        *sample += s;
    }

    let spec_speech = engine.forward(&speech_audio).unwrap();
    let config = ActivityConfig::default();
    let speech_report = detect_activity(&spec_speech, &profile, &config);

    println!(
        "Speech signal report: confidence={:.2}, SNR={:.2} dB, active_ratio={:.2}",
        speech_report.overall_confidence, speech_report.vocal_snr_db, speech_report.active_frame_ratio
    );
    assert!(speech_report.overall_confidence >= 0.40);
    assert!(speech_report.vocal_snr_db > 6.0);
    assert!(speech_report.active_frame_ratio > 0.40);

    // 3. Audio with background noise only
    let noise_report = detect_activity(&spec_calib, &profile, &config);
    println!(
        "Background noise only report: confidence={:.2}, SNR={:.2} dB, active_ratio={:.2}",
        noise_report.overall_confidence, noise_report.vocal_snr_db, noise_report.active_frame_ratio
    );
    assert!(noise_report.overall_confidence < 0.10);
    assert!(noise_report.active_frame_ratio < 0.10);
}
