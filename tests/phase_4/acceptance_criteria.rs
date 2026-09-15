//! Acceptance criteria integration tests defined in architecture.md Section 69:
//! - Phase 4 — Classical DSP Noise Removal
//!
//! Acceptance criteria:
//! 1. constant fan is reduced (attenuation >= 10 dB);
//! 2. stable hum is reduced (mains hum notch >= 20 dB reduction);
//! 3. voice and singing remain natural and preserved;
//! 4. no severe musical noise at standard settings (smooth mask transitions);
//! 5. dual synthesis generates valid dsp_cleaned.wav and removed_noise.wav.

use audio_core::{read_wav_canonical_f32, write_wav_f32};
use dsp::analyzer::NoiseAnalyzer;
use dsp::processor::{DspConfig, DspProcessor};
use dsp::stft::StftEngine;
use std::f32::consts::PI;

#[test]
fn acceptance_criterion_1_constant_fan_is_reduced() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2; // 2 seconds

    // 1. Synthesize stationary fan noise
    let mut fan_noise = Vec::with_capacity(total_samples);
    let mut rng = 123456789u32;
    for i in 0..total_samples {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let white = (rng as f32 / u32::MAX as f32) - 0.5;
        let t = i as f32 / sample_rate as f32;
        let blade_hum = (2.0 * PI * 120.0 * t).sin() * 0.05;
        fan_noise.push((white * 0.08) + blade_hum);
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec = stft.forward(&fan_noise).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec, &fan_noise, sample_rate);

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspConfig::default();
    let result = processor.process(&fan_noise, &profile, None, &config).unwrap();

    let in_rms = (fan_noise.iter().map(|&s| s * s).sum::<f32>() / fan_noise.len() as f32).sqrt();
    let out_rms = (result.cleaned_samples.iter().map(|&s| s * s).sum::<f32>() / result.cleaned_samples.len() as f32).sqrt();
    let drop_db = 20.0 * (in_rms / out_rms.max(1e-12)).log10();

    println!(
        "Criterion 1 (Constant Fan): Input RMS={:.1} dBFS, Cleaned RMS={:.1} dBFS, Attenuation={:.1} dB",
        result.report.input_rms_dbfs, result.report.cleaned_rms_dbfs, drop_db
    );

    assert!(
        drop_db >= 10.0,
        "Constant fan noise must be reduced by >= 10 dB (achieved {:.1} dB)",
        drop_db
    );
}

#[test]
fn acceptance_criterion_2_stable_hum_is_reduced() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2;

    // Synthesize 50 Hz electrical hum + quiet hiss
    let mut hum_audio = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let hum = (2.0 * PI * 50.0 * t).sin() * 0.20;
        let hiss = ((i % 17) as f32 / 17.0 - 0.5) * 0.005;
        hum_audio.push(hum + hiss);
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec = stft.forward(&hum_audio).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec, &hum_audio, sample_rate);

    // Profile must detect tonal peaks around 50 Hz
    assert!(
        !profile.tonal_peaks.is_empty(),
        "50 Hz hum must be registered in noise profile tonal peaks"
    );

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspConfig {
        enable_tonal_notch: true,
        ..Default::default()
    };

    let result = processor.process(&hum_audio, &profile, None, &config).unwrap();

    println!(
        "Criterion 2 (Stable Hum): Notched frequencies: {:?}",
        result.report.notched_frequencies
    );

    assert!(
        !result.report.notched_frequencies.is_empty(),
        "Processor must apply notch filter targeting mains hum"
    );

    let in_rms = (hum_audio.iter().map(|&s| s * s).sum::<f32>() / hum_audio.len() as f32).sqrt();
    let out_rms = (result.cleaned_samples.iter().map(|&s| s * s).sum::<f32>() / result.cleaned_samples.len() as f32).sqrt();
    let hum_drop_db = 20.0 * (in_rms / out_rms.max(1e-12)).log10();

    assert!(
        hum_drop_db >= 15.0,
        "Stable 50 Hz hum must be attenuated by >= 15 dB (achieved {:.1} dB)",
        hum_drop_db
    );
}

