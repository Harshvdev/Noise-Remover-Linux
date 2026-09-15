//! Audio capture, ring buffer streaming, and WAV recording engine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig, SupportedStreamConfig};
use hound::{WavSpec, WavWriter};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

use crate::device::DeviceManager;
use crate::error::RecorderError;
use crate::meter::AudioMeter;
use audio_core::pcm::{i16_to_f32, i32_to_f32, interleaved_to_mono};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderMode {
    Idle,
    Calibrating,
    Recording,
}

#[derive(Debug, Clone)]
pub struct RecorderStatus {
    pub mode: RecorderMode,
    pub sample_rate: u32,
    pub channels: u16,
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub has_clipped: bool,
    pub recorded_seconds: f32,
    pub calibration_progress: f32,
    pub dropped_samples: usize,
    pub is_saving: bool,
    pub last_error: Option<String>,
}

pub struct AudioRecorder {
    device_manager: DeviceManager,
    selected_device_idx: Option<usize>,
    stream: Option<Stream>,
    active_config: Option<StreamConfig>,
    meter: Arc<AudioMeter>,
    is_capturing: Arc<AtomicBool>,
    mode: Arc<Mutex<RecorderMode>>,
    output_dir: PathBuf,
    writer_handle: Option<JoinHandle<()>>,
    recorded_seconds: Arc<Mutex<f32>>,
    calibration_progress: Arc<Mutex<f32>>,
    last_recording_path: Arc<Mutex<Option<PathBuf>>>,
    last_calibration_path: Arc<Mutex<Option<PathBuf>>>,
    dropped_samples: Arc<std::sync::atomic::AtomicUsize>,
    is_saving: Arc<AtomicBool>,
    finalized_count: Arc<std::sync::atomic::AtomicUsize>,
    last_stream_error: Arc<Mutex<Option<String>>>,
}

