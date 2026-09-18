//! DPDFNet2 48 kHz High-Resolution Neural Denoiser Implementation.
//!
//! Wraps `sherpa-onnx` offline speech denoiser configured with `dpdfnet2_48khz_hr.onnx`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use sherpa_onnx::{
    OfflineSpeechDenoiser, OfflineSpeechDenoiserConfig,
    OfflineSpeechDenoiserDpdfNetModelConfig, OfflineSpeechDenoiserModelConfig,
};

use crate::backend::{DenoiseReport, DenoiserBackend};
use crate::error::DenoiserError;
use crate::model::find_dpdfnet2_model;

/// Configuration options for DPDFNet2-48k inference.
#[derive(Debug, Clone)]
pub struct DpdfnetConfig {
    /// Path to `dpdfnet2_48khz_hr.onnx`.
    pub model_path: PathBuf,
    /// Number of CPU worker threads (default: 2 for CPU-first efficiency).
    pub num_threads: i32,
    /// Maximum noise attenuation limit in dB (0.0 = unlimited, default).
    pub attenuation_limit_db: f32,
    /// Preservation layer blending settings (Phase 7).
    pub preservation: dsp::PreservationMode,
}

impl Default for DpdfnetConfig {
    fn default() -> Self {
        let model_path = find_dpdfnet2_model().unwrap_or_else(|| PathBuf::from("models/dpdfnet2_48khz_hr.onnx"));
        Self {
            model_path,
            num_threads: 2,
            attenuation_limit_db: 0.0,
            preservation: dsp::PreservationMode::Adaptive(dsp::AdaptivePreservationConfig::default()),
        }
    }
}

/// DPDFNet2-48k Neural Denoiser Backend.
pub struct DpdfnetDenoiser {
    config: DpdfnetConfig,
    denoiser: OfflineSpeechDenoiser,
}

impl DpdfnetDenoiser {
    /// Initialize the DPDFNet2 denoiser with custom configuration.
    pub fn new(config: DpdfnetConfig) -> Result<Self, DenoiserError> {
        if !config.model_path.exists() {
            return Err(DenoiserError::ModelNotFound(config.model_path.clone()));
        }

        let model_path_str = config.model_path.to_string_lossy().to_string();

        let model_config = OfflineSpeechDenoiserModelConfig {
            dpdfnet: OfflineSpeechDenoiserDpdfNetModelConfig {
                model: Some(model_path_str),
                attenuation_limit_db: config.attenuation_limit_db,
            },
            gtcrn: Default::default(),
            num_threads: config.num_threads,
            debug: false,
            provider: Some("cpu".to_string()),
        };

        let denoiser_config = OfflineSpeechDenoiserConfig {
            model: model_config,
        };

        let denoiser = OfflineSpeechDenoiser::create(&denoiser_config).ok_or_else(|| {
            DenoiserError::ModelInitFailed(format!(
                "Failed to initialize DPDFNet2 model from {:?}",
                config.model_path
            ))
        })?;

        log::info!(
            "[INFO] DPDFNet2-48k initialized successfully from {:?}",
            config.model_path
        );

        Ok(Self { config, denoiser })
    }

    /// Load the DPDFNet2 denoiser using default model search paths.
    pub fn load_default() -> Result<Self, DenoiserError> {
        Self::new(DpdfnetConfig::default())
    }

    /// Load the denoiser from a specific model file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DenoiserError> {
        let config = DpdfnetConfig {
            model_path: path.as_ref().to_path_buf(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Access the active configuration.
    pub fn config(&self) -> &DpdfnetConfig {
        &self.config
    }

    /// Denoise audio samples and generate a comprehensive diagnostic report.
    pub fn denoise_with_report(
        &mut self,
        input: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>, DenoiseReport), DenoiserError> {
        self.denoise_with_activity(input, None)
    }

    /// Denoise audio samples with vocal activity guidance and generate a diagnostic report.
    pub fn denoise_with_activity(
        &mut self,
        input: &[f32],
        activity: Option<&dsp::ActivityReport>,
    ) -> Result<(Vec<f32>, Vec<f32>, DenoiseReport), DenoiserError> {
        let start_time = Instant::now();
        let cleaned = self.process(input)?;
        let elapsed_ms = start_time.elapsed().as_secs_f32() * 1000.0;

        let duration_sec = input.len() as f32 / 48000.0;
        let rtf = (elapsed_ms / 1000.0) / duration_sec.max(0.001);

        let latency_samples = self.latency_samples();
        let latency_ms = (latency_samples as f32 / self.sample_rate() as f32) * 1000.0;

        // Phase 7: Preservation Layer with Latency Alignment & Aligned Dual Synthesis
        let preservation_layer =
            dsp::PreservationLayer::new(self.sample_rate(), self.config.preservation.clone());

        let (final_cleaned, removed_noise, pres_rep) =
            preservation_layer.process(input, &cleaned, latency_samples, activity);

        let in_power = if !input.is_empty() {
            input.iter().map(|&s| s * s).sum::<f32>() / input.len() as f32
        } else {
            1e-12
        };
        let out_power = if !final_cleaned.is_empty() {
            final_cleaned.iter().map(|&s| s * s).sum::<f32>() / final_cleaned.len() as f32
        } else {
            1e-12
        };
        let noise_power = if !removed_noise.is_empty() {
            removed_noise.iter().map(|&s| s * s).sum::<f32>() / removed_noise.len() as f32
        } else {
            1e-12
        };

        let input_rms_dbfs = 20.0 * in_power.max(1e-12).sqrt().log10();
        let cleaned_rms_dbfs = 20.0 * out_power.max(1e-12).sqrt().log10();
        let removed_noise_rms_dbfs = 20.0 * noise_power.max(1e-12).sqrt().log10();
        let attenuation_db = (input_rms_dbfs - cleaned_rms_dbfs).max(0.0);

        let report = DenoiseReport {
            model_name: self.name(),
            sample_rate: self.sample_rate(),
            latency_samples,
            latency_ms,
            duration_seconds: duration_sec,
            inference_time_ms: elapsed_ms,
            rtf,
            input_rms_dbfs,
            cleaned_rms_dbfs,
            removed_noise_rms_dbfs,
            attenuation_db,
            mean_preservation_alpha: pres_rep.mean_alpha,
            vocal_leakage_attenuation_db: pres_rep.vocal_leakage_attenuation_db,
        };

        Ok((final_cleaned, removed_noise, report))
    }
}

impl DenoiserBackend for DpdfnetDenoiser {
    fn name(&self) -> &'static str {
        "DPDFNet2-48k (sherpa-onnx)"
    }

    fn sample_rate(&self) -> u32 {
        48000
    }

    fn latency_samples(&self) -> usize {
        // sherpa-onnx OfflineSpeechDenoiser operates in offline mode, producing output
        // that is time-aligned with the input at 0 samples delay (lag 0).
        0
    }

    fn reset(&mut self) -> Result<(), DenoiserError> {
        // Offline denoiser resets state per invocation
        Ok(())
    }

    fn process(&mut self, input: &[f32]) -> Result<Vec<f32>, DenoiserError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let result = self.denoiser.run(input, 48000);
        let mut cleaned = result.samples;

        // Ensure output length aligns with input
        if cleaned.len() < input.len() {
            cleaned.resize(input.len(), 0.0);
        } else if cleaned.len() > input.len() {
            cleaned.truncate(input.len());
        }

        Ok(cleaned)
    }
}
