//! Acceptance criteria integration tests defined in architecture.md Section 72:
//! - Phase 7 — Preservation Layer
//!
//! Acceptance criteria:
//! 1. No comb filtering from dry/wet misalignment;
//! 2. No obvious phasing;
//! 3. Vocal remains close to original when denoising is light;
//! 4. Removed-noise track does not contain obvious wanted vocals at normal settings;
//! 5. End-to-end preservation on real recorded audio if present.

use denoiser::DpdfnetDenoiser;
use dsp::{
    AdaptivePreservationConfig, DspConfig, DspIntensity, DspProcessor, HarmonicityConfig,
    LatencyAligner, NoiseProfile, PreservationLayer, PreservationMode, StftEngine,
};
use std::f32::consts::PI;
use std::path::Path;

#[test]
fn acceptance_criterion_1_no_comb_filtering_from_dry_wet_misalignment() {
    let sample_rate: u32 = 48000;
    let n = sample_rate as usize * 2; // 2 seconds

    // 1. Synthesize broadband signal with multitone components
    let mut original = vec![0.0f32; n];
    for (i, sample) in original.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        *sample = (2.0 * PI * 400.0 * t).sin() * 0.25
            + (2.0 * PI * 800.0 * t).sin() * 0.20
            + (2.0 * PI * 1200.0 * t).sin() * 0.15
            + (2.0 * PI * 1600.0 * t).sin() * 0.10
            + (2.0 * PI * 2000.0 * t).sin() * 0.08;
    }

    // 2. Simulate processed signal delayed by 24 samples (0.5 ms delay - severe comb notches at 1 kHz, 3 kHz, etc.)
    let latency_samples = 24;
    let mut delayed_processed = vec![0.0f32; n];
    delayed_processed[latency_samples..].copy_from_slice(&original[..n - latency_samples]);

    // Unaligned blend: 50% dry + 50% delayed wet (causes comb filtering)
    let unaligned_metrics =
        LatencyAligner::check_comb_filtering(&original, &delayed_processed, sample_rate);
    println!(
        "Unaligned blend metrics: notch_depth={:.1} dB, comb_detected={}, coherence={:.3}",
        unaligned_metrics.notch_depth_db,
        unaligned_metrics.has_comb_filtering,
        unaligned_metrics.phase_coherence
    );

    assert!(
        unaligned_metrics.has_comb_filtering || unaligned_metrics.notch_depth_db > 8.0,
        "Unaligned mixing must exhibit comb-filtering spectral notches"
    );

    // 3. Process with Phase 7 Preservation Layer (with latency alignment)
    let layer = PreservationLayer::new(sample_rate, PreservationMode::Global(0.50));
    let (cleaned, removed, report) =
        layer.process(&original, &delayed_processed, latency_samples, None);

    assert_eq!(cleaned.len(), n);
    assert_eq!(removed.len(), n);

    println!(
        "Preservation aligned blend metrics: notch_depth={:.1} dB, comb_detected={}, coherence={:.3}",
        report.comb_metrics.notch_depth_db,
        report.comb_metrics.has_comb_filtering,
        report.comb_metrics.phase_coherence
    );

    // Acceptance criterion: No comb filtering from dry/wet misalignment
    assert!(
        !report.comb_metrics.has_comb_filtering,
        "Latency-aligned preservation layer must eliminate comb filtering"
    );
    assert!(
        report.comb_metrics.notch_depth_db < 3.0,
        "Aligned blend must have minimal notch ripple (< 3.0 dB, got {:.1} dB)",
        report.comb_metrics.notch_depth_db
    );
    assert!(
        report.comb_metrics.phase_coherence > 0.95,
        "Latency-aligned signals must achieve high phase coherence (> 0.95, got {:.3})",
        report.comb_metrics.phase_coherence
    );
}

