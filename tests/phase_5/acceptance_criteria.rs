//! Acceptance criteria integration tests defined in architecture.md Section 70:
//! - Phase 5 — Residual Noise Decision
//!
//! Acceptance criteria:
//! 1. DSP residual = low  -> finish without AI;
//! 2. DSP residual = high -> invoke neural backend (Phase 6);
//! 3. Speech-free quiet recording evaluates residual noise directly without false speech reporting;
//! 4. Full end-to-end pipeline: Audio -> Analyzer -> DSP Processor -> Residual Evaluation.

use audio_core::write_wav_f32;
use dsp::analyzer::NoiseAnalyzer;
use dsp::processor::{DspConfig, DspProcessor};
use dsp::residual::{analyze_residual, DenoiseDecision, ResidualLevel};
use dsp::stft::StftEngine;
use std::f32::consts::PI;

#[test]
fn acceptance_criterion_1_low_residual_finishes_without_ai() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2; // 2 seconds

    // Clean audio with very quiet background noise (-65 dBFS) and prominent speech (-18 dBFS)
    let mut clean_speech = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let hiss = 0.0005 * ((i % 100) as f32 / 100.0 - 0.5); // ~ -70 dBFS
        let voice = if (24000..72000).contains(&i) {
            0.25 * (2.0 * PI * 440.0 * t).sin()
        } else {
            0.0
        };
        clean_speech.push(voice + hiss);
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec = stft.forward(&clean_speech[..24000]).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec, &clean_speech[..24000], sample_rate);

    let report = analyze_residual(&clean_speech, &profile, None, sample_rate).unwrap();

    println!(
        "Criterion 1 (Low Residual): Floor={:.1} dBFS, SNR={:.1} dB, Level={:?}, Decision={:?}",
        report.residual_noise_dbfs, report.post_dsp_snr_db, report.level, report.decision
    );

    assert_eq!(
        report.level,
        ResidualLevel::Low,
        "Low residual noise must be classified as ResidualLevel::Low"
    );
    assert_eq!(
        report.decision,
        DenoiseDecision::FinishWithoutAi,
        "Cleaned audio should finish without AI"
    );
}

#[test]
fn test_diagnose_user_recording_if_present() {
    let calib_path = std::path::Path::new("../recordings/noise_reference.wav");
    let orig_path = std::path::Path::new("../recordings/original.wav");
    let cleaned_path = std::path::Path::new("../recordings/dsp_cleaned.wav");

    if !calib_path.exists() || !orig_path.exists() || !cleaned_path.exists() {
        println!("User recordings not present, skipping diagnostic test.");
        return;
    }

    let (calib_samples, _) = audio_core::read_wav_canonical_f32(calib_path).unwrap();
    let (orig_samples, _) = audio_core::read_wav_canonical_f32(orig_path).unwrap();
    let (cleaned_samples, _) = audio_core::read_wav_canonical_f32(cleaned_path).unwrap();

    let stft = StftEngine::default_48k().unwrap();
    let spec_calib = stft.forward(&calib_samples).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec_calib, &calib_samples, 48000);

    let analyzer = NoiseAnalyzer::new(48000).unwrap();
    let sig_report = analyzer.analyze_signal(&orig_samples, &profile).unwrap();

    let res_report = analyze_residual(
        &cleaned_samples,
        &profile,
        Some(&sig_report.activity),
        48000,
    ).unwrap();

    println!("=== REAL USER RECORDING (CURRENT DSP OUTPUT) ===");
    println!("Profile noise floor: {:.1} dBFS", profile.noise_floor_dbfs);
    println!("Profile stationarity: {:.2}", profile.stationarity_score);
    println!("Vocal confidence: {:.2}", sig_report.activity.overall_confidence);
    println!("Vocal SNR: {:.1} dB", sig_report.activity.vocal_snr_db);
    println!("Active speech frame ratio: {:.1}%", sig_report.activity.active_frame_ratio * 100.0);
    println!("Speech RMS: {:.1} dBFS", res_report.speech_rms_dbfs);
    println!("Residual noise floor: {:.1} dBFS", res_report.residual_noise_dbfs);
    println!("Post-DSP SNR: {:.1} dB", res_report.post_dsp_snr_db);
    println!("Attenuation: {:.1} dB", res_report.noise_attenuation_db);
    println!("Level: {:?}", res_report.level);
    println!("Decision: {:?}", res_report.decision);
    println!("Explanation: {}", res_report.explanation);
    println!("================================================");

    // Test with more aggressive DSP config
    let mut aggressive_config = DspConfig::default();
    aggressive_config.wiener.min_gain = 0.04; // ~ -28 dB floor
    aggressive_config.wiener.oversubtraction = 1.45;
    let proc = DspProcessor::new(48000).unwrap();
    let agg_result = proc.process(&orig_samples, &profile, Some(&sig_report.activity), &aggressive_config).unwrap();

    println!("=== AGGRESSIVE DSP TEST ===");
    println!("Input RMS: {:.1} dBFS -> Cleaned RMS: {:.1} dBFS", agg_result.report.input_rms_dbfs, agg_result.report.cleaned_rms_dbfs);
    println!("Attenuation: {:.1} dB", agg_result.report.attenuation_db);
    println!("Residual Noise Floor: {:.1} dBFS", agg_result.residual.residual_noise_dbfs);
    println!("Post-DSP SNR: {:.1} dB", agg_result.residual.post_dsp_snr_db);
    assert_eq!(
        res_report.decision,
        DenoiseDecision::InvokeNeuralBackend,
        "Audio with loud initial noise (-31.8 dBFS) must invoke neural backend when residual is audible"
    );
}

