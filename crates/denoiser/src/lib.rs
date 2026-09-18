//! Neural Audio Denoiser Subsystem for Linux Voice & Singing Noise Remover.
//!
//! Provides Phase 6 neural inference backends:
//! - DPDFNet2-48k (sherpa-onnx / ONNX Runtime) high-resolution 48 kHz denoiser.
//! - Generic `DenoiserBackend` trait for extensible model benchmarking.
//! - Asset discovery and download management for pre-trained weights.

pub mod backend;
pub mod deepfilter;
pub mod dpdfnet;
pub mod error;
pub mod model;

pub use backend::{DenoiseReport, DenoiserBackend};
pub use deepfilter::{DeepFilterConfig, DeepFilterDenoiser};
pub use dpdfnet::{DpdfnetConfig, DpdfnetDenoiser};
pub use error::DenoiserError;
pub use model::{
    ensure_deepfilter_binary, ensure_dpdfnet2_model, find_deepfilter_binary, find_dpdfnet2_model,
    DEEPFILTER_BINARY_FILENAME, DPDFNET2_48K_FILENAME,
};

/// Available neural enhancement models for voice and singing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NeuralModel {
    /// DPDFNet2 48 kHz High-Resolution Speech Enhancement (sherpa-onnx / ONNX Runtime).
    #[default]
    Dpdfnet2_48k,
    /// DeepFilterNet3 Full-Band 48 kHz Multi-Stage Speech Enhancement (tract runtime).
    DeepFilterNet3,
}

impl NeuralModel {
    /// Display name of the neural model.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Dpdfnet2_48k => "DPDFNet2-48k (sherpa-onnx)",
            Self::DeepFilterNet3 => "DeepFilterNet3 (tract)",
        }
    }

    /// Short model identifier.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Dpdfnet2_48k => "dpdfnet2_48k",
            Self::DeepFilterNet3 => "deepfilter_net3",
        }
    }
}