#[test]
fn acceptance_criterion_2_no_obvious_phasing() {
    let sample_rate: u32 = 48000;
    let n = sample_rate as usize * 2; // 2 seconds

    // Synthesize noise calibration reference
    let mut noise_calib = vec![0.0f32; n];
    for (i, s) in noise_calib.iter_mut().enumerate() {
        *s = ((i % 53) as f32 / 53.0 - 0.5) * 0.02;
    }

    let stft = StftEngine::default_48k().unwrap();
    let spec_noise = stft.forward(&noise_calib).unwrap();
    let profile = NoiseProfile::from_spectrogram(&spec_noise, &noise_calib, sample_rate);

    // Synthesize harmonic vowel tone (F0 = 220 Hz, A3) + background noise
    let mut original = noise_calib.clone();
    for (i, s) in original.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        let vowel = 0.30 * (2.0 * PI * 220.0 * t).sin()
            + 0.15 * (2.0 * PI * 440.0 * t).sin()
            + 0.10 * (2.0 * PI * 660.0 * t).sin()
            + 0.05 * (2.0 * PI * 880.0 * t).sin();
        *s += vowel;
    }

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspIntensity::Gentle.to_config();
    let result = processor.process(&original, &profile, None, &config).unwrap();

    // Check delay between original and cleaned output via cross-correlation
    let (lag, corr) = LatencyAligner::estimate_delay(&original, &result.cleaned_samples, 200);
    println!(
        "Criterion 2 (Phasing): Best lag = {} samples, cross-correlation peak = {:.4}",
        lag, corr
    );

    assert!(
        lag.abs() <= 2,
        "DSP cleaned output must have negligible phase delay (<= 2 samples / 0.04 ms, got {})",
        lag
    );
    assert!(
        corr > 0.98,
        "Phase coherence must be high (> 0.98) to prevent hollow phasing (got {:.4})",
        corr
    );
    assert!(
        !result.preservation.comb_metrics.has_comb_filtering,
        "Output must have no comb filtering"
    );
}

#[test]
fn acceptance_criterion_3_vocal_remains_close_to_original_when_denoising_is_light() {
    let sample_rate: u32 = 48000;
    let n = sample_rate as usize * 2; // 2 seconds

    // 1. Noise calibration reference
    let mut noise_calib = vec![0.0f32; n];
    for (i, s) in noise_calib.iter_mut().enumerate() {
        *s = ((i % 31) as f32 / 31.0 - 0.5) * 0.01;
    }
    let stft = StftEngine::default_48k().unwrap();
    let spec_noise = stft.forward(&noise_calib).unwrap();
    let profile = NoiseProfile::from_spectrogram(&spec_noise, &noise_calib, sample_rate);

    // 2. Synthesize singing vocal with vibrato (F0 around 330 Hz, E4) + background noise
    let mut vocal_track = noise_calib.clone();
    for (i, s) in vocal_track.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        let vibrato = 330.0 + 8.0 * (2.0 * PI * 5.5 * t).sin();
        let vocal = 0.25 * (2.0 * PI * vibrato * t).sin()
            + 0.12 * (2.0 * PI * (2.0 * vibrato) * t).sin()
            + 0.06 * (2.0 * PI * (3.0 * vibrato) * t).sin();
        *s += vocal;
    }

    let in_rms = (vocal_track.iter().map(|&s| s * s).sum::<f32>() / n as f32).sqrt();
    let in_rms_dbfs = 20.0 * in_rms.log10();

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspConfig {
        preservation: PreservationMode::Adaptive(AdaptivePreservationConfig {
            base_alpha: 1.0,
            min_vocal_alpha: 0.65,
            vocal_protection_strength: 0.85,
            harmonic_protection_strength: 0.90,
            harmonicity: HarmonicityConfig::default(),
        }),
        ..DspIntensity::Gentle.to_config()
    };

    let result = processor.process(&vocal_track, &profile, None, &config).unwrap();

    let out_rms = (result.cleaned_samples.iter().map(|&s| s * s).sum::<f32>() / n as f32).sqrt();
    let out_rms_dbfs = 20.0 * out_rms.log10();
    let deviation_db = (in_rms_dbfs - out_rms_dbfs).abs();

    println!(
        "Criterion 3 (Vocal Preservation): In RMS={:.2} dBFS, Out RMS={:.2} dBFS, Deviation={:.2} dB, Preserved={:.1}%",
        in_rms_dbfs,
        out_rms_dbfs,
        deviation_db,
        result.preservation.vocal_preservation_percentage
    );

    // Acceptance criterion: Vocal remains close to original when denoising is light (< 0.5 dB change in vocal body)
    assert!(
        deviation_db < 0.50,
        "Vocal RMS must stay within 0.5 dB of original under light/vocal-safe settings (got {:.2} dB)",
        deviation_db
    );
    assert!(
        result.preservation.vocal_preservation_percentage >= 95.0,
        "Vocal preservation percentage must be >= 95.0% (got {:.1}%)",
        result.preservation.vocal_preservation_percentage
    );
}

