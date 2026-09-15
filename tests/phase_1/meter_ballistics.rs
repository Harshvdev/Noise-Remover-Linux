//! Tests covering AudioMeter peak, RMS, clipping, and ballistics calculations.

use recorder::meter::AudioMeter;

#[test]
fn test_meter_initialization() {
    let meter = AudioMeter::new();
    assert_eq!(meter.peak_dbfs(), -96.0);
    assert_eq!(meter.rms_dbfs(), -96.0);
    assert!(!meter.take_clipping());
}

#[test]
fn test_meter_instant_peak_rise() {
    let meter = AudioMeter::new();

    // Full scale 1.0 sample buffer
    let full_scale = [1.0f32; 128];
    meter.update(&full_scale);

    // Peak should instantly register at 0 dBFS
    assert!((meter.peak_dbfs() - 0.0).abs() < 1e-4);
    assert!(meter.take_clipping());
}

#[test]
fn test_meter_smooth_decay() {
    let meter = AudioMeter::new();

    // Spike to 0 dBFS
    meter.update(&[1.0f32; 64]);
    let initial_peak = meter.peak_dbfs();
    assert!((initial_peak - 0.0).abs() < 1e-4);

    // Feed silence: peak must decay smoothly rather than dropping instantly to -96 dBFS
    let silence = [0.0f32; 128];
    meter.update(&silence);

    let decayed_peak = meter.peak_dbfs();
    assert!(decayed_peak < initial_peak);
    assert!(decayed_peak > -96.0); // Smooth fall
}

#[test]
fn test_meter_clipping_latch() {
    let meter = AudioMeter::new();

    // Clipped buffer
    meter.update(&[1.05f32; 10]);
    // First take_clipping must return true
    assert!(meter.take_clipping());
    // Subsequent call must return false (latched reset)
    assert!(!meter.take_clipping());
}

#[test]
fn test_meter_does_not_mutate_input_buffer() {
    let meter = AudioMeter::new();
    let original = vec![0.123f32, -0.456, 0.789, 0.0];
    let buffer = original.clone();

    meter.update(&buffer);
    assert_eq!(buffer, original);
}