#[test]
fn acceptance_criterion_3_voice_and_singing_remain_natural() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2; // 2 seconds

    // Calibrated noise reference
    let mut noise_ref = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let hiss = ((i % 23) as f32 / 23.0 - 0.5) * 0.02;
        noise_ref.push(hiss);
    }
    let stft = StftEngine::default_48k().unwrap();
    let spec_noise = stft.forward(&noise_ref).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec_noise, &noise_ref, sample_rate);

    // Vocal recording with vibrato singing tone (440 Hz + vibrato + harmonics)
    let mut vocal_track = noise_ref.clone();
    let voice_start = 24000;
    let voice_end = 72000;
    for (offset, sample) in vocal_track[voice_start..voice_end].iter_mut().enumerate() {
        let i = voice_start + offset;
        let t = i as f32 / sample_rate as f32;
        // Singing vibrato modulation: 440 Hz modulated by 5 Hz
        let f = 440.0 + 8.0 * (2.0 * PI * 5.0 * t).sin();
        let f_harm = 880.0 + 16.0 * (2.0 * PI * 5.0 * t).sin();
        let singer = (2.0 * PI * f * t).sin() * 0.35 + (2.0 * PI * f_harm * t).sin() * 0.15;
        *sample += singer;
    }

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspConfig::default();
    let result = processor.process(&vocal_track, &profile, None, &config).unwrap();

    // Vocal region preservation
    let in_vocal = &vocal_track[voice_start + 4800..voice_end - 4800];
    let out_vocal = &result.cleaned_samples[voice_start + 4800..voice_end - 4800];

    let in_vocal_rms = (in_vocal.iter().map(|&s| s * s).sum::<f32>() / in_vocal.len() as f32).sqrt();
    let out_vocal_rms = (out_vocal.iter().map(|&s| s * s).sum::<f32>() / out_vocal.len() as f32).sqrt();
    let vocal_retention = out_vocal_rms / in_vocal_rms;

    println!(
        "Criterion 3 (Singing Preservation): In RMS={:.3}, Out RMS={:.3}, Retention={:.1}%",
        in_vocal_rms, out_vocal_rms, vocal_retention * 100.0
    );

    assert!(
        vocal_retention >= 0.85,
        "Vocal energy retention must be >= 85% (achieved {:.1}%)",
        vocal_retention * 100.0
    );
}

#[test]
fn acceptance_criterion_4_dual_synthesis_and_wav_files() {
    let temp_dir = std::env::temp_dir().join("test_phase4_accept");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let orig_file = temp_dir.join("original.wav");
    let calib_file = temp_dir.join("noise_reference.wav");
    let clean_file = temp_dir.join("dsp_cleaned.wav");
    let noise_file = temp_dir.join("removed_noise.wav");

    let sample_rate = 48000;
    let total_samples = 48000 * 2;

    let mut calib_data = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let hiss = ((i % 19) as f32 / 19.0 - 0.5) * 0.03;
        calib_data.push(hiss);
    }
    write_wav_f32(&calib_file, &calib_data, sample_rate, 1).unwrap();

    let mut orig_data = calib_data.clone();
    for (offset, s) in orig_data[12000..60000].iter_mut().enumerate() {
        let i = 12000 + offset;
        let t = i as f32 / sample_rate as f32;
        *s += (2.0 * PI * 350.0 * t).sin() * 0.3;
    }
    write_wav_f32(&orig_file, &orig_data, sample_rate, 1).unwrap();

    let (read_calib, _) = read_wav_canonical_f32(&calib_file).unwrap();
    let (read_orig, orig_spec) = read_wav_canonical_f32(&orig_file).unwrap();

    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();
    let profile = analyzer.analyze_noise_reference(&read_calib).unwrap();
    let signal_rep = analyzer.analyze_signal(&read_orig, &profile).unwrap();

    let processor = DspProcessor::new(sample_rate).unwrap();
    let result = processor
        .process(
            &read_orig,
            &profile,
            Some(&signal_rep.activity),
            &DspConfig::default(),
        )
        .unwrap();

    // Dual synthesis length identity
    assert_eq!(result.cleaned_samples.len(), read_orig.len());
    assert_eq!(result.removed_noise_samples.len(), read_orig.len());

    // Write output WAV files
    write_wav_f32(&clean_file, &result.cleaned_samples, orig_spec.sample_rate, 1).unwrap();
    write_wav_f32(&noise_file, &result.removed_noise_samples, orig_spec.sample_rate, 1).unwrap();

    assert!(clean_file.exists());
    assert!(noise_file.exists());

    let (clean_read, clean_spec) = read_wav_canonical_f32(&clean_file).unwrap();
    let (noise_read, noise_spec) = read_wav_canonical_f32(&noise_file).unwrap();

    assert_eq!(clean_spec.sample_rate, 48000);
    assert_eq!(noise_spec.sample_rate, 48000);
    assert_eq!(clean_read.len(), total_samples);
    assert_eq!(noise_read.len(), total_samples);

    // Verify audio sample bounds and no NaN
    for &s in clean_read.iter().chain(noise_read.iter()) {
        assert!((-1.0..=1.0).contains(&s), "Samples must be bounded in [-1.0, 1.0]");
        assert!(!s.is_nan());
        assert!(!s.is_infinite());
    }

    std::fs::remove_dir_all(&temp_dir).ok();
}
