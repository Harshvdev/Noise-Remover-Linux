//! Audio capture, ring buffer streaming, and WAV recording engine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig, SupportedStreamConfig};
use hound::{WavSpec, WavWriter};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

use crate::device::{find_best_input_config, DeviceManager};
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
    pub spectrum: Vec<f32>,
}

pub struct AudioRecorder {
    device_manager: DeviceManager,
    selected_device_idx: Option<usize>,
    stream: Option<Stream>,
    active_config: Option<StreamConfig>,
    meter: Arc<AudioMeter>,
    is_capturing: Arc<AtomicBool>,
    mode: Arc<Mutex<RecorderMode>>,
    output_dir: Arc<Mutex<PathBuf>>,
    writer_handle: Option<JoinHandle<()>>,
    recorded_seconds: Arc<Mutex<f32>>,
    calibration_progress: Arc<Mutex<f32>>,
    last_recording_path: Arc<Mutex<Option<PathBuf>>>,
    last_calibration_path: Arc<Mutex<Option<PathBuf>>>,
    dropped_samples: Arc<std::sync::atomic::AtomicUsize>,
    is_saving: Arc<AtomicBool>,
    finalized_count: Arc<std::sync::atomic::AtomicUsize>,
    last_stream_error: Arc<Mutex<Option<String>>>,
    last_capture_time_ms: Arc<AtomicU64>,
}

