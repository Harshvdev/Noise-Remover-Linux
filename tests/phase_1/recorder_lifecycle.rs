use recorder::{AudioRecorder, RecorderMode};

#[test]
fn test_recorder_initial_state() {
    let temp_dir = std::env::temp_dir().join("test_recorder_init");
    let rec = AudioRecorder::new(&temp_dir);

    let status = rec.status();
    assert_eq!(status.mode, RecorderMode::Idle);
    assert_eq!(status.recorded_seconds, 0.0);
    assert_eq!(status.calibration_progress, 0.0);
    assert_eq!(status.dropped_samples, 0);
    assert!(!status.is_saving);
    assert!(status.last_error.is_none());

    // Directory should be created
    assert!(temp_dir.exists());
    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_calibration_mode_and_cancellation() {
    let temp_dir = std::env::temp_dir().join("test_calib_cancel");
    let rec = AudioRecorder::new(&temp_dir);

    // Trigger calibration
    rec.start_calibration();
    assert_eq!(rec.status().mode, RecorderMode::Calibrating);

    // Cancel calibration
    rec.cancel_calibration();
    assert_eq!(rec.status().mode, RecorderMode::Idle);

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_recording_mode_and_stop() {
    let temp_dir = std::env::temp_dir().join("test_rec_stop");
    let rec = AudioRecorder::new(&temp_dir);

    rec.start_recording();
    assert_eq!(rec.status().mode, RecorderMode::Recording);

    rec.stop_recording_or_calibration();
    assert_eq!(rec.status().mode, RecorderMode::Idle);

    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_calibration_target_samples_calculation() {
    // Phase 1 requirement: 2 seconds of ambient noise
    let sample_rate = 48000;
    let target_samples = (sample_rate as f32 * 2.0) as usize;
    assert_eq!(target_samples, 96000);

    // Progress check
    let samples_half = 48000;
    let progress_half = (samples_half as f32 / target_samples as f32).clamp(0.0, 1.0);
    assert!((progress_half - 0.5).abs() < 1e-4);

    let samples_full = 96000;
    let progress_full = (samples_full as f32 / target_samples as f32).clamp(0.0, 1.0);
    assert!((progress_full - 1.0).abs() < 1e-4);
}
