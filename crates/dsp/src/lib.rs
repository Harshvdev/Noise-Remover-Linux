//! DSP Analysis and Noise Processing Engine for Linux Noise Remover.
//!
//! Provides:
//! - STFT / ISTFT time-frequency transform with Hann window and OLA normalization.
//! - Power Spectral Density (PSD) estimation and noise profile generation.
//! - Stationarity detection (evaluating fan/hiss vs burst/traffic noise).
//! - Persistent tonal peak detection (50/60 Hz mains hum and harmonics).
//! - Vocal activity and SNR confidence estimation.

pub mod activity;
pub mod analyzer;
pub mod dc_blocker;
pub mod error;
pub mod harmonicity;
pub mod latency;
pub mod mask;
pub mod noise_profile;
pub mod notch;
pub mod preservation;
pub mod processor;
pub mod residual;
pub mod stationarity;
pub mod stft;
pub mod tonal;
pub mod wiener;
pub mod window;

pub use activity::{detect_activity, ActivityConfig, ActivityReport};
pub use analyzer::{NoiseAnalyzer, SignalAnalysisReport};
pub use dc_blocker::{apply_dc_blocker, DcBlocker};
pub use error::DspError;
pub use harmonicity::{FrameHarmonicity, HarmonicityConfig, HarmonicityEstimator};
pub use latency::{CombFilterMetrics, LatencyAligner};
pub use mask::{MaskSmoother, MaskSmootherConfig};
pub use noise_profile::NoiseProfile;
pub use notch::{BiquadNotch, TonalNotchFilter};
pub use preservation::{
    AdaptivePreservationConfig, PreservationLayer, PreservationMode, PreservationReport,
};
pub use processor::{DspConfig, DspIntensity, DspProcessResult, DspProcessingReport, DspProcessor};
pub use residual::{analyze_residual, DenoiseDecision, ResidualLevel, ResidualReport};
pub use stationarity::{analyze_stationarity, StationarityReport};
pub use stft::{Spectrogram, StftEngine};
pub use tonal::{detect_tonal_peaks, find_mains_hum_peaks, TonalDetectionConfig, TonalPeak};
pub use wiener::{WienerConfig, WienerSuppressor};
pub use window::{compute_ola_normalization, hann_window};

