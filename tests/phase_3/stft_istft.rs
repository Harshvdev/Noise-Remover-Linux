//! STFT and ISTFT synthesis inversion tests.

use dsp::stft::StftEngine;
use std::f32::consts::PI;

#[test]
fn test_stft_istft_reconstruction_fidelity() {
    let engine = StftEngine::default_48k().unwrap();
    let sample_rate = 48000;
    let duration = 0.5;
    let total_samples = (sample_rate as f32 * duration) as usize;

    // Multi-tone complex signal
    let mut signal = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let s = (2.0 * PI * 220.0 * t).sin() * 0.3
            + (2.0 * PI * 880.0 * t).sin() * 0.2
            + (2.0 * PI * 2400.0 * t).sin() * 0.1;
        signal.push(s);
    }

    let spec = engine.forward(&signal).unwrap();
    assert_eq!(spec.num_bins(), 513);
    assert!(spec.num_frames() > 10);

    let reconstructed = engine.inverse(&spec).unwrap();
    assert_eq!(reconstructed.len(), signal.len());

    // Evaluate reconstruction error away from boundary edge tapers
    let margin = engine.window_size();
    let mut max_err = 0.0f32;
    for i in margin..(signal.len() - margin) {
        let err = (signal[i] - reconstructed[i]).abs();
        if err > max_err {
            max_err = err;
        }
    }

    println!("STFT-ISTFT interior max error: {:.6}", max_err);
    assert!(max_err < 1e-3, "Reconstruction relative error must be below 1e-3");
}

#[test]
fn test_stft_rejects_buffer_shorter_than_window() {
    let engine = StftEngine::default_48k().unwrap();
    let short_signal = vec![0.0f32; 512]; // window_size is 1024
    let res = engine.forward(&short_signal);
    assert!(res.is_err());
}
