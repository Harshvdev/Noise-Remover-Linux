//! Model discovery and asset retrieval.

use std::path::{Path, PathBuf};
use crate::error::DenoiserError;

pub const DPDFNET2_48K_FILENAME: &str = "dpdfnet2_48khz_hr.onnx";
pub const DPDFNET2_48K_DOWNLOAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/dpdfnet2_48khz_hr.onnx";

/// Search known candidate directories for the DPDFNet2 ONNX model file.
pub fn find_dpdfnet2_model() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("models").join(DPDFNET2_48K_FILENAME),
        PathBuf::from("../models").join(DPDFNET2_48K_FILENAME),
        PathBuf::from("../../models").join(DPDFNET2_48K_FILENAME),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
            return Some(candidate.clone());
        }
    }

    if let Ok(exe_dir) = std::env::current_exe() {
        if let Some(parent) = exe_dir.parent() {
            let model_path = parent.join("models").join(DPDFNET2_48K_FILENAME);
            if model_path.exists() {
                return Some(model_path);
            }
        }
    }

    None
}

/// Download the DPDFNet2 48 kHz model to the destination path if not already present.
pub fn ensure_dpdfnet2_model<P: AsRef<Path>>(dest_dir: P) -> Result<PathBuf, DenoiserError> {
    let dir = dest_dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let target = dir.join(DPDFNET2_48K_FILENAME);

    if target.exists() && target.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        return Ok(target);
    }

    log::info!(
        "[INFO] Downloading DPDFNet2 model from {} to {:?}",
        DPDFNET2_48K_DOWNLOAD_URL,
        target
    );

    let output = std::process::Command::new("curl")
        .args([
            "-L",
            "-o",
            target.to_str().unwrap_or(""),
            DPDFNET2_48K_DOWNLOAD_URL,
        ])
        .output()
        .map_err(|e| DenoiserError::ModelInitFailed(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() || !target.exists() {
        return Err(DenoiserError::ModelInitFailed(
            "Failed to download dpdfnet2_48khz_hr.onnx".to_string(),
        ));
    }

    Ok(target)
}
