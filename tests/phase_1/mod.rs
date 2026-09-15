//! Phase 1 & Audio Foundation Acceptance Test Suite.
//!
//! Covers all acceptance criteria from architecture.md:
//! - Phase 1: Recorder MVP (Capture lifecycle, 2s calibration, WAV persistence, metering, device routing)
//! - Phase 2: Audio Foundation (Resampling, multi-channel downmixing, format conversions, playback transport)

mod recorder_lifecycle;
mod meter_ballistics;
mod wav_persistence;
mod device_enumeration;
mod audio_foundation;
mod playback_engine;
mod acceptance_criteria;
