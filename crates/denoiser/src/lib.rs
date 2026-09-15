//! Neural Audio Denoiser Subsystem for Linux Voice & Singing Noise Remover.
//!
//! Provides Phase 6 neural inference backends:
//! - DPDFNet2-48k (sherpa-onnx / ONNX Runtime) high-resolution 48 kHz denoiser.
//! - Generic `DenoiserBackend` trait for extensible model benchmarking.
//! - Asset discovery and download management for pre-trained weights.

pub mod backend;
pub mod dpdfnet;
pub mod error;
pub mod model;

pub use backend::{DenoiseReport, DenoiserBackend};
pub use dpdfnet::{DpdfnetConfig, DpdfnetDenoiser};
pub use error::DenoiserError;
pub use model::{ensure_dpdfnet2_model, find_dpdfnet2_model, DPDFNET2_48K_FILENAME};
