//! Acceptance criteria integration tests defined in architecture.md Section 73:
//! - Phase 8 — DeepFilterNet3 Benchmark & Integration
//!
//! Acceptance criteria:
//! 1. Integrate DeepFilterNet3 as a second backend;
//! 2. Canonical 48 kHz output, matching input length, and finite sample values;
//! 3. Latency alignment: Delay compensation (-D) gives sample-exact 0-delay alignment;
//! 4. Seamless integration with Phase 7 Preservation Layer (adaptive alpha & dual synthesis);
//! 5. Side-by-side benchmark comparing:
//!    - DSP + DPDFNet2-48k
//!    - DSP + DeepFilterNet3
//!    across RTF, CPU/time, noise attenuation, and vocal preservation.

use denoiser::{
    DeepFilterConfig, DeepFilterDenoiser, DenoiserBackend, DpdfnetDenoiser,
};
use dsp::{
    AdaptivePreservationConfig, DspIntensity, DspProcessor, NoiseProfile, PreservationMode,
    StftEngine,
};
use std::f32::consts::PI;
use std::path::PathBuf;
use std::time::Instant;

fn calculate_rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -120.0;
    }
    let power: f32 = samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32;
    20.0 * power.max(1e-12).sqrt().log10()
}

#[test]
fn acceptance_criterion_1_deepfilter_canonical_48khz_and_finite() {
    let mut denoiser = match DeepFilterDenoiser::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DeepFilterNet3 binary not found ({})", e);
            return;
        }
    };

    assert_eq!(denoiser.sample_rate(), 48000, "DeepFilterNet3 must operate at 48 kHz");

    let sample_rate = 48000;
    let total_samples = sample_rate * 2; // 2 seconds

    let mut noisy_speech = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let noise = 0.02 * ((i % 97) as f32 / 97.0 - 0.5);
        let tone = if (24000..72000).contains(&i) {
            0.25 * (2.0 * PI * 440.0 * t).sin()
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

    for (i, &s) in cleaned.iter().enumerate() {
        assert!(s.is_finite(), "Sample at {} must be finite: {}", i, s);
    }
}

#[test]
fn acceptance_criterion_2_deepfilter_delay_compensation_exact_zero() {
    let config = DeepFilterConfig {
        compensate_delay: true,
        ..Default::default()
    };

    let mut denoiser = match DeepFilterDenoiser::new(config) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DeepFilterNet3 binary not found ({})", e);
            return;
        }
    };

    assert_eq!(
        denoiser.latency_samples(),
        0,
        "DeepFilterNet3 with delay compensation must declare 0 latency samples"
    );

    let sample_rate = 48000;
    let total_samples = sample_rate * 2;
    let mut signal = vec![0.0f32; total_samples];

    // Create distinctive bursts at 0.5s and 1.2s
    let burst1_idx = sample_rate / 2;
    for i in 0..480 {
        let t = i as f32 / sample_rate as f32;
        signal[burst1_idx + i] = 0.4 * (2.0 * PI * 800.0 * t).sin();
    }

    let cleaned = denoiser.process(&signal).expect("Processing must succeed");

    // Find energy peak around burst1 in cleaned signal
    let window_start = burst1_idx.saturating_sub(480);
    let window_end = (burst1_idx + 960).min(cleaned.len());
    let mut max_abs = 0.0f32;
    let mut peak_idx = burst1_idx;
    for (offset, &val) in cleaned[window_start..window_end].iter().enumerate() {
        if val.abs() > max_abs {
            max_abs = val.abs();
            peak_idx = window_start + offset;
        }
    }

    // Peak should be within the burst window without delay skew
    assert!(
        peak_idx >= burst1_idx && peak_idx <= burst1_idx + 480,
        "Peak of burst at {} was detected at {}, within compensated boundary",
        burst1_idx,
        peak_idx
    );
}