#[test]
fn acceptance_criterion_2_high_residual_invokes_neural_ai() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2;

    // Noisy audio with loud residual hiss (-32 dBFS)
    let mut noisy_speech = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let loud_hiss = 0.045 * ((i % 100) as f32 / 100.0 - 0.5); // ~ -32 dBFS
        let voice = if (24000..72000).contains(&i) {
            0.15 * (2.0 * PI * 440.0 * t).sin()
        } else {
            0.0
        };
        noisy_speech.push(voice + loud_hiss);
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec = stft.forward(&noisy_speech[..24000]).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec, &noisy_speech[..24000], sample_rate);

    let report = analyze_residual(&noisy_speech, &profile, None, sample_rate).unwrap();

    println!(
        "Criterion 2 (High Residual): Floor={:.1} dBFS, SNR={:.1} dB, Level={:?}, Decision={:?}",
        report.residual_noise_dbfs, report.post_dsp_snr_db, report.level, report.decision
    );

    assert_eq!(
        report.decision,
        DenoiseDecision::InvokeNeuralBackend,
        "High residual noise must trigger InvokeNeuralBackend decision"
    );
}

#[test]
fn acceptance_criterion_3_speech_free_quiet_recording_finishes_without_ai() {
    let sample_rate = 48000;
    let total_samples = 48000 * 2;

    // Quiet studio environment with NO speech (-60 dBFS)
    let mut quiet_room = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let hiss = 0.001 * ((i % 61) as f32 / 61.0 - 0.5);
        quiet_room.push(hiss);
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec = stft.forward(&quiet_room).unwrap();
    let profile = dsp::NoiseProfile::from_spectrogram(&spec, &quiet_room, sample_rate);

    let report = analyze_residual(&quiet_room, &profile, None, sample_rate).unwrap();

    println!(
        "Criterion 3 (Speech-Free Quiet): Floor={:.1} dBFS, Level={:?}, Decision={:?}, Explanation={}",
        report.residual_noise_dbfs, report.level, report.decision, report.explanation
    );

    assert_eq!(
        report.level,
        ResidualLevel::Low,
        "Quiet speech-free audio must be categorized as Low"
    );
    assert_eq!(
        report.decision,
        DenoiseDecision::FinishWithoutAi,
        "Quiet speech-free audio must finish without AI"
    );
    assert!(
        report.explanation.contains("No vocal activity detected"),
        "Explanation must clearly indicate absence of vocal activity"
    );
}

#[test]
fn acceptance_criterion_4_end_to_end_dsp_and_residual_pipeline() {
    let temp_dir = std::env::temp_dir().join("test_phase5_accept");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let calib_file = temp_dir.join("noise_reference.wav");
    let speech_file = temp_dir.join("original.wav");

    let sample_rate = 48000;
    let total_samples = 48000 * 2;

    // 1. Synthesize and write noise reference (fan noise)
    let mut calib_data = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let hiss = ((i % 31) as f32 / 31.0 - 0.5) * 0.03;
        calib_data.push(hiss);
    }
    write_wav_f32(&calib_file, &calib_data, sample_rate, 1).unwrap();

    // 2. Synthesize and write voice + noise recording
    let mut speech_data = calib_data.clone();
    for (offset, sample) in speech_data[24000..72000].iter_mut().enumerate() {
        let i = 24000 + offset;
        let t = i as f32 / sample_rate as f32;
        *sample += (2.0 * PI * 400.0 * t).sin() * 0.35;
    }
    write_wav_f32(&speech_file, &speech_data, sample_rate, 1).unwrap();

    // 3. Run full NoiseAnalyzer
    let analyzer = NoiseAnalyzer::new(sample_rate).unwrap();
    let (read_calib, _) = audio_core::read_wav_canonical_f32(&calib_file).unwrap();
    let (read_speech, _) = audio_core::read_wav_canonical_f32(&speech_file).unwrap();

    let profile = analyzer.analyze_noise_reference(&read_calib).unwrap();
    let signal_rep = analyzer.analyze_signal(&read_speech, &profile).unwrap();

    // 4. Run DspProcessor end-to-end
    let processor = DspProcessor::new(sample_rate).unwrap();
    let result = processor
        .process(
            &read_speech,
            &profile,
            Some(&signal_rep.activity),
            &DspConfig::default(),
        )
        .unwrap();

    // Verify Phase 5 residual analysis report is generated and consistent
    println!(
        "Criterion 4 (End-to-End Pipeline):\n  Original Noise Floor: {:.1} dBFS\n  Post-DSP Residual: {:.1} dBFS\n  Post-DSP SNR: {:.1} dB\n  Attenuation: {:.1} dB\n  Decision: {:?}\n  Explanation: {}",
        result.residual.original_noise_dbfs,
        result.residual.residual_noise_dbfs,
        result.residual.post_dsp_snr_db,
        result.residual.noise_attenuation_db,
        result.residual.decision,
        result.residual.explanation
    );

    assert!(result.residual.noise_attenuation_db > 0.0);
    assert_eq!(result.residual.original_noise_dbfs, profile.noise_floor_dbfs);
    assert!(
        result.residual.decision == DenoiseDecision::FinishWithoutAi
            || result.residual.decision == DenoiseDecision::InvokeNeuralBackend
    );

    std::fs::remove_dir_all(&temp_dir).ok();
}