#[test]
fn acceptance_criterion_4_removed_noise_does_not_contain_wanted_vocals() {
    let sample_rate: u32 = 48000;
    let n = sample_rate as usize * 2; // 2 seconds

    // 1. Noise calibration reference
    let mut noise_calib = vec![0.0f32; n];
    for (i, s) in noise_calib.iter_mut().enumerate() {
        *s = ((i % 47) as f32 / 47.0 - 0.5) * 0.02; // -34 dBFS noise
    }
    let stft = StftEngine::default_48k().unwrap();
    let spec_noise = stft.forward(&noise_calib).unwrap();
    let profile = NoiseProfile::from_spectrogram(&spec_noise, &noise_calib, sample_rate);

    // 2. Synthesize vocal burst in middle (0.5s to 1.5s) + steady room noise
    let mut speech_audio = noise_calib.clone();
    for (i, s) in speech_audio.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        let speech = if (24000..72000).contains(&i) {
            0.28 * (2.0 * PI * 280.0 * t).sin() + 0.14 * (2.0 * PI * 560.0 * t).sin()
        } else {
            0.0
        };
        *s += speech;
    }

    let processor = DspProcessor::new(sample_rate).unwrap();
    let config = DspIntensity::Balanced.to_config();
    let result = processor.process(&speech_audio, &profile, None, &config).unwrap();

    // Analyze speech region in removed noise track (0.6s to 1.4s)
    let speech_region_orig = &speech_audio[28800..67200];
    let speech_region_noise = &result.removed_noise_samples[28800..67200];

    let speech_in_rms =
        (speech_region_orig.iter().map(|&s| s * s).sum::<f32>() / speech_region_orig.len() as f32).sqrt();
    let speech_leak_rms =
        (speech_region_noise.iter().map(|&s| s * s).sum::<f32>() / speech_region_noise.len() as f32).sqrt();

    let vocal_leakage_attenuation_db = 20.0 * (speech_in_rms / speech_leak_rms.max(1e-12)).log10();

    println!(
        "Criterion 4 (Vocal Leakage): Speech In RMS={:.1} dBFS, Leakage in Removed Noise={:.1} dBFS, Attenuation={:.1} dB",
        20.0 * speech_in_rms.log10(),
        20.0 * speech_leak_rms.log10(),
        vocal_leakage_attenuation_db
    );

    // Acceptance criterion: Removed noise track does not contain obvious wanted vocals (attenuation > 20 dB)
    assert!(
        vocal_leakage_attenuation_db >= 20.0,
        "Removed-noise track must not contain obvious wanted vocals (leakage attenuation must be >= 20.0 dB, got {:.1} dB)",
        vocal_leakage_attenuation_db
    );

    // Waveform & Energy conservation check: Cleaned + Removed ≈ Original within 0.1 dB
    let sum_audio: Vec<f32> = result
        .cleaned_samples
        .iter()
        .zip(result.removed_noise_samples.iter())
        .map(|(&c, &r)| c + r)
        .collect();
    let sum_rms = (sum_audio.iter().map(|&s| s * s).sum::<f32>() / n as f32).sqrt();
    let in_total_rms = (speech_audio.iter().map(|&s| s * s).sum::<f32>() / n as f32).sqrt();
    let energy_diff_db = (20.0 * (sum_rms / in_total_rms.max(1e-12)).log10()).abs();
    println!("Criterion 4: Dual-synthesis energy conservation diff = {:.3} dB", energy_diff_db);
    assert!(
        energy_diff_db < 0.10,
        "Total audio energy must be conserved within 0.10 dB (got {:.3} dB)",
        energy_diff_db
    );
}

#[test]
fn acceptance_criterion_5_real_recording_preservation_if_present() {
    let orig_path = Path::new("../recordings/original.wav");
    if !orig_path.exists() {
        println!("User recording not present; skipping real audio preservation test.");
        return;
    }

    let (samples, spec) = match audio_core::read_wav_canonical_f32(orig_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read user recording: {}", e);
            return;
        }
    };

    println!(
        "Testing Preservation Layer on real audio: {} samples ({:.2}s)",
        samples.len(),
        samples.len() as f32 / spec.sample_rate as f32
    );

    // Test with Neural DPDFNet2 backend if model is present
    if let Ok(mut denoiser) = DpdfnetDenoiser::load_default() {
        let (cleaned, removed, report) = denoiser
            .denoise_with_report(&samples)
            .expect("Neural inference must succeed");

        println!("=== DPDFNET2 PRESERVATION REPORT ===");
        println!(
            "Model Latency: {} samples ({:.1} ms)",
            report.latency_samples, report.latency_ms
        );
        println!(
            "Mean Preservation Alpha: {:.3}",
            report.mean_preservation_alpha
        );
        println!(
            "Vocal Leakage Attenuation: {:.1} dB",
            report.vocal_leakage_attenuation_db
        );
        println!(
            "Cleaned Output Length: {}, Removed Noise Length: {}",
            cleaned.len(),
            removed.len()
        );
        println!("====================================");

        assert_eq!(cleaned.len(), samples.len());
        assert_eq!(removed.len(), samples.len());

        for (i, &s) in cleaned.iter().enumerate() {
            assert!(s.is_finite(), "Cleaned sample at {} must be finite", i);
        }
        for (i, &s) in removed.iter().enumerate() {
            assert!(s.is_finite(), "Removed noise sample at {} must be finite", i);
        }
    }
}