impl AudioRecorder {
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        let dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).ok();

        Self {
            device_manager: DeviceManager::new(),
            selected_device_idx: None,
            stream: None,
            active_config: None,
            meter: AudioMeter::new(),
            is_capturing: Arc::new(AtomicBool::new(false)),
            mode: Arc::new(Mutex::new(RecorderMode::Idle)),
            output_dir: dir,
            writer_handle: None,
            recorded_seconds: Arc::new(Mutex::new(0.0)),
            calibration_progress: Arc::new(Mutex::new(0.0)),
            last_recording_path: Arc::new(Mutex::new(None)),
            last_calibration_path: Arc::new(Mutex::new(None)),
            dropped_samples: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            is_saving: Arc::new(AtomicBool::new(false)),
            finalized_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            last_stream_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn device_manager(&self) -> &DeviceManager {
        &self.device_manager
    }

    pub fn selected_device_idx(&self) -> Option<usize> {
        self.selected_device_idx
    }

    /// Select default device.
    pub fn select_default_device(&mut self) -> Result<(), RecorderError> {
        self.stop_stream();
        *self.last_stream_error.lock().unwrap() = None;

        let pw_sources = DeviceManager::get_pipewire_sources();
        if !pw_sources.is_empty() {
            let def_idx = pw_sources.iter().position(|s| s.is_default).unwrap_or(0);
            self.selected_device_idx = Some(def_idx);
            let _ = self.device_manager.select_pipewire_source(def_idx);
        }

        let device = self
            .device_manager
            .default_input_device()
            .ok_or(RecorderError::NoSupportedConfig)?;
        let default_config = device.default_input_config()?;

        if self.selected_device_idx.is_none() {
            if let Ok(devices) = self.device_manager.list_input_devices() {
                if let Some((idx, _)) = devices.iter().find(|(_, name)| name.starts_with("Default")) {
                    self.selected_device_idx = Some(*idx);
                } else if !devices.is_empty() {
                    self.selected_device_idx = Some(devices[0].0);
                }
            }
        }

        self.start_capture_stream(&device, default_config)?;
        Ok(())
    }

    /// Select an input device by index and start real-time monitoring (level meter).
    pub fn select_device(&mut self, index: usize) -> Result<(), RecorderError> {
        self.stop_stream();
        *self.last_stream_error.lock().unwrap() = None;

        let pw_sources = DeviceManager::get_pipewire_sources();
        if !pw_sources.is_empty() {
            self.device_manager.select_pipewire_source(index)?;
            let device = self
                .device_manager
                .default_input_device()
                .ok_or(RecorderError::NoSupportedConfig)?;
            let default_config = device.default_input_config()?;
            self.selected_device_idx = Some(index);
            self.start_capture_stream(&device, default_config)?;
            return Ok(());
        }

        let device = self.device_manager.get_input_device(index)?;
        let default_config = device.default_input_config()?;
        self.selected_device_idx = Some(index);
        self.start_capture_stream(&device, default_config)?;

        Ok(())
    }

    /// Check if the currently selected input device is muted in system settings.
    pub fn is_selected_device_muted(&self) -> bool {
        if let Some(idx) = self.selected_device_idx {
            self.device_manager.is_pipewire_source_muted(idx)
        } else {
            false
        }
    }

    /// Unmute the currently selected input device in system settings.
    pub fn unmute_selected_device(&self) -> Result<(), RecorderError> {
        if let Some(idx) = self.selected_device_idx {
            self.device_manager.unmute_pipewire_source(idx)
        } else {
            Ok(())
        }
    }

    /// Start 2-second ambient noise calibration.
    pub fn start_calibration(&self) {
        *self.mode.lock().unwrap() = RecorderMode::Calibrating;
        *self.calibration_progress.lock().unwrap() = 0.0;
    }

    /// Cancel active calibration.
    pub fn cancel_calibration(&self) {
        let mut m = self.mode.lock().unwrap();
        if *m == RecorderMode::Calibrating {
            *m = RecorderMode::Idle;
        }
    }

    /// Start recording voice/singing.
    pub fn start_recording(&self) {
        *self.mode.lock().unwrap() = RecorderMode::Recording;
        *self.recorded_seconds.lock().unwrap() = 0.0;
    }

    /// Stop recording or calibration.
    pub fn stop_recording_or_calibration(&self) {
        *self.mode.lock().unwrap() = RecorderMode::Idle;
    }

    /// Get current status for GUI rendering.
    pub fn status(&self) -> RecorderStatus {
        let (sample_rate, channels) = match &self.active_config {
            Some(c) => (c.sample_rate, c.channels),
            None => (0, 0),
        };

        let mode = *self.mode.lock().unwrap();
        let recorded_seconds = *self.recorded_seconds.lock().unwrap();
        let calibration_progress = *self.calibration_progress.lock().unwrap();
        let dropped_samples = self.dropped_samples.load(Ordering::Relaxed);
        let is_saving = self.is_saving.load(Ordering::Relaxed);
        let last_error = self.last_stream_error.lock().unwrap().clone();

        RecorderStatus {
            mode,
            sample_rate,
            channels,
            peak_dbfs: self.meter.peak_dbfs(),
            rms_dbfs: self.meter.rms_dbfs(),
            has_clipped: self.meter.take_clipping(),
            recorded_seconds,
            calibration_progress,
            dropped_samples,
            is_saving,
            last_error,
        }
    }

    pub fn is_saving(&self) -> bool {
        self.is_saving.load(Ordering::Relaxed)
    }

    pub fn finalized_count(&self) -> usize {
        self.finalized_count.load(Ordering::Relaxed)
    }

    pub fn last_recording_path(&self) -> Option<PathBuf> {
        self.last_recording_path.lock().unwrap().clone()
    }

    pub fn last_calibration_path(&self) -> Option<PathBuf> {
        self.last_calibration_path.lock().unwrap().clone()
    }

    fn stop_stream(&mut self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        self.stream = None;
        if let Some(h) = self.writer_handle.take() {
            let _ = h.join();
        }
    }

    fn start_capture_stream(
        &mut self,
        device: &Device,
        supported_config: SupportedStreamConfig,
    ) -> Result<(), RecorderError> {
        let config: StreamConfig = supported_config.into();
        let sample_rate = config.sample_rate;
        let channels = config.channels as usize;

        // Allocate 5 seconds worth of ring buffer capacity
        let buffer_capacity = (sample_rate as usize * channels * 5).max(16384);
        let ring_buffer = HeapRb::<f32>::new(buffer_capacity);
        let (mut producer, mut consumer) = ring_buffer.split();

        self.is_capturing.store(true, Ordering::SeqCst);
        let is_capturing = self.is_capturing.clone();
        let meter = self.meter.clone();
        let mode = self.mode.clone();
        let output_dir = self.output_dir.clone();
        let recorded_seconds = self.recorded_seconds.clone();
        let calibration_progress = self.calibration_progress.clone();
        let last_recording_path = self.last_recording_path.clone();
        let last_calibration_path = self.last_calibration_path.clone();
        let is_saving = self.is_saving.clone();
        let dropped_samples = self.dropped_samples.clone();
        let finalized_count = self.finalized_count.clone();
        let last_stream_error = self.last_stream_error.clone();

        // Writer worker thread drains consumer and writes WAV files
        self.writer_handle = Some(thread::spawn(move || {
            let mut current_writer: Option<WavWriter<std::io::BufWriter<std::fs::File>>> = None;
            let mut active_mode = RecorderMode::Idle;
            let mut samples_in_file = 0usize;
            let calibration_target_samples = (sample_rate as f32 * 2.0) as usize;

            let mut drain_buf = vec![0.0f32; 4096];
            let mut remainder_buf = Vec::<f32>::new();

            // Helper to process interleaved samples frame-by-frame and write mono
            let process_and_write = |raw_samples: &[f32],
                                     remainder: &mut Vec<f32>,
                                     writer: &mut Option<WavWriter<std::io::BufWriter<std::fs::File>>>,
                                     ch: usize|
             -> usize {
                if raw_samples.is_empty() && remainder.is_empty() {
                    return 0;
                }

                let mut combined = std::mem::take(remainder);
                combined.extend_from_slice(raw_samples);

                let usable_frames = combined.len().checked_div(ch).unwrap_or(0);
                let usable_samples = usable_frames * ch;

                if usable_samples < combined.len() {
                    *remainder = combined[usable_samples..].to_vec();
                }

                if usable_samples == 0 {
                    return 0;
                }

                let mono_samples = interleaved_to_mono(&combined[..usable_samples], ch);
                if let Some(ref mut w) = writer {
                    for &s in &mono_samples {
                        let _ = w.write_sample(s);
                    }
                }
                mono_samples.len()
            };

            while is_capturing.load(Ordering::Relaxed) {
                let current_target_mode = *mode.lock().unwrap();

                // Mode transition handling
                if current_target_mode != active_mode {
                    // If we were recording, drain in-flight audio currently in the ring buffer
                    if active_mode == RecorderMode::Recording {
                        is_saving.store(true, Ordering::SeqCst);
                        loop {
                            let read = consumer.pop_slice(&mut drain_buf);
                            if read == 0 {
                                break;
                            }
                            process_and_write(&drain_buf[..read], &mut remainder_buf, &mut current_writer, channels);
                        }
                        process_and_write(&[], &mut remainder_buf, &mut current_writer, channels);

                        if let Some(w) = current_writer.take() {
                            let _ = w.finalize();
                        }
                        finalized_count.fetch_add(1, Ordering::SeqCst);
                        is_saving.store(false, Ordering::SeqCst);
                    } else if let Some(w) = current_writer.take() {
                        // For canceled calibration or other modes, close writer immediately
                        let _ = w.finalize();
                        finalized_count.fetch_add(1, Ordering::SeqCst);
                    }

                    active_mode = current_target_mode;
                    samples_in_file = 0;

                    match active_mode {
                        RecorderMode::Calibrating => {
                            let path = output_dir.join("noise_reference.wav");
                            let spec = WavSpec {
                                channels: 1,
                                sample_rate,
                                bits_per_sample: 32,
                                sample_format: hound::SampleFormat::Float,
                            };
                            if let Ok(w) = WavWriter::create(&path, spec) {
                                current_writer = Some(w);
                                *last_calibration_path.lock().unwrap() = Some(path);
                            }
                        }
                        RecorderMode::Recording => {
                            let path = output_dir.join("original.wav");
                            let spec = WavSpec {
                                channels: 1,
                                sample_rate,
                                bits_per_sample: 32,
                                sample_format: hound::SampleFormat::Float,
                            };
                            if let Ok(w) = WavWriter::create(&path, spec) {
                                current_writer = Some(w);
                                *last_recording_path.lock().unwrap() = Some(path);
                            }
                        }
                        RecorderMode::Idle => {
                            // nothing to open
                        }
                    }
                }

                // Drain ring buffer during normal capture
                let read = consumer.pop_slice(&mut drain_buf);
                if read > 0 {
                    let written_mono = process_and_write(
                        &drain_buf[..read],
                        &mut remainder_buf,
                        &mut current_writer,
                        channels,
                    );
                    samples_in_file += written_mono;

                    match active_mode {
                        RecorderMode::Recording => {
                            let secs = samples_in_file as f32 / sample_rate as f32;
                            *recorded_seconds.lock().unwrap() = secs;
                        }
                        RecorderMode::Calibrating => {
                            let progress = (samples_in_file as f32
                                / calibration_target_samples as f32)
                                .clamp(0.0, 1.0);
                            *calibration_progress.lock().unwrap() = progress;

                            if samples_in_file >= calibration_target_samples {
                                // 2-second calibration completed automatically - finalize immediately
                                process_and_write(&[], &mut remainder_buf, &mut current_writer, channels);
                                if let Some(w) = current_writer.take() {
                                    let _ = w.finalize();
                                }
                                finalized_count.fetch_add(1, Ordering::SeqCst);
                                *calibration_progress.lock().unwrap() = 1.0;
                                let mut m = mode.lock().unwrap();
                                *m = RecorderMode::Idle;
                                active_mode = RecorderMode::Idle;
                            }
                        }
                        RecorderMode::Idle => {}
                    }
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }

            // Finalize writer on shutdown with non-blocking drain
            loop {
                let read = consumer.pop_slice(&mut drain_buf);
                if read == 0 {
                    break;
                }
                process_and_write(&drain_buf[..read], &mut remainder_buf, &mut current_writer, channels);
            }
            process_and_write(&[], &mut remainder_buf, &mut current_writer, channels);

            if let Some(w) = current_writer.take() {
                let _ = w.finalize();
            }
            finalized_count.fetch_add(1, Ordering::SeqCst);
        }));

        // Audio callback with error capture and overrun tracking
        let stream_err_slot = last_stream_error.clone();
        let cap_flag = self.is_capturing.clone();
        let err_fn = move |err| {
            log::error!("CPAL audio input stream error: {:?}", err);
            *stream_err_slot.lock().unwrap() = Some(format!("{}", err));
            cap_flag.store(false, Ordering::SeqCst);
        };

        let sample_format = supported_config.sample_format();
        let drop_cnt = dropped_samples.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    meter.update(data);
                    let pushed = producer.push_slice(data);
                    if pushed < data.len() {
                        drop_cnt.fetch_add(data.len() - pushed, Ordering::Relaxed);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut scratch = [0.0f32; 1024];
                    for chunk in data.chunks(1024) {
                        for (i, &sample) in chunk.iter().enumerate() {
                            scratch[i] = i16_to_f32(sample);
                        }
                        meter.update(&scratch[..chunk.len()]);
                        let pushed = producer.push_slice(&scratch[..chunk.len()]);
                        if pushed < chunk.len() {
                            drop_cnt.fetch_add(chunk.len() - pushed, Ordering::Relaxed);
                        }
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I32 => device.build_input_stream(
                config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    let mut scratch = [0.0f32; 1024];
                    for chunk in data.chunks(1024) {
                        for (i, &sample) in chunk.iter().enumerate() {
                            scratch[i] = i32_to_f32(sample);
                        }
                        meter.update(&scratch[..chunk.len()]);
                        let pushed = producer.push_slice(&scratch[..chunk.len()]);
                        if pushed < chunk.len() {
                            drop_cnt.fetch_add(chunk.len() - pushed, Ordering::Relaxed);
                        }
                    }
                },
                err_fn,
                None,
            )?,
            _ => return Err(RecorderError::NoSupportedConfig),
        };

        stream.play()?;
        self.stream = Some(stream);
        self.active_config = Some(config);

        Ok(())
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.stop_stream();
    }
}
