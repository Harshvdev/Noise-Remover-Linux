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
        let device = self
            .device_manager
            .default_input_device()
            .ok_or(RecorderError::NoSupportedConfig)?;
        let default_config = device.default_input_config()?;

        // Find matching device index if possible
        if let Ok(dev_desc) = device.description() {
            if let Ok(devices) = self.device_manager.list_input_devices() {
                for (idx, name) in devices {
                    if name == dev_desc.name() {
                        self.selected_device_idx = Some(idx);
                        break;
                    }
                }
            }
        }

        self.start_capture_stream(&device, default_config)?;
        Ok(())
    }

    /// Select an input device by index and start real-time monitoring (level meter).
    pub fn select_device(&mut self, index: usize) -> Result<(), RecorderError> {
        self.stop_stream();

        let device = self.device_manager.get_input_device(index)?;
        let default_config = device.default_input_config()?;
        self.selected_device_idx = Some(index);
        self.start_capture_stream(&device, default_config)?;

        Ok(())
    }

    /// Start 2-second ambient noise calibration.
    pub fn start_calibration(&self) {
        *self.mode.lock().unwrap() = RecorderMode::Calibrating;
        *self.calibration_progress.lock().unwrap() = 0.0;
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

        RecorderStatus {
            mode,
            sample_rate,
            channels,
            peak_dbfs: self.meter.peak_dbfs(),
            rms_dbfs: self.meter.rms_dbfs(),
            has_clipped: self.meter.take_clipping(),
            recorded_seconds,
            calibration_progress,
        }
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
        let config: StreamConfig = supported_config.clone().into();
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

        // Writer worker thread drains consumer and writes WAV files
        self.writer_handle = Some(thread::spawn(move || {
            let mut current_writer: Option<WavWriter<std::io::BufWriter<std::fs::File>>> = None;
            let mut active_mode = RecorderMode::Idle;
            let mut samples_in_file = 0usize;
            let calibration_target_samples = (sample_rate as f32 * 2.0) as usize;

            let mut drain_buf = vec![0.0f32; 4096];

            while is_capturing.load(Ordering::Relaxed) {
                let current_target_mode = *mode.lock().unwrap();

                // Mode transition handling
                if current_target_mode != active_mode {
                    // Close previous writer if any
                    if let Some(w) = current_writer.take() {
                        let _ = w.finalize();
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

                // Drain ring buffer
                let read = consumer.pop_slice(&mut drain_buf);
                if read > 0 {
                    // Downmix interleaved channels to mono if needed
                    let mono_samples = interleaved_to_mono(&drain_buf[..read], channels);

                    if let Some(ref mut writer) = current_writer {
                        for &s in &mono_samples {
                            let _ = writer.write_sample(s);
                        }
                        samples_in_file += mono_samples.len();

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
                                    // 2-second calibration completed automatically
                                    let mut m = mode.lock().unwrap();
                                    *m = RecorderMode::Idle;
                                }
                            }
                            RecorderMode::Idle => {}
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }

            // Finalize writer on shutdown
            if let Some(w) = current_writer.take() {
                let _ = w.finalize();
            }
        }));

        // Audio callback
        let err_fn = |err| {
            log::error!("CPAL audio input stream error: {:?}", err);
        };

        let sample_format = supported_config.sample_format();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config.clone(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    meter.update(data);
                    let _ = producer.push_slice(data);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                config.clone(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut scratch = [0.0f32; 1024];
                    for chunk in data.chunks(1024) {
                        for (i, &sample) in chunk.iter().enumerate() {
                            scratch[i] = i16_to_f32(sample);
                        }
                        meter.update(&scratch[..chunk.len()]);
                        let _ = producer.push_slice(&scratch[..chunk.len()]);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I32 => device.build_input_stream(
                config.clone(),
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    let mut scratch = [0.0f32; 1024];
                    for chunk in data.chunks(1024) {
                        for (i, &sample) in chunk.iter().enumerate() {
                            scratch[i] = i32_to_f32(sample);
                        }
                        meter.update(&scratch[..chunk.len()]);
                        let _ = producer.push_slice(&scratch[..chunk.len()]);
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
