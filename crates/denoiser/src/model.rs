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

pub const DEEPFILTER_BINARY_FILENAME: &str = "deep-filter";
pub const DEEPFILTER_BINARY_DOWNLOAD_URL: &str =
    "https://github.com/Rikorose/DeepFilterNet/releases/download/v0.5.6/deep-filter-0.5.6-x86_64-unknown-linux-musl";

/// Search known candidate directories for the DeepFilterNet3 standalone binary.
pub fn find_deepfilter_binary() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("models/bin").join(DEEPFILTER_BINARY_FILENAME),
        PathBuf::from("../models/bin").join(DEEPFILTER_BINARY_FILENAME),
        PathBuf::from("../../models/bin").join(DEEPFILTER_BINARY_FILENAME),
        PathBuf::from("bin").join(DEEPFILTER_BINARY_FILENAME),
        PathBuf::from("/home/thesky/.gemini/antigravity-ide/brain/6d4ef171-5f39-4ceb-bdaa-fb3e55988872/scratch")
            .join(DEEPFILTER_BINARY_FILENAME),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
            return Some(candidate.clone());
        }
    }

    if let Ok(exe_dir) = std::env::current_exe() {
        if let Some(parent) = exe_dir.parent() {
            let p1 = parent.join("models").join("bin").join(DEEPFILTER_BINARY_FILENAME);
            if p1.exists() {
                return Some(p1);
            }
            let p2 = parent.join(DEEPFILTER_BINARY_FILENAME);
            if p2.exists() {
                return Some(p2);
            }
        }
    }

    // Check system PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let bin = dir.join(DEEPFILTER_BINARY_FILENAME);
            if bin.exists() && bin.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
                return Some(bin);
            }
        }
    }

    None
}

/// Download the DeepFilterNet3 standalone binary to the destination directory if not already present.
pub fn ensure_deepfilter_binary<P: AsRef<Path>>(dest_dir: P) -> Result<PathBuf, DenoiserError> {
    let dir = dest_dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let target = dir.join(DEEPFILTER_BINARY_FILENAME);

    if target.exists() && target.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
        }
        return Ok(target);
    }

    log::info!(
        "[INFO] Downloading DeepFilterNet3 binary from {} to {:?}",
        DEEPFILTER_BINARY_DOWNLOAD_URL,
        target
    );

    let output = std::process::Command::new("curl")
        .args([
            "-L",
            "-o",
            target.to_str().unwrap_or(""),
            DEEPFILTER_BINARY_DOWNLOAD_URL,
        ])
        .output()
        .map_err(|e| DenoiserError::ModelInitFailed(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() || !target.exists() {
        return Err(DenoiserError::ModelInitFailed(
            "Failed to download deep-filter binary".to_string(),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }

    Ok(target)
}
