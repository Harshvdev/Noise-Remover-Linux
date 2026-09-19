//! Automatic Processing Pipeline & State Machine (Phase 9).
//!
//! Defined in Sections 53, 74, 76, and 77 of `architecture.md`:
//! - End-to-end orchestration: Analyze -> DSP -> Residual Decision -> Neural (if needed) -> Preservation -> Output.
//! - Conservative bias: preserves more vocal content whenever signal classification is ambiguous.
//! - Sequential model loading: loads neural weights only when residual noise demands it.
//! - Strict state machine lifecycle with non-blocking cancellation token support.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dsp::{
    AdaptivePreservationConfig, DenoiseDecision, DspIntensity, DspProcessingReport,
    DspProcessor, NoiseAnalyzer, NoiseProfile, PreservationMode, ResidualLevel,
    ResidualReport, SignalAnalysisReport,
};

use crate::backend::DenoiseReport;
use crate::deepfilter::{DeepFilterConfig, DeepFilterDenoiser};
use crate::dpdfnet::{DpdfnetConfig, DpdfnetDenoiser};
use crate::error::DenoiserError;
use crate::NeuralModel;

/// Explicit processing stages defined in Section 76 of `architecture.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStage {
    Idle,
    Analyzing,
    DspProcessing,
    ResidualAnalysis,
    NeuralProcessing,
    LatencyAlignment,
    Preservation,
    OutputWriting,
    Complete,
    Cancelled,
    Error,
}

impl PipelineStage {
    /// Human-readable description of current processing state.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Analyzing => "Analyzing Acoustics",
            Self::DspProcessing => "Classical DSP Noise Suppression",
            Self::ResidualAnalysis => "Residual Noise Assessment",
            Self::NeuralProcessing => "Neural AI Speech Enhancement",
            Self::LatencyAlignment => "Latency Alignment",
            Self::Preservation => "Vocal & Singing Preservation",
            Self::OutputWriting => "Writing Output Files",
            Self::Complete => "Complete",
            Self::Cancelled => "Cancelled by User",
            Self::Error => "Processing Error",
        }
    }
}

