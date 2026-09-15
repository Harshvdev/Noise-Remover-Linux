//! Phase 3 — Noise Analyzer Test Suite.
//!
//! Validates all core DSP and spectral analysis capabilities:
//! - STFT / ISTFT invertible synthesis and Hann window OLA
//! - Noise Power Spectral Density (PSD) and robust median estimation
//! - Stationarity detection (stationary fan/hiss vs bursty/traffic noise)
//! - Persistent tonal peak detection (50/60 Hz mains hum and harmonics)
//! - Vocal activity confidence and SNR estimation
//! - Explicit Phase 3 acceptance criteria (§68)

mod acceptance_criteria;
mod noise_psd;
mod stationarity;
mod stft_istft;
mod tonal_hum;
mod vocal_activity;