impl AudioRecorder {
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        let dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).ok();

        let initial_rec = dir.join("original.wav");
        let last_recording = if initial_rec.exists() {
            Some(initial_rec)
        } else {
            None
        };

        let initial_calib = dir.join("noise_reference.wav");
        let last_calib = if initial_calib.exists() {
            Some(initial_calib)
        } else {
            None
        };

        Self {
            device_manager: DeviceManager::new(),
            selected_device_idx: None,
            stream: None,
            active_config: None,
            meter: AudioMeter::new(),
            is_capturing: Arc::new(AtomicBool::new(false)),
            mode: Arc::new(Mutex::new(RecorderMode::Idle)),
            output_dir: Arc::new(Mutex::new(dir)),
            writer_handle: None,
            recorded_seconds: Arc::new(Mutex::new(0.0)),
            calibration_progress: Arc::new(Mutex::new(0.0)),
            last_recording_path: Arc::new(Mutex::new(last_recording)),
            last_calibration_path: Arc::new(Mutex::new(last_calib)),
            dropped_samples: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            is_saving: Arc::new(AtomicBool::new(false)),
            finalized_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            last_stream_error: Arc::new(Mutex::new(None)),
            last_capture_time_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn device_manager(&self) -> &DeviceManager {
        &self.device_manager
    }

    pub fn selected_device_idx(&self) -> Option<usize> {
        self.selected_device_idx
    }

    pub fn output_dir(&self) -> PathBuf {
        self.output_dir.lock().unwrap().clone()
    }

    pub fn set_output_dir<P: AsRef<Path>>(&self, dir: P) {
        let p = dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&p);
        *self.output_dir.lock().unwrap() = p;
    }

    pub fn is_stream_active(&self) -> bool {
        if self.stream.is_none() || !self.is_capturing.load(Ordering::SeqCst) {
            return false;
        }
        let last = self.last_capture_time_ms.load(Ordering::Relaxed);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if last == 0 || now_ms < last {
            return true;
        }
        // If stream hasn't received audio in over 750ms, it was disconnected/stalled
        now_ms - last < 750
    }

    /// Automatically reconnect/reinitialize the hardware stream if it disconnected
    pub fn ensure_active_stream(&mut self) -> Result<(), RecorderError> {
        if self.is_stream_active() {
            return Ok(());
        }
        log::info!("Audio capture stream is inactive or disconnected, re-initializing...");
        if let Some(idx) = self.selected_device_idx {
            self.select_device(idx)
        } else {
            self.select_default_device()
        }
    }

    /// Select default device, prioritizing Wired External > Bluetooth > Internal.
    pub fn select_default_device(&mut self) -> Result<(), RecorderError> {
        self.stop_stream();
        *self.last_stream_error.lock().unwrap() = None;

        #[cfg(target_os = "android")]
        {
            let _ = DeviceManager::select_android_default_device();
            let android_devs = DeviceManager::get_android_devices();
            if let Some(top) = android_devs.first() {
                self.selected_device_idx = Some(top.id as usize);
            }
        }

        let pw_sources = DeviceManager::get_pipewire_sources();
        if !pw_sources.is_empty() {
            // Sources are sorted by priority: Wired External (0) > Bluetooth (1) > Internal (2)
            // Index 0 is guaranteed to be the highest priority available input
            let def_idx = 0;
            self.selected_device_idx = Some(def_idx);
            let _ = self.device_manager.select_pipewire_source(def_idx);
        }

        let device = self
            .device_manager
            .default_input_device()
            .ok_or(RecorderError::NoSupportedConfig)?;
        let best_config = find_best_input_config(&device)?;

        if self.selected_device_idx.is_none() {
            if let Ok(devices) = self.device_manager.list_input_devices() {
                if let Some((idx, _)) = devices.iter().find(|(_, name)| name.starts_with("Default"))
                {
                    self.selected_device_idx = Some(*idx);
                } else if !devices.is_empty() {
                    self.selected_device_idx = Some(devices[0].0);
                }
            }
        }

        self.start_capture_stream(&device, best_config)?;
        Ok(())
    }

    /// Select an input device by index and start real-time monitoring (level meter).
    pub fn select_device(&mut self, index: usize) -> Result<(), RecorderError> {
        if self.selected_device_idx == Some(index) && self.stream.is_some() {
            // Already active and capturing on this device, fast-path return
            return Ok(());
        }

        self.stop_stream();
        *self.last_stream_error.lock().unwrap() = None;

        #[cfg(target_os = "android")]
        {
            let _ = DeviceManager::select_android_device(index as i32);
            self.selected_device_idx = Some(index);
            let device = self
                .device_manager
                .default_input_device()
                .ok_or(RecorderError::NoSupportedConfig)?;
            let best_config = find_best_input_config(&device)?;
            self.start_capture_stream(&device, best_config)?;
            return Ok(());
        }

        #[cfg(not(target_os = "android"))]
        {
            let pw_sources = DeviceManager::get_pipewire_sources();
            if !pw_sources.is_empty() {
                self.device_manager.select_pipewire_source(index)?;
                let device = self
                    .device_manager
                    .default_input_device()
                    .ok_or(RecorderError::NoSupportedConfig)?;
                let best_config = find_best_input_config(&device)?;
                self.selected_device_idx = Some(index);
                self.start_capture_stream(&device, best_config)?;
                return Ok(());
            }

            let device = self.device_manager.get_input_device(index)?;
            let best_config = find_best_input_config(&device)?;
            self.selected_device_idx = Some(index);
            self.start_capture_stream(&device, best_config)?;
            Ok(())
        }
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

    /// Get current volume percent (0-100) for the currently selected input device.
    pub fn get_selected_device_volume(&self) -> Option<u32> {
        self.selected_device_idx.and_then(|idx| self.device_manager.get_pipewire_source_volume(idx))
    }

    /// Set volume percent (0-100) for the currently selected input device.
    pub fn set_selected_device_volume(&self, volume_percent: u32) -> Result<(), RecorderError> {
        if let Some(idx) = self.selected_device_idx {
            self.device_manager.set_pipewire_source_volume(idx, volume_percent)
        } else {
            Ok(())
        }
    }

    /// Start 3-second ambient noise calibration.
    pub fn start_calibration(&self) {
        #[cfg(target_os = "android")]
        DeviceManager::notify_android_recording_started();
        *self.mode.lock().unwrap() = RecorderMode::Calibrating;
        *self.calibration_progress.lock().unwrap() = 0.0;
    }

    /// Cancel active calibration.
    pub fn cancel_calibration(&self) {
        let mut m = self.mode.lock().unwrap();
        if *m == RecorderMode::Calibrating {
            *m = RecorderMode::Idle;
            *self.calibration_progress.lock().unwrap() = 0.0;
            #[cfg(target_os = "android")]
            DeviceManager::notify_android_recording_stopped();
        }
    }

    /// Start recording voice/singing.
    pub fn start_recording(&self) {
        #[cfg(target_os = "android")]
        DeviceManager::notify_android_recording_started();
        *self.mode.lock().unwrap() = RecorderMode::Recording;
        *self.recorded_seconds.lock().unwrap() = 0.0;
    }

    /// Stop recording or calibration.
    pub fn stop_recording_or_calibration(&self) {
        *self.mode.lock().unwrap() = RecorderMode::Idle;
        #[cfg(target_os = "android")]
        DeviceManager::notify_android_recording_stopped();
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

        let spectrum = self.meter.spectrum_128(sample_rate);

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
            spectrum,
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

    pub fn set_last_recording_path<P: Into<PathBuf>>(&self, path: P) {
        *self.last_recording_path.lock().unwrap() = Some(path.into());
    }

    pub fn last_calibration_path(&self) -> Option<PathBuf> {
        self.last_calibration_path.lock().unwrap().clone()
    }

    pub fn set_last_calibration_path<P: Into<PathBuf>>(&self, path: P) {
        *self.last_calibration_path.lock().unwrap() = Some(path.into());
    }

    pub fn stop_stream(&mut self) {
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
        let now_init = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_capture_time_ms.store(now_init, Ordering::SeqCst);
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
        let last_capture_time_ms = self.last_capture_time_ms.clone();

        // Writer worker thread drains consumer and writes WAV files
        self.writer_handle = Some(thread::spawn(move || {
            let mut current_writer: Option<WavWriter<std::io::BufWriter<std::fs::File>>> = None;
            let mut active_mode = RecorderMode::Idle;
            let mut samples_in_file = 0usize;
            let calibration_target_samples = (sample_rate as f32 * 3.0) as usize;

            let mut drain_buf = vec![0.0f32; 4096];
            let mut remainder_buf = Vec::<f32>::new();

            // Helper to process interleaved samples frame-by-frame and write mono
            let process_and_write =
                |raw_samples: &[f32],
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
                            process_and_write(
                                &drain_buf[..read],
                                &mut remainder_buf,
                                &mut current_writer,
                                channels,
                            );
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
                        if active_mode == RecorderMode::Calibrating {
                            // If calibration was canceled before reaching target, discard the partial file
                            let target_dir = output_dir.lock().unwrap().clone();
                            let calib_file = target_dir.join("noise_reference.wav");
                            if samples_in_file < calibration_target_samples {
                                let _ = std::fs::remove_file(&calib_file);
                                if last_calibration_path.lock().unwrap().as_deref() == Some(&calib_file) {
                                    *last_calibration_path.lock().unwrap() = None;
                                }
                            }
                            *calibration_progress.lock().unwrap() = 0.0;
                        } else {
                            finalized_count.fetch_add(1, Ordering::SeqCst);
                        }
                    }

                    active_mode = current_target_mode;
                    samples_in_file = 0;

                    let current_output_dir = output_dir.lock().unwrap().clone();

                    match active_mode {
                        RecorderMode::Calibrating => {
                            let path = current_output_dir.join("noise_reference.wav");
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
                            let path = current_output_dir.join("original.wav");
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
                                process_and_write(
                                    &[],
                                    &mut remainder_buf,
                                    &mut current_writer,
                                    channels,
                                );
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
                process_and_write(
                    &drain_buf[..read],
                    &mut remainder_buf,
                    &mut current_writer,
                    channels,
                );
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
        let err_cap_time = last_capture_time_ms.clone();
        let err_fn = move |err| {
            log::error!("CPAL audio input stream error: {:?}", err);
            *stream_err_slot.lock().unwrap() = Some(format!("{}", err));
            cap_flag.store(false, Ordering::SeqCst);
            err_cap_time.store(0, Ordering::SeqCst);
        };

        let sample_format = supported_config.sample_format();
        let drop_cnt = dropped_samples.clone();
        let time_f32 = last_capture_time_ms.clone();
        let time_i16 = last_capture_time_ms.clone();
        let time_i32 = last_capture_time_ms.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    time_f32.store(now, Ordering::Relaxed);
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
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    time_i16.store(now, Ordering::Relaxed);
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
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    time_i32.store(now, Ordering::Relaxed);
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

        // Ensure PipeWire routes this app stream to the chosen physical microphone (not monitor loopback)
        if let Some(idx) = self.selected_device_idx {
            let sources = DeviceManager::get_pipewire_sources();
            if let Some(src) = sources.get(idx) {
                let target = if src.bluetooth_card.is_some() {
                    DeviceManager::find_active_bluez_source().unwrap_or_else(|| src.name.clone())
                } else {
                    src.name.clone()
                };
                DeviceManager::move_app_stream_to_source(&target);
            }
        }

        Ok(())
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.stop_stream();
    }
}