/// Thread-safe non-blocking cancellation token defined in Section 77 of `architecture.md`.
#[derive(Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal the pipeline to cancel at the next stage/block boundary.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Check whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Reset the cancellation token.
    pub fn reset(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

/// Automatic routing outcome chosen by the decision engine (Section 53 & 74).
#[derive(Debug, Clone, PartialEq)]
pub enum AutoRoute {
    /// Classical DSP alone was sufficient (saves CPU, RAM, and battery).
    DspOnly {
        residual_level: ResidualLevel,
        post_dsp_snr_db: f32,
        attenuation_db: f32,
        reason: String,
    },
    /// Neural backend was engaged to remove remaining non-stationary or high residual noise.
    Neural {
        backend: NeuralModel,
        residual_level: ResidualLevel,
        dsp_attenuation_db: f32,
        neural_attenuation_db: f32,
        reason: String,
    },
}

/// Configuration options for the Phase 9 Automatic Processing Pipeline.
#[derive(Debug, Clone)]
pub struct AutoPipelineConfig {
    /// Preferred neural model if neural processing is required (default: DPDFNet2-48k).
    pub preferred_neural_backend: NeuralModel,
    /// Force neural processing regardless of residual assessment.
    pub force_neural: bool,
    /// Force DSP-only processing, bypassing neural AI.
    pub force_dsp_only: bool,
    /// Apply conservative bias to vocal preservation (§74: preserve more when uncertain).
    pub conservative_bias: bool,
    /// DSP intensity for pre-conditioning (default: Balanced).
    pub dsp_intensity: DspIntensity,
    /// Enable Phase 7 adaptive vocal preservation layer (default: true).
    pub adaptive_preservation: bool,
    /// Enable impulse click / packet jitter filter (default: true).
    pub enable_declicker: bool,
    /// Enable plosive / wind blast filter (default: true).
    pub enable_plosive_filter: bool,
    /// Sample rate in Hz (canonical: 48000).
    pub sample_rate: u32,
}

impl Default for AutoPipelineConfig {
    fn default() -> Self {
        Self {
            preferred_neural_backend: NeuralModel::Dpdfnet2_48k,
            force_neural: false,
            force_dsp_only: false,
            conservative_bias: true,
            dsp_intensity: DspIntensity::Balanced,
            adaptive_preservation: true,
            enable_declicker: true,
            enable_plosive_filter: true,
            sample_rate: 48000,
        }
    }
}

/// Live progress notification emitted during pipeline execution.
#[derive(Debug, Clone)]
pub struct PipelineProgress {
    pub stage: PipelineStage,
    pub progress: f32,
    pub message: String,
}

/// Comprehensive report and audio outputs returned upon pipeline completion.
#[derive(Debug, Clone)]
pub struct AutoPipelineResult {
    pub cleaned_samples: Vec<f32>,
    pub removed_noise_samples: Vec<f32>,
    pub route: AutoRoute,
    pub stages_executed: Vec<PipelineStage>,
    pub noise_profile: NoiseProfile,
    pub signal_report: Option<SignalAnalysisReport>,
    pub dsp_report: DspProcessingReport,
    pub residual_report: ResidualReport,
    pub denoise_report: Option<DenoiseReport>,
    pub total_duration_ms: f32,
    pub rtf: f32,
}

/// Orchestrator for the Phase 9 Automatic Processing Pipeline.
pub struct AutoPipeline;

impl AutoPipeline {
    /// Execute the complete automatic noise removal pipeline on canonical 48 kHz mono audio.
    pub fn run<F>(
        input_samples: &[f32],
        noise_reference: Option<&[f32]>,
        config: &AutoPipelineConfig,
        cancel_token: Option<&CancellationToken>,
        mut progress_cb: F,
    ) -> Result<AutoPipelineResult, DenoiserError>
    where
        F: FnMut(PipelineProgress),
    {
        let total_start = Instant::now();
        let mut stages_executed = Vec::new();

        let check_cancel = |stage: PipelineStage| -> Result<(), DenoiserError> {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    log::warn!("[PIPELINE] Execution cancelled during stage {:?}", stage);
                    return Err(DenoiserError::Cancelled);
                }
            }
            Ok(())
        };

        if input_samples.is_empty() {
            return Err(DenoiserError::InferenceFailed("Input audio is empty".into()));
        }

        // -------------------------------------------------------------
        // Stage 1: Analyzing (Section 68, 74, 76)
        // -------------------------------------------------------------
        check_cancel(PipelineStage::Analyzing)?;
        stages_executed.push(PipelineStage::Analyzing);
        progress_cb(PipelineProgress {
            stage: PipelineStage::Analyzing,
            progress: 0.10,
            message: "Analyzing background noise PSD, stationarity, and tonal peaks...".into(),
        });

        let sample_rate = config.sample_rate;
        let analyzer = NoiseAnalyzer::new(sample_rate)
            .map_err(|e| DenoiserError::ModelInitFailed(format!("Analyzer init error: {}", e)))?;

        let noise_profile = if let Some(ref_samples) = noise_reference {
            analyzer
                .analyze_noise_reference(ref_samples)
                .map_err(|e| DenoiserError::ModelInitFailed(format!("Failed to analyze noise reference: {}", e)))?
        } else {
            // Estimate noise profile from initial 2.0s or quietest 10%
            let profile_len = (sample_rate as usize * 2).min(input_samples.len());
            analyzer
                .analyze_noise_reference(&input_samples[..profile_len])
                .map_err(|e| DenoiserError::ModelInitFailed(format!("Failed to estimate noise profile: {}", e)))?
        };

        let signal_report = analyzer.analyze_signal(input_samples, &noise_profile).ok();

        // -------------------------------------------------------------
        // Stage 2: Classical DSP Noise Suppression (Section 69, 76)
        // -------------------------------------------------------------
        check_cancel(PipelineStage::DspProcessing)?;
        stages_executed.push(PipelineStage::DspProcessing);
        progress_cb(PipelineProgress {
            stage: PipelineStage::DspProcessing,
            progress: 0.30,
            message: "Running classical DSP pre-conditioning (DC block, tonal notch, Wiener)...".into(),
        });

        let processor = DspProcessor::new(sample_rate)?;
        let mut dsp_cfg = config.dsp_intensity.to_config();
        dsp_cfg.enable_declicker = config.enable_declicker;
        dsp_cfg.enable_plosive_filter = config.enable_plosive_filter;

        let activity_ref = signal_report.as_ref().map(|r| &r.activity);
        let dsp_result = processor.process(input_samples, &noise_profile, activity_ref, &dsp_cfg)?;
        let dsp_cleaned = dsp_result.cleaned_samples;
        let dsp_removed = dsp_result.removed_noise_samples;
        let dsp_report = dsp_result.report;

        // -------------------------------------------------------------
        // Stage 3: Residual Noise Decision (Section 70, 74, 53)
        // -------------------------------------------------------------
        check_cancel(PipelineStage::ResidualAnalysis)?;
        stages_executed.push(PipelineStage::ResidualAnalysis);
        progress_cb(PipelineProgress {
            stage: PipelineStage::ResidualAnalysis,
            progress: 0.50,
            message: "Evaluating post-DSP residual noise floor and SNR...".into(),
        });

        let residual_report = dsp::analyze_residual(&dsp_cleaned, &noise_profile, activity_ref, sample_rate)?;

        // Decision logic with Conservative Bias (§74, §53)
        let route = if config.force_dsp_only {
            AutoRoute::DspOnly {
                residual_level: residual_report.level,
                post_dsp_snr_db: residual_report.post_dsp_snr_db,
                attenuation_db: dsp_report.attenuation_db,
                reason: "Forced DSP-only mode by configuration.".into(),
            }
        } else if config.force_neural {
            AutoRoute::Neural {
                backend: config.preferred_neural_backend,
                residual_level: residual_report.level,
                dsp_attenuation_db: dsp_report.attenuation_db,
                neural_attenuation_db: 0.0,
                reason: "Forced Neural AI mode by configuration.".into(),
            }
        } else {
            // Automatic decision:
            let is_stationary_noise = noise_profile.stationarity_score >= 0.65;
            let is_dsp_sufficient = residual_report.decision == DenoiseDecision::FinishWithoutAi
                && residual_report.residual_noise_dbfs <= -52.0
                && residual_report.post_dsp_snr_db >= 25.0;

            if is_dsp_sufficient && is_stationary_noise {
                AutoRoute::DspOnly {
                    residual_level: residual_report.level,
                    post_dsp_snr_db: residual_report.post_dsp_snr_db,
                    attenuation_db: dsp_report.attenuation_db,
                    reason: format!(
                        "Classical DSP eliminated stationary background noise ({:.1} dB attenuation, SNR {:.1} dB). Neural AI bypassed to preserve 100% natural vocal timbre.",
                        dsp_report.attenuation_db, residual_report.post_dsp_snr_db
                    ),
                }
            } else {
                let reason = format!(
                    "Residual noise remains {} ({:.1} dBFS post-DSP). Engaging {} to eliminate in-band noise.",
                    residual_report.level.as_str(),
                    residual_report.residual_noise_dbfs,
                    config.preferred_neural_backend.display_name()
                );
                AutoRoute::Neural {
                    backend: config.preferred_neural_backend,
                    residual_level: residual_report.level,
                    dsp_attenuation_db: dsp_report.attenuation_db,
                    neural_attenuation_db: 0.0,
                    reason,
                }
            }
        };

        // -------------------------------------------------------------
        // Stage 4: Execution based on Selected Route
        // -------------------------------------------------------------
        let (final_cleaned, final_removed, denoise_report) = match &route {
            AutoRoute::DspOnly { .. } => {
                log::info!("[PIPELINE] Selected DSP-only path. Bypassing neural model.");
                stages_executed.push(PipelineStage::Preservation);
                stages_executed.push(PipelineStage::Complete);
                progress_cb(PipelineProgress {
                    stage: PipelineStage::Complete,
                    progress: 1.0,
                    message: "Completed with classical DSP noise removal.".into(),
                });
                (dsp_cleaned, dsp_removed, None)
            }
            AutoRoute::Neural { backend, .. } => {
                check_cancel(PipelineStage::NeuralProcessing)?;
                stages_executed.push(PipelineStage::NeuralProcessing);
                progress_cb(PipelineProgress {
                    stage: PipelineStage::NeuralProcessing,
                    progress: 0.65,
                    message: format!("Running {} speech enhancement...", backend.display_name()),
                });

                // Sequential model loading (§54): only load the selected model here
                // Apply Conservative Bias (§74): preserve more when uncertain
                let preservation_mode = if config.adaptive_preservation {
                    let mut adapt_cfg = AdaptivePreservationConfig::default();
                    if config.conservative_bias {
                        // Conservative bias parameters
                        adapt_cfg.min_vocal_alpha = 0.50;
                        adapt_cfg.vocal_protection_strength = 0.90;
                        adapt_cfg.harmonic_protection_strength = 0.95;
                    }
                    PreservationMode::Adaptive(adapt_cfg)
                } else {
                    PreservationMode::Global(1.0)
                };

                let (cleaned, removed, rep) = match backend {
                    NeuralModel::Dpdfnet2_48k => {
                        let dpdfnet_cfg = DpdfnetConfig {
                            preservation: preservation_mode,
                            ..Default::default()
                        };
                        let mut denoiser = DpdfnetDenoiser::new(dpdfnet_cfg)?;
                        check_cancel(PipelineStage::NeuralProcessing)?;
                        denoiser.denoise_with_activity(&dsp_cleaned, activity_ref)?
                    }
                    NeuralModel::DeepFilterNet3 => {
                        let df_cfg = DeepFilterConfig {
                            preservation: preservation_mode,
                            compensate_delay: true,
                            ..Default::default()
                        };
                        let mut denoiser = DeepFilterDenoiser::new(df_cfg)?;
                        check_cancel(PipelineStage::NeuralProcessing)?;
                        denoiser.denoise_with_activity(&dsp_cleaned, activity_ref)?
                    }
                };

                stages_executed.push(PipelineStage::LatencyAlignment);
                stages_executed.push(PipelineStage::Preservation);
                stages_executed.push(PipelineStage::Complete);

                progress_cb(PipelineProgress {
                    stage: PipelineStage::Complete,
                    progress: 1.0,
                    message: format!(
                        "Completed {} enhancement with vocal preservation.",
                        backend.display_name()
                    ),
                });

                (cleaned, removed, Some(rep))
            }
        };

        check_cancel(PipelineStage::Complete)?;

        let total_duration_ms = total_start.elapsed().as_secs_f32() * 1000.0;
        let audio_len_sec = input_samples.len() as f32 / sample_rate as f32;
        let rtf = (total_duration_ms / 1000.0) / audio_len_sec.max(0.001);

        log::info!(
            "[PIPELINE] Complete. Audio: {:.2}s, Elapsed: {:.1}ms (RTF: {:.3}). Route: {:?}",
            audio_len_sec,
            total_duration_ms,
            rtf,
            route
        );

        Ok(AutoPipelineResult {
            cleaned_samples: final_cleaned,
            removed_noise_samples: final_removed,
            route,
            stages_executed,
            noise_profile,
            signal_report,
            dsp_report,
            residual_report,
            denoise_report,
            total_duration_ms,
            rtf,
        })
    }
}
