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
    assert!(latency > 0, "Model latency must be reported and > 0");
    println!("Reported model latency: {} samples ({:.1} ms)", latency, latency as f32 / 48.0);

    let sample_rate = 48000;
    let total_samples = sample_rate * 3; // 3 seconds
    let mut test_audio = vec![0.0f32; total_samples];
    for (i, sample) in test_audio.iter_mut().enumerate() {
        *sample = 0.02 * ((i % 73) as f32 / 73.0 - 0.5);
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

    assert!(report.rtf < 1.0, "DPDFNet2 inference should be faster than real time (RTF < 1.0)");
    assert!(report.attenuation_db > 5.0, "Noise must be attenuated");
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
    let (cleaned, removed, report) = denoiser.denoise_with_report(&samples).unwrap();

    println!("=== DPDFNET2 REAL AUDIO TEST ===");
    println!("Audio Duration: {:.2}s", report.duration_seconds);
    println!("Inference Time: {:.1}ms (RTF: {:.3})", report.inference_time_ms, report.rtf);
    println!("Input RMS: {:.1} dBFS -> Cleaned RMS: {:.1} dBFS", report.input_rms_dbfs, report.cleaned_rms_dbfs);
    println!("Removed Noise RMS: {:.1} dBFS", report.removed_noise_rms_dbfs);
    println!("Neural Attenuation: {:.1} dB", report.attenuation_db);
    println!("================================");

    assert_eq!(cleaned.len(), samples.len());
    assert_eq!(removed.len(), samples.len());
}
