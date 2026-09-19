//! DeepFilterNet3 Neural Denoiser Implementation.
//!
//! Wraps the standalone DeepFilterNet3 native runtime (`deep-filter`) embedding the
//! tract-based multi-stage model (encoder + ERB decoder + complex DF decoder).
//! Provides delay compensation (`-D`), Phase 7 vocal preservation, and aligned dual synthesis.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::backend::{DenoiseReport, DenoiserBackend};
use crate::error::DenoiserError;
use crate::model::find_deepfilter_binary;

/// Configuration options for DeepFilterNet3 inference.
#[derive(Debug, Clone)]
pub struct DeepFilterConfig {
    /// Path to `deep-filter` executable binary.
    pub binary_path: PathBuf,
    /// Compensate delay of STFT and model lookahead (default: true for 0 samples lag).
    pub compensate_delay: bool,
    /// Enable post-filter (default: false for maximum natural timbre).
    pub post_filter: bool,
    /// Post-filter beta (default: 0.02, higher = stronger attenuation).
    pub post_filter_beta: f32,
    /// Attenuation limit in dB (default: 100.0 = full attenuation).
    pub attenuation_limit_db: f32,
    /// Preservation layer blending settings (Phase 7).
    pub preservation: dsp::PreservationMode,
}

impl Default for DeepFilterConfig {
    fn default() -> Self {
        let binary_path = find_deepfilter_binary()
            .unwrap_or_else(|| PathBuf::from("models/bin/deep-filter"));
        Self {
            binary_path,
            compensate_delay: true,
            post_filter: false,
            post_filter_beta: 0.02,
            attenuation_limit_db: 100.0,
            preservation: dsp::PreservationMode::Global(1.0),
        }
    }
}

/// DeepFilterNet3 Neural Denoiser Backend.
pub struct DeepFilterDenoiser {
    config: DeepFilterConfig,
}

impl DeepFilterDenoiser {
    /// Initialize the DeepFilterNet3 denoiser with custom configuration.
    pub fn new(config: DeepFilterConfig) -> Result<Self, DenoiserError> {
        if !config.binary_path.exists() {
            return Err(DenoiserError::ModelNotFound(config.binary_path.clone()));
        }

        // Test running --version to verify binary is functional
        let out = std::process::Command::new(&config.binary_path)
            .arg("--version")
            .output()
            .map_err(|e| {
                DenoiserError::ModelInitFailed(format!(
                    "Failed to execute DeepFilterNet3 binary at {:?}: {}",
                    config.binary_path, e
                ))
            })?;

        if !out.status.success() {
            return Err(DenoiserError::ModelInitFailed(format!(
                "DeepFilterNet3 binary at {:?} exited with non-zero status",
                config.binary_path
            )));
        }

        log::info!(
            "[INFO] DeepFilterNet3 initialized successfully from {:?}",
            config.binary_path
        );

        Ok(Self { config })
    }

    /// Load the DeepFilterNet3 denoiser using default model search paths.
    pub fn load_default() -> Result<Self, DenoiserError> {
        Self::new(DeepFilterConfig::default())
    }

    /// Load the denoiser from a specific binary path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DenoiserError> {
        let config = DeepFilterConfig {
            binary_path: path.as_ref().to_path_buf(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Access the active configuration.
    pub fn config(&self) -> &DeepFilterConfig {
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

impl DenoiserBackend for DeepFilterDenoiser {
    fn name(&self) -> &'static str {
        "DeepFilterNet3 (tract)"
    }

    fn sample_rate(&self) -> u32 {
        48000
    }

    fn latency_samples(&self) -> usize {
        if self.config.compensate_delay {
            0
        } else {
            1440
        }
    }

    fn reset(&mut self) -> Result<(), DenoiserError> {
        Ok(())
    }

    fn process(&mut self, input: &[f32]) -> Result<Vec<f32>, DenoiserError> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        if !self.config.binary_path.exists() {
            return Err(DenoiserError::ModelNotFound(self.config.binary_path.clone()));
        }

        // Fast RAM-disk directory if available (/dev/shm) else system temp dir
        let base_temp = if Path::new("/dev/shm").is_dir()
            && std::fs::metadata("/dev/shm")
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        {
            PathBuf::from("/dev/shm")
        } else {
            std::env::temp_dir()
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let session_name = format!("dfnet3_{}_{}_{}", std::process::id(), now.as_secs(), now.subsec_nanos());
        let run_dir = base_temp.join(session_name);
        std::fs::create_dir_all(&run_dir)?;

        let input_wav = run_dir.join("input.wav");
        let out_dir = run_dir.join("out");
        std::fs::create_dir_all(&out_dir)?;

        // Write canonical 48 kHz mono float WAV
        audio_core::write_wav_f32(&input_wav, input, 48000, 1)
            .map_err(|e| DenoiserError::ModelInitFailed(format!("Failed to write temp input wav: {}", e)))?;

        let mut cmd = std::process::Command::new(&self.config.binary_path);
        if self.config.compensate_delay {
            cmd.arg("-D");
        }
        if self.config.post_filter {
            cmd.arg("--pf");
            cmd.arg("--pf-beta");
            cmd.arg(format!("{}", self.config.post_filter_beta));
        }
        cmd.arg("-a");
        cmd.arg(format!("{}", self.config.attenuation_limit_db));
        cmd.arg("-o");
        cmd.arg(&out_dir);
        cmd.arg(&input_wav);

        let output = cmd.output().map_err(|e| {
            let _ = std::fs::remove_dir_all(&run_dir);
            DenoiserError::InferenceFailed(format!("Failed to execute deep-filter binary: {}", e))
        })?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr).to_string();
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(DenoiserError::InferenceFailed(format!(
                "deep-filter process failed: {}",
                err_msg
            )));
        }

        let out_wav = out_dir.join("input.wav");
        if !out_wav.exists() {
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(DenoiserError::InferenceFailed(
                "deep-filter produced no output file".to_string(),
            ));
        }

        let (mut cleaned, _) = audio_core::read_wav_canonical_f32(&out_wav)
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&run_dir);
                DenoiserError::InferenceFailed(format!("Failed to read cleaned wav: {}", e))
            })?;

        // Cleanup temporary run directory immediately
        let _ = std::fs::remove_dir_all(&run_dir);

        // Ensure output length aligns with input
        if cleaned.len() < input.len() {
            cleaned.resize(input.len(), 0.0);
        } else if cleaned.len() > input.len() {
            cleaned.truncate(input.len());
        }

        Ok(cleaned)
    }
}
