//! Noise PSD estimation and calibration profile tests.

use dsp::noise_profile::NoiseProfile;
use dsp::stft::StftEngine;

#[test]
fn test_noise_profile_median_resistance() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let total_samples = 48000;

    // Steady background noise
    let mut steady_noise = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let val = ((i % 23) as f32 / 23.0 - 0.5) * 0.04;
        steady_noise.push(val);
    }

    // Corrupted noise with a short burst/cough spike
    let mut corrupted = steady_noise.clone();
    for sample in &mut corrupted[15000..15400] {
        *sample += 0.8;
    }

    let spec_steady = engine.forward(&steady_noise).unwrap();
    let prof_steady = NoiseProfile::from_spectrogram(&spec_steady, &steady_noise, sample_rate);

    let spec_corrupted = engine.forward(&corrupted).unwrap();
    let prof_corrupted = NoiseProfile::from_spectrogram(&spec_corrupted, &corrupted, sample_rate);

    // Median PSD should reject the transient spike
    let mut max_diff = 0.0f32;
    for (a, b) in prof_steady.psd.iter().zip(prof_corrupted.psd.iter()) {
        let diff = (a - b).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }

    println!("Max median PSD diff with spike: {:.6}", max_diff);
    assert!(max_diff < 0.02, "Median PSD must be robust to transient calibration thumps");
}

#[test]
fn test_bin_frequency_conversions() {
    let sample_rate = 48000;
    let engine = StftEngine::default_48k().unwrap();
    let signal = vec![0.01f32; 48000];
    let spec = engine.forward(&signal).unwrap();
    let prof = NoiseProfile::from_spectrogram(&spec, &signal, sample_rate);

    // 1000 Hz bin
    let bin_1k = prof.frequency_to_bin(1000.0);
    let freq_back = prof.bin_to_frequency(bin_1k);
    assert!((freq_back - 1000.0).abs() <= 25.0, "Frequency conversion roundtrip");
}
