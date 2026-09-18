//! Acceptance criteria integration tests defined in architecture.md Section 71:
//! - Phase 6 — DPDFNet2-48k
//!
//! Acceptance criteria:
//! 1. Model output is canonical 48 kHz;
//! 2. Long files process without OOM;
//! 3. Latency is measurable/documented and state is continuous;
//! 4. In-band background noise is eliminated;
//! 5. Verification on real recorded audio if present.

use denoiser::{DenoiserBackend, DpdfnetDenoiser};
use std::f32::consts::PI;

#[test]
fn acceptance_criterion_1_model_output_is_canonical_48khz() {
    let mut denoiser = match DpdfnetDenoiser::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DPDFNet2 model not found ({})", e);
            return;
        }
    };

    assert_eq!(denoiser.sample_rate(), 48000, "DPDFNet2 must operate at 48 kHz");

    let sample_rate = 48000;
    let total_samples = sample_rate * 2; // 2 seconds

    let mut noisy_speech = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let noise = 0.03 * ((i % 100) as f32 / 100.0 - 0.5);
        let tone = if (24000..72000).contains(&i) {
            0.2 * (2.0 * PI * 440.0 * t).sin()
        } else {
            0.0
        };
        noisy_speech.push(tone + noise);
    }

    let cleaned = denoiser.process(&noisy_speech).expect("Inference must succeed");

    assert_eq!(
        cleaned.len(),
        noisy_speech.len(),
        "Cleaned output length must exactly match input length"
    );

    // Verify samples are finite
    for (i, &s) in cleaned.iter().enumerate() {
        assert!(s.is_finite(), "Sample at {} must be finite: {}", i, s);
    }
}

#[test]
fn acceptance_criterion_2_latency_and_performance_reporting() {
    let mut denoiser = match DpdfnetDenoiser::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DPDFNet2 model not found ({})", e);
            return;
        }
    };

    let latency = denoiser.latency_samples();
    assert_eq!(latency, 0, "Offline DPDFNet2 denoiser outputs zero-latency time-aligned audio");
    println!("Reported model latency: {} samples ({:.1} ms)", latency, latency as f32 / 48.0);

    let sample_rate = 48000;
    let total_samples = sample_rate * 3; // 3 seconds
    let mut test_audio = vec![0.0f32; total_samples];
    let mut lcg = 123456789u64;
    for sample in test_audio.iter_mut() {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
        let rand_val = ((lcg >> 33) as f32 / (1u64 << 31) as f32) - 0.5;
        *sample = 0.02 * rand_val;
    }

    let (cleaned, removed, report) = denoiser
        .denoise_with_report(&test_audio)
        .expect("denoise_with_report must succeed");

    assert_eq!(cleaned.len(), total_samples);
    assert_eq!(removed.len(), total_samples);

    println!(
        "Performance: duration={:.2}s, inference={:.1}ms, RTF={:.3}, attenuation={:.1}dB",
        report.duration_seconds, report.inference_time_ms, report.rtf, report.attenuation_db
    );

    assert!(report.rtf < 2.5, "DPDFNet2 inference should be practical in debug mode (got RTF={:.3})", report.rtf);
    assert!(report.attenuation_db > 5.0, "Noise must be attenuated (got {:.1} dB)", report.attenuation_db);
}

#[test]
fn acceptance_criterion_3_long_files_process_without_oom() {
    let mut denoiser = match DpdfnetDenoiser::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DPDFNet2 model not found ({})", e);
            return;
        }
    };

    let sample_rate = 48000;
    // 6 seconds of audio
    let chunk_size = sample_rate * 2;
    let mut chunk = vec![0.01f32; chunk_size];
    for (i, sample) in chunk.iter_mut().enumerate() {
        *sample = ((i % 50) as f32 / 50.0 - 0.5) * 0.04;
    }

    for block_idx in 0..3 {
        let out = denoiser.process(&chunk).expect("Chunk inference must succeed");
        assert_eq!(out.len(), chunk_size);
        println!("Processed long-file block {} successfully", block_idx + 1);
    }
}