#[test]
fn acceptance_criterion_3_deepfilter_preservation_layer_integration() {
    let config = DeepFilterConfig {
        preservation: PreservationMode::Adaptive(AdaptivePreservationConfig {
            min_vocal_alpha: 0.45,
            vocal_protection_strength: 0.85,
            harmonic_protection_strength: 0.90,
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut denoiser = match DeepFilterDenoiser::new(config) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping: DeepFilterNet3 binary not found ({})", e);
            return;
        }
    };

    let sample_rate = 48000;
    let total_samples = sample_rate * 2;
    let mut signal = vec![0.0f32; total_samples];

    // 0 to 1s: pure silence + noise
    for i in 0..sample_rate {
        signal[i] = 0.01 * ((i % 71) as f32 / 71.0 - 0.5);
    }
    // 1 to 2s: vocal harmonic tone
    for i in sample_rate..total_samples {
        let t = i as f32 / sample_rate as f32;
        signal[i] = 0.3 * (2.0 * PI * 330.0 * t).sin() + 0.15 * (2.0 * PI * 660.0 * t).sin();
    }

    let (cleaned, removed_noise, report) = denoiser
        .denoise_with_report(&signal)
        .expect("Denoise with report must succeed");

    assert_eq!(cleaned.len(), signal.len());
    assert_eq!(removed_noise.len(), signal.len());
    assert_eq!(report.sample_rate, 48000);
    assert_eq!(report.latency_samples, 0);
    assert!(report.rtf > 0.0);
    assert!(report.vocal_leakage_attenuation_db >= 0.0);

    // Sum of cleaned + removed should reconstruct the aligned input (at high fidelity)
    for i in 0..signal.len() {
        let reconstructed = cleaned[i] + removed_noise[i];
        assert!(
            (reconstructed - signal[i]).abs() < 1e-3,
            "Reconstruction error at {} exceeded threshold: {} vs {}",
            i,
            reconstructed,
            signal[i]
        );
    }
}

#[test]
fn acceptance_criterion_4_full_side_by_side_benchmark() {
    let original_path = PathBuf::from("../recordings/original.wav");
    let noise_ref_path = PathBuf::from("../recordings/noise_reference.wav");

    // Check if real user recordings are available
    if !original_path.exists() {
        println!("Skipping real recording benchmark: ../recordings/original.wav not found.");
        return;
    }

    let (original_samples, spec) =
        audio_core::read_wav_canonical_f32(&original_path).expect("Failed to read original.wav");
    assert_eq!(spec.sample_rate, 48000);

    let noise_samples = if noise_ref_path.exists() {
        audio_core::read_wav_canonical_f32(&noise_ref_path)
            .map(|(s, _)| s)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    println!("\n================================================================================");
    println!("PHASE 8 BENCHMARK: DPDFNet2-48k vs DeepFilterNet3 Side-by-Side Comparison");
    println!("================================================================================");
    println!(
        "Audio Duration: {:.2} seconds ({} samples)",
        original_samples.len() as f32 / 48000.0,
        original_samples.len()
    );

    // 1. Stage A: Original
    let orig_silence = calculate_rms_dbfs(&original_samples[..48000 * 3 / 2]); // First 1.5s is silence
    let orig_vocal = calculate_rms_dbfs(&original_samples[48000 * 2..48000 * 6]); // 2.0s to 6.0s is vocal

    // 2. Stage B: DSP Only
    let processor = DspProcessor::new(48000).unwrap();
    let dsp_config = DspIntensity::Balanced.to_config();
    let stft = StftEngine::default_48k().unwrap();
    let profile = if !noise_samples.is_empty() {
        let spec = stft.forward(&noise_samples).unwrap();
        NoiseProfile::from_spectrogram(&spec, &noise_samples, 48000)
    } else {
        let spec = stft.forward(&original_samples[..48000 * 3 / 2]).unwrap();
        NoiseProfile::from_spectrogram(&spec, &original_samples[..48000 * 3 / 2], 48000)
    };

    let start_dsp = Instant::now();
    let dsp_res = processor
        .process(&original_samples, &profile, None, &dsp_config)
        .expect("DSP processing failed");
    let dsp_cleaned = dsp_res.cleaned_samples;
    let dsp_time_ms = start_dsp.elapsed().as_secs_f32() * 1000.0;
    let dsp_rtf = (dsp_time_ms / 1000.0) / (original_samples.len() as f32 / 48000.0);
    let dsp_silence = calculate_rms_dbfs(&dsp_cleaned[..48000 * 3 / 2]);
    let dsp_vocal = calculate_rms_dbfs(&dsp_cleaned[48000 * 2..48000 * 6]);

    // 3. Stage C: DPDFNet2 Only
    let mut dpdfnet = match DpdfnetDenoiser::load_default() {
        Ok(d) => Some(d),
        Err(e) => {
            println!("DPDFNet2 not available: {}", e);
            None
        }
    };

    let (dpdf_cleaned, dpdf_report) = if let Some(ref mut denoiser) = dpdfnet {
        let (c, _, rep) = denoiser.denoise_with_report(&original_samples).unwrap();
        (Some(c), Some(rep))
    } else {
        (None, None)
    };

    // 4. Stage D: DSP + DPDFNet2
    let (dsp_dpdf_cleaned, dsp_dpdf_report) = if let Some(ref mut denoiser) = dpdfnet {
        let (c, _, rep) = denoiser.denoise_with_report(&dsp_cleaned).unwrap();
        (Some(c), Some(rep))
    } else {
        (None, None)
    };

    // 5. Stage E: DeepFilterNet3 Only
    let mut deepfilter = match DeepFilterDenoiser::load_default() {
        Ok(d) => Some(d),
        Err(e) => {
            println!("DeepFilterNet3 not available: {}", e);
            None
        }
    };

    let (df_cleaned, df_report) = if let Some(ref mut denoiser) = deepfilter {
        let (c, _, rep) = denoiser.denoise_with_report(&original_samples).unwrap();
        (Some(c), Some(rep))
    } else {
        (None, None)
    };

    // 6. Stage F: DSP + DeepFilterNet3
    let (dsp_df_cleaned, dsp_df_report) = if let Some(ref mut denoiser) = deepfilter {
        let (c, _, rep) = denoiser.denoise_with_report(&dsp_cleaned).unwrap();
        (Some(c), Some(rep))
    } else {
        (None, None)
    };

    println!("\n| Configuration | Total Time (ms) | RTF | Silence RMS | Silence Atten | Vocal RMS | Vocal Delta |");
    println!("|---|---|---|---|---|---|---|");
    println!(
        "| A = Original | - | - | {:.2} dBFS | 0.0 dB | {:.2} dBFS | 0.0 dB |",
        orig_silence, orig_vocal
    );
    println!(
        "| B = DSP Only | {:.1} ms | {:.3} | {:.2} dBFS | {:.2} dB | {:.2} dBFS | {:.2} dB |",
        dsp_time_ms,
        dsp_rtf,
        dsp_silence,
        orig_silence - dsp_silence,
        dsp_vocal,
        (orig_vocal - dsp_vocal).abs()
    );

    if let (Some(c), Some(rep)) = (dpdf_cleaned, dpdf_report.as_ref()) {
        let sil = calculate_rms_dbfs(&c[..48000 * 3 / 2]);
        let voc = calculate_rms_dbfs(&c[48000 * 2..48000 * 6]);
        println!(
            "| C = DPDFNet2-48k | {:.1} ms | {:.3} | {:.2} dBFS | {:.2} dB | {:.2} dBFS | {:.2} dB |",
            rep.inference_time_ms,
            rep.rtf,
            sil,
            orig_silence - sil,
            voc,
            (orig_vocal - voc).abs()
        );
    }

    if let (Some(c), Some(rep)) = (dsp_dpdf_cleaned.as_ref(), dsp_dpdf_report.as_ref()) {
        let sil = calculate_rms_dbfs(&c[..48000 * 3 / 2]);
        let voc = calculate_rms_dbfs(&c[48000 * 2..48000 * 6]);
        println!(
            "| D = DSP + DPDFNet2-48k | {:.1} ms | {:.3} | {:.2} dBFS | {:.2} dB | {:.2} dBFS | {:.2} dB |",
            rep.inference_time_ms + dsp_time_ms,
            rep.rtf,
            sil,
            orig_silence - sil,
            voc,
            (orig_vocal - voc).abs()
        );

        // Assertions for DPDFNet2
        assert!(
            orig_silence - sil >= 25.0,
            "DSP + DPDFNet2 should attenuate silence >= 25 dB"
        );
        assert!(
            (orig_vocal - voc).abs() <= 2.0,
            "DSP + DPDFNet2 vocal delta should be <= 2.0 dB"
        );
    }

    if let (Some(c), Some(rep)) = (df_cleaned, df_report.as_ref()) {
        let sil = calculate_rms_dbfs(&c[..48000 * 3 / 2]);
        let voc = calculate_rms_dbfs(&c[48000 * 2..48000 * 6]);
        println!(
            "| E = DeepFilterNet3 | {:.1} ms | {:.3} | {:.2} dBFS | {:.2} dB | {:.2} dBFS | {:.2} dB |",
            rep.inference_time_ms,
            rep.rtf,
            sil,
            orig_silence - sil,
            voc,
            (orig_vocal - voc).abs()
        );
    }

    if let (Some(c), Some(rep)) = (dsp_df_cleaned.as_ref(), dsp_df_report.as_ref()) {
        let sil = calculate_rms_dbfs(&c[..48000 * 3 / 2]);
        let voc = calculate_rms_dbfs(&c[48000 * 2..48000 * 6]);
        println!(
            "| F = DSP + DeepFilterNet3 | {:.1} ms | {:.3} | {:.2} dBFS | {:.2} dB | {:.2} dBFS | {:.2} dB |",
            rep.inference_time_ms + dsp_time_ms,
            rep.rtf,
            sil,
            orig_silence - sil,
            voc,
            (orig_vocal - voc).abs()
        );

        // Assertions for DeepFilterNet3
        assert!(
            orig_silence - sil >= 30.0,
            "DSP + DeepFilterNet3 should attenuate silence >= 30 dB"
        );
        assert!(
            (orig_vocal - voc).abs() <= 2.5,
            "DSP + DeepFilterNet3 vocal delta should be <= 2.5 dB"
        );
    }

    println!("================================================================================\n");
}