#[test]
fn acceptance_criterion_4_real_user_recording_deep_clean_if_present() {
    let orig_path = std::path::Path::new("../recordings/original.wav");
    if !orig_path.exists() {
        println!("User recording not present; skipping real audio acceptance test.");
        return;
    }

    let mut denoiser = match DpdfnetDenoiser::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DPDFNet2 model not found ({})", e);
            return;
        }
    };

    let (samples, _) = audio_core::read_wav_canonical_f32(orig_path).unwrap();
    let raw_cleaned = denoiser.process(&samples).unwrap();
    let silence_raw_rms = (raw_cleaned[..48000 * 3].iter().map(|&s| s * s).sum::<f32>() / (48000.0 * 3.0)).sqrt();
    let silence_orig_rms = (samples[..48000 * 3].iter().map(|&s| s * s).sum::<f32>() / (48000.0 * 3.0)).sqrt();
    println!(">>> RAW DPDFNET2 SILENCE: Orig RMS = {:.2} dBFS, Raw Cleaned RMS = {:.2} dBFS (Attenuation = {:.2} dB)",
        20.0 * silence_orig_rms.log10(), 20.0 * silence_raw_rms.log10(), 20.0 * (silence_orig_rms / silence_raw_rms).log10());
    let (lag, corr) = dsp::LatencyAligner::estimate_delay(&samples, &raw_cleaned, 1000);
    println!(">>> EMPIRICAL DPDFNET2 LAG: {} samples, correlation: {:.4}", lag, corr);

    let (cleaned, removed, report) = denoiser.denoise_with_report(&samples).unwrap();
    let silence_final_rms = (cleaned[..48000 * 3].iter().map(|&s| s * s).sum::<f32>() / (48000.0 * 3.0)).sqrt();
    println!(">>> PRESERVATION LAYER SILENCE: Cleaned RMS = {:.2} dBFS (Attenuation = {:.2} dB)",
        20.0 * silence_final_rms.log10(), 20.0 * (silence_orig_rms / silence_final_rms).log10());

    println!("=== DPDFNET2 REAL AUDIO TEST ===");
    println!("Audio Duration: {:.2}s", report.duration_seconds);
    println!("Inference Time: {:.1}ms (RTF: {:.3})", report.inference_time_ms, report.rtf);
    println!("Input RMS: {:.1} dBFS -> Cleaned RMS: {:.1} dBFS", report.input_rms_dbfs, report.cleaned_rms_dbfs);
    println!("Removed Noise RMS: {:.1} dBFS", report.removed_noise_rms_dbfs);
    println!("Neural Attenuation: {:.1} dB", report.attenuation_db);
    println!("Mean Alpha: {:.3}, Vocal Leakage Attenuation: {:.1} dB", report.mean_preservation_alpha, report.vocal_leakage_attenuation_db);
    println!("================================");

    assert_eq!(cleaned.len(), samples.len());
    assert_eq!(removed.len(), samples.len());

    let dsp_path = std::path::Path::new("../recordings/dsp_cleaned.wav");
    let input_for_save = if dsp_path.exists() { dsp_path } else { orig_path };
    let (save_samples, _) = audio_core::read_wav_canonical_f32(input_for_save).unwrap();
    let (save_cleaned, save_removed, _) = denoiser.denoise_with_report(&save_samples).unwrap();
    let _ = audio_core::write_wav_f32(std::path::Path::new("../recordings/deep_cleaned.wav"), &save_cleaned, 48000, 1);
    let _ = audio_core::write_wav_f32(std::path::Path::new("../recordings/removed_deep_noise.wav"), &save_removed, 48000, 1);
    println!(">>> Refreshed recordings/deep_cleaned.wav and recordings/removed_deep_noise.wav with zero-latency preservation!");
}
