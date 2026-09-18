//! Desktop GUI application for Offline Voice & Singing Noise Remover.

mod waveform;

use denoiser::{
    DeepFilterConfig, DeepFilterDenoiser, DenoiseReport, DpdfnetConfig, DpdfnetDenoiser,
    NeuralModel,
};
use dsp::{
    DenoiseDecision, DspIntensity, DspProcessingReport, DspProcessor, NoiseAnalyzer, NoiseProfile,
    ResidualLevel, ResidualReport, SignalAnalysisReport,
};
use eframe::egui::{self, Color32, ProgressBar, RichText, ScrollArea, TopBottomPanel};
use playback::AudioPlayer;
use recorder::{AudioRecorder, RecorderMode};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use waveform::WaveformRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioTab {
    Original,
    NoiseReference,
    DspCleaned,
    RemovedNoise,
    DeepCleaned,
    RemovedDeepNoise,
}

impl AudioTab {
    fn is_full_take(&self) -> bool {
        !matches!(self, AudioTab::NoiseReference)
    }
}

enum UploadWorkerMessage {
    Success {
        file_name: String,
        duration_secs: f32,
        calib_path: PathBuf,
        rec_path: PathBuf,
        noise_profile: Box<NoiseProfile>,
        signal_report: Option<SignalAnalysisReport>,
    },
    Error(String),
}

enum DspWorkerMessage {
    Success {
        result: dsp::DspProcessResult,
    },
    Error(String),
}

enum AiWorkerMessage {
    Success {
        cleaned: Vec<f32>,
        removed: Vec<f32>,
        report: DenoiseReport,
    },
    Error(String),
}

struct NoiseRemoverApp {
    recorder: AudioRecorder,
    player: AudioPlayer,
    waveform: WaveformRenderer,
    analyzer: Option<NoiseAnalyzer>,
    noise_profile: Option<NoiseProfile>,
    signal_report: Option<SignalAnalysisReport>,
    dsp_report: Option<DspProcessingReport>,
    residual_report: Option<ResidualReport>,
    denoiser_available: bool,
    selected_neural_model: NeuralModel,
    dpdfnet_available: bool,
    deepfilter_available: bool,
    denoiser_config: DpdfnetConfig,
    deepfilter_config: DeepFilterConfig,
    deepfilter_post_filter: bool,
    denoise_report: Option<DenoiseReport>,
    is_ai_processing: bool,
    ai_rx: Option<Receiver<AiWorkerMessage>>,
    is_uploading: bool,
    upload_rx: Option<Receiver<UploadWorkerMessage>>,
    is_dsp_processing: bool,
    dsp_rx: Option<Receiver<DspWorkerMessage>>,
    dsp_intensity: DspIntensity,
    dsp_preservation_report: Option<dsp::PreservationReport>,
    adaptive_preservation: bool,
    manual_preservation_alpha: f32,
    devices: Vec<(usize, String)>,
    selected_device_idx: usize,
    active_tab: AudioTab,
    status_message: String,
    clipping_highlight_frames: usize,
    smoothed_peak_dbfs: f32,
    last_finalized_count: usize,
    active_capture_mode: Option<RecorderMode>,
}

impl NoiseRemoverApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Apply dark theme aesthetics
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(226, 232, 240));
        visuals.window_fill = Color32::from_rgb(15, 23, 42);
        visuals.panel_fill = Color32::from_rgb(15, 23, 42);
        cc.egui_ctx.set_visuals(visuals);

        let output_dir = PathBuf::from("recordings");
        let mut recorder = AudioRecorder::new(&output_dir);
        let devices = recorder
            .device_manager()
            .list_input_devices()
            .unwrap_or_default();

        let mut selected_idx = devices.first().map(|(i, _)| *i).unwrap_or(0);
        let mut status = "Select a microphone to begin monitoring.".to_string();

        if let Ok(()) = recorder.select_default_device() {
            if let Some(idx) = recorder.selected_device_idx() {
                selected_idx = idx;
            }
            if let Some((_, name)) = devices.iter().find(|(i, _)| *i == selected_idx) {
                status = format!("Monitoring: {}", name);
            }
        } else if !devices.is_empty() {
            selected_idx = devices[0].0;
            if let Err(e) = recorder.select_device(selected_idx) {
                status = format!("Failed to open microphone: {}", e);
            } else {
                status = format!("Monitoring: {}", devices[0].1);
            }
        }

        let mut app = Self {
            recorder,
            player: AudioPlayer::new(),
            waveform: WaveformRenderer::new(),
            analyzer: NoiseAnalyzer::new(48000).ok(),
            noise_profile: None,
            signal_report: None,
            dsp_report: None,
            residual_report: None,
            denoiser_available: false,
            selected_neural_model: NeuralModel::Dpdfnet2_48k,
            dpdfnet_available: false,
            deepfilter_available: false,
            denoiser_config: DpdfnetConfig::default(),
            deepfilter_config: DeepFilterConfig::default(),
            deepfilter_post_filter: false,
            denoise_report: None,
            is_ai_processing: false,
            ai_rx: None,
            is_uploading: false,
            upload_rx: None,
            is_dsp_processing: false,
            dsp_rx: None,
            dsp_intensity: DspIntensity::Balanced,
            dsp_preservation_report: None,
            adaptive_preservation: true,
            manual_preservation_alpha: 0.85,
            devices,
            selected_device_idx: selected_idx,
            active_tab: AudioTab::Original,
            status_message: status,
            clipping_highlight_frames: 0,
            smoothed_peak_dbfs: -96.0,
            last_finalized_count: 0,
            active_capture_mode: None,
        };

        // Initialize neural denoiser backends (DPDFNet2 & DeepFilterNet3)
        let dpdf_cfg = DpdfnetConfig::default();
        if dpdf_cfg.model_path.exists() {
            match DpdfnetDenoiser::new(dpdf_cfg) {
                Ok(_) => {
                    log::info!("[INFO] DPDFNet2-48k neural denoiser initialized successfully");
                    app.dpdfnet_available = true;
                }
                Err(e) => {
                    log::warn!("[WARN] DPDFNet2 neural denoiser unavailable: {}", e);
                }
            }
        }

        let df_cfg = DeepFilterConfig::default();
        if df_cfg.binary_path.exists() {
            match DeepFilterDenoiser::new(df_cfg) {
                Ok(_) => {
                    log::info!("[INFO] DeepFilterNet3 neural denoiser initialized successfully");
                    app.deepfilter_available = true;
                }
                Err(e) => {
                    log::warn!("[WARN] DeepFilterNet3 neural denoiser unavailable: {}", e);
                }
            }
        }

        app.denoiser_available = app.dpdfnet_available || app.deepfilter_available;
        if app.dpdfnet_available {
            app.selected_neural_model = NeuralModel::Dpdfnet2_48k;
        } else if app.deepfilter_available {
            app.selected_neural_model = NeuralModel::DeepFilterNet3;
        }

        app.run_dsp_analysis();
        app.reload_active_audio();
        app
    }

    fn run_dsp_analysis(&mut self) {
        let analyzer = match self.analyzer.as_ref() {
            Some(a) => a,
            None => return,
        };

        // 1. Analyze noise reference if available
        if let Some(calib_path) = self.recorder.last_calibration_path() {
            if calib_path.exists() {
                if let Ok((samples, _)) = audio_core::read_wav_canonical_f32(&calib_path) {
                    if let Ok(profile) = analyzer.analyze_noise_reference(&samples) {
                        log::info!(
                            "[INFO] Noise stationarity: {:.2}",
                            profile.stationarity_score
                        );
                        log::info!(
                            "[INFO] Estimated noise floor: {:.1} dBFS",
                            profile.noise_floor_dbfs
                        );
                        for peak in &profile.tonal_peaks {
                            log::info!(
                                "[INFO] Tonal peak: {:.1} Hz (prominence: {:.1} dB, conf: {:.2})",
                                peak.frequency_hz,
                                peak.strength_db,
                                peak.confidence
                            );
                        }
                        self.noise_profile = Some(profile);
                    }
                }
            }
        }

        // 2. Analyze speech recording if available
        if let Some(ref profile) = self.noise_profile {
            if let Some(rec_path) = self.recorder.last_recording_path() {
                if rec_path.exists() {
                    if let Ok((samples, _)) = audio_core::read_wav_canonical_f32(&rec_path) {
                        if let Ok(report) = analyzer.analyze_signal(&samples, profile) {
                            log::info!(
                                "[INFO] Vocal activity confidence: {:.2}, SNR: {:.1} dB",
                                report.activity.overall_confidence,
                                report.activity.vocal_snr_db
                            );
                            self.signal_report = Some(report);
                        }
                    }
                }
            }
        }

        // 3. Analyze post-DSP residual noise if dsp_cleaned.wav exists
        if let Some(ref profile) = self.noise_profile {
            let cleaned_path = self.dsp_cleaned_path();
            if cleaned_path.exists() {
                if let Ok((cleaned_samples, _)) = audio_core::read_wav_canonical_f32(&cleaned_path)
                {
                    let activity = self.signal_report.as_ref().map(|r| &r.activity);
                    if let Ok(residual) =
                        dsp::analyze_residual(&cleaned_samples, profile, activity, 48000)
                    {
                        log::info!("[INFO] DSP residual: {}", residual.level.as_str());
                        log::info!("[INFO] DSP decision: {}", residual.decision.as_str());
                        self.residual_report = Some(residual);
                    }
                }
            }
        }
    }

    fn dsp_cleaned_path(&self) -> PathBuf {
        PathBuf::from("recordings/dsp_cleaned.wav")
    }

    fn removed_noise_path(&self) -> PathBuf {
        PathBuf::from("recordings/removed_noise.wav")
    }

    fn deep_cleaned_path(&self) -> PathBuf {
        PathBuf::from("recordings/deep_cleaned.wav")
    }

    fn removed_deep_noise_path(&self) -> PathBuf {
        PathBuf::from("recordings/removed_deep_noise.wav")
    }

    fn run_dsp_cleaning(&mut self, ctx: &egui::Context) {
        if self.is_dsp_processing {
            return;
        }

        let profile = match self.noise_profile.as_ref() {
            Some(p) => p.clone(),
            None => {
                self.status_message = "Please calibrate noise reference first.".into();
                return;
            }
        };

        let rec_path = match self.recorder.last_recording_path() {
            Some(p) if p.exists() => p.clone(),
            _ => {
                self.status_message = "No voice recording found. Record voice first!".into();
                return;
            }
        };

        let (samples, _) = match audio_core::read_wav_canonical_f32(&rec_path) {
            Ok(s) => s,
            Err(e) => {
                self.status_message = format!("Failed to read recording: {}", e);
                return;
            }
        };

        let mut config = self.dsp_intensity.to_config();
        if !self.adaptive_preservation {
            config.preservation = dsp::PreservationMode::Global(self.manual_preservation_alpha);
        }
        let activity = self.signal_report.as_ref().map(|r| r.activity.clone());

        self.is_dsp_processing = true;
        self.status_message = "⚡ Running classical DSP cleaning & harmonic preservation...".into();

        let (tx, rx) = channel();
        self.dsp_rx = Some(rx);

        let cleaned_path = self.dsp_cleaned_path();
        let noise_path = self.removed_noise_path();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let processor = match DspProcessor::new(48000) {
                Ok(p) => p,
                Err(e) => {
                    tx.send(DspWorkerMessage::Error(format!("DSP processor error: {}", e))).ok();
                    ctx_clone.request_repaint();
                    return;
                }
            };

            match processor.process(&samples, &profile, activity.as_ref(), &config) {
                Ok(result) => {
                    if let Err(e) = audio_core::write_wav_f32(&cleaned_path, &result.cleaned_samples, 48000, 1) {
                        tx.send(DspWorkerMessage::Error(format!("Failed to write dsp_cleaned.wav: {}", e))).ok();
                    } else if let Err(e) = audio_core::write_wav_f32(&noise_path, &result.removed_noise_samples, 48000, 1) {
                        tx.send(DspWorkerMessage::Error(format!("Failed to write removed_noise.wav: {}", e))).ok();
                    } else {
                        tx.send(DspWorkerMessage::Success { result }).ok();
                    }
                }
                Err(e) => {
                    tx.send(DspWorkerMessage::Error(format!("DSP processing failed: {}", e))).ok();
                }
            }
            ctx_clone.request_repaint();
        });
    }

    fn run_deep_cleaning(&mut self) {
        if !self.denoiser_available {
            self.status_message = "No neural denoiser backend available (models missing).".into();
            return;
        }

        match self.selected_neural_model {
            NeuralModel::Dpdfnet2_48k if !self.dpdfnet_available => {
                self.status_message = "DPDFNet2 model not loaded (weights missing).".into();
                return;
            }
            NeuralModel::DeepFilterNet3 if !self.deepfilter_available => {
                self.status_message = "DeepFilterNet3 binary not loaded (missing).".into();
                return;
            }
            _ => {}
        }

        if self.is_ai_processing {
            return;
        }

        // Prefer dsp_cleaned.wav if available, otherwise fallback to original.wav
        let input_path = if self.dsp_cleaned_path().exists() {
            self.dsp_cleaned_path()
        } else if let Some(rec_path) = self.recorder.last_recording_path() {
            if rec_path.exists() {
                rec_path
            } else {
                self.status_message = "No recording found to clean.".into();
                return;
            }
        } else {
            self.status_message = "No recording found to clean.".into();
            return;
        };

        let (samples, spec) = match audio_core::read_wav_canonical_f32(&input_path) {
            Ok(s) => s,
            Err(e) => {
                self.status_message = format!("Failed to read input audio: {}", e);
                return;
            }
        };

        if spec.sample_rate != 48000 {
            self.status_message = format!("Input must be 48 kHz (found {} Hz).", spec.sample_rate);
            return;
        }

        self.is_ai_processing = true;
        let model_tag = self.selected_neural_model.display_name();
        self.status_message = format!("🧠 Neural Deep Clean running in background ({model_tag})...");

        let (tx, rx) = channel();
        self.ai_rx = Some(rx);

        let selected_model = self.selected_neural_model;
        let mut dpdf_config = self.denoiser_config.clone();
        if !self.adaptive_preservation {
            dpdf_config.preservation = dsp::PreservationMode::Global(self.manual_preservation_alpha);
        }

        let mut df_config = self.deepfilter_config.clone();
        df_config.post_filter = self.deepfilter_post_filter;
        if !self.adaptive_preservation {
            df_config.preservation = dsp::PreservationMode::Global(self.manual_preservation_alpha);
        }

        let activity = self.signal_report.as_ref().map(|r| r.activity.clone());
        std::thread::spawn(move || {
            let res = match selected_model {
                NeuralModel::Dpdfnet2_48k => {
                    match DpdfnetDenoiser::new(dpdf_config) {
                        Ok(mut denoiser) => denoiser.denoise_with_activity(&samples, activity.as_ref()),
                        Err(e) => Err(e),
                    }
                }
                NeuralModel::DeepFilterNet3 => {
                    match DeepFilterDenoiser::new(df_config) {
                        Ok(mut denoiser) => denoiser.denoise_with_activity(&samples, activity.as_ref()),
                        Err(e) => Err(e),
                    }
                }
            };

            match res {
                Ok((cleaned, removed, report)) => {
                    let _ = tx.send(AiWorkerMessage::Success {
                        cleaned,
                        removed,
                        report,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AiWorkerMessage::Error(e.to_string()));
                }
            }
        });
    }

    fn reload_active_audio(&mut self) {
        self.reload_active_audio_seamless(false);
    }

    fn reload_active_audio_seamless(&mut self, preserve_position: bool) {
        let path = match self.active_tab {
            AudioTab::Original => self.recorder.last_recording_path(),
            AudioTab::NoiseReference => self.recorder.last_calibration_path(),
            AudioTab::DspCleaned => Some(self.dsp_cleaned_path()),
            AudioTab::RemovedNoise => Some(self.removed_noise_path()),
            AudioTab::DeepCleaned => Some(self.deep_cleaned_path()),
            AudioTab::RemovedDeepNoise => Some(self.removed_deep_noise_path()),
        };

        if let Some(p) = path {
            if p.exists() {
                let res = if preserve_position {
                    self.player.load_file_preserve_position(&p, true)
                } else {
                    self.player.load_file(&p)
                };
                if let Err(e) = res {
                    self.status_message = format!("Playback error: {}", e);
                } else {
                    self.waveform.update(self.player.samples(), 600);
                }
            }
        }
    }

    fn start_upload_file_dialog(&mut self, ctx: &egui::Context) {
        if self.is_uploading {
            return;
        }
        self.player.stop();
        self.is_uploading = true;
        self.status_message = "Opening file chooser...".into();

        let (tx, rx) = channel();
        self.upload_rx = Some(rx);

        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let file_opt = rfd::FileDialog::new()
                .add_filter(
                    "Audio Files",
                    &[
                        "wav", "mp3", "flac", "ogg", "m4a", "aac", "opus", "WAV", "MP3",
                        "FLAC", "OGG", "M4A",
                    ],
                )
                .set_title("Select Audio File (First 2s = Noise Reference)")
                .pick_file();

            if let Some(path) = file_opt {
                Self::process_uploaded_file_worker(path, tx);
            } else {
                tx.send(UploadWorkerMessage::Error("File selection cancelled.".into()))
                    .ok();
            }
            ctx_clone.request_repaint();
        });
    }

    fn start_async_upload_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.is_uploading {
            return;
        }
        self.player.stop();
        self.is_uploading = true;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "audio file".into());
        self.status_message = format!("Loading '{}'...", file_name);

        let (tx, rx) = channel();
        self.upload_rx = Some(rx);

        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            Self::process_uploaded_file_worker(path, tx);
            ctx_clone.request_repaint();
        });
    }

    fn process_uploaded_file_worker(path: PathBuf, tx: Sender<UploadWorkerMessage>) {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "audio file".into());

        // 1. Load canonical 48kHz mono float samples
        let samples = match audio_core::load_audio_canonical_48k(&path) {
            Ok(s) => s,
            Err(e) => {
                tx.send(UploadWorkerMessage::Error(format!("Failed to load audio: {}", e)))
                    .ok();
                return;
            }
        };

        // 2. Validate minimum duration: at least 2.0s (96,000 samples)
        let min_samples = 48000 * 2;
        if samples.len() < min_samples {
            let dur = samples.len() as f32 / 48000.0;
            tx.send(UploadWorkerMessage::Error(format!(
                "Uploaded audio is too short ({:.1}s). It must be at least 2.0s to extract noise reference.",
                dur
            )))
            .ok();
            return;
        }

        // 3. Extract first 2 seconds for noise calibration reference
        let noise_samples = &samples[..min_samples];
        let calib_path = PathBuf::from("recordings/noise_reference.wav");
        if let Err(e) = audio_core::write_wav_f32(&calib_path, noise_samples, 48000, 1) {
            tx.send(UploadWorkerMessage::Error(format!("Failed to save noise reference: {}", e)))
                .ok();
            return;
        }

        // 4. Save entire audio take as original.wav
        let rec_path = PathBuf::from("recordings/original.wav");
        if let Err(e) = audio_core::write_wav_f32(&rec_path, &samples, 48000, 1) {
            tx.send(UploadWorkerMessage::Error(format!("Failed to save original recording: {}", e)))
                .ok();
            return;
        }

        // 5. Run DSP analysis (noise profile + vocal activity)
        let analyzer = match NoiseAnalyzer::new(48000) {
            Ok(a) => a,
            Err(e) => {
                tx.send(UploadWorkerMessage::Error(format!("Analyzer error: {}", e)))
                    .ok();
                return;
            }
        };

        let profile = match analyzer.analyze_noise_reference(noise_samples) {
            Ok(p) => p,
            Err(e) => {
                tx.send(UploadWorkerMessage::Error(format!("Noise profile analysis failed: {}", e)))
                    .ok();
                return;
            }
        };

        let signal_report = analyzer.analyze_signal(&samples, &profile).ok();
        let duration_secs = samples.len() as f32 / 48000.0;

        tx.send(UploadWorkerMessage::Success {
            file_name,
            duration_secs,
            calib_path,
            rec_path,
            noise_profile: Box::new(profile),
            signal_report,
        })
        .ok();
    }
}

impl eframe::App for NoiseRemoverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for drag-and-drop audio files dropped onto the application window
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() && !self.is_uploading && !self.is_dsp_processing && !self.is_ai_processing {
            for file in dropped_files {
                if let Some(path) = file.path {
                    self.start_async_upload_path(path, ctx);
                    break;
                }
            }
        }

        // Poll for asynchronous audio file upload completion
        if let Some(ref rx) = self.upload_rx {
            match rx.try_recv() {
                Ok(UploadWorkerMessage::Success {
                    file_name,
                    duration_secs,
                    calib_path,
                    rec_path,
                    noise_profile,
                    signal_report,
                }) => {
                    self.recorder.set_last_calibration_path(&calib_path);
                    self.recorder.set_last_recording_path(&rec_path);

                    // Invalidate previous DSP and AI results
                    self.dsp_report = None;
                    self.residual_report = None;
                    self.denoise_report = None;
                    self.dsp_preservation_report = None;
                    self.waveform.clear();
                    let _ = std::fs::remove_file(self.dsp_cleaned_path());
                    let _ = std::fs::remove_file(self.removed_noise_path());
                    let _ = std::fs::remove_file(self.deep_cleaned_path());
                    let _ = std::fs::remove_file(self.removed_deep_noise_path());

                    self.noise_profile = Some(*noise_profile);
                    self.signal_report = signal_report;

                    self.active_tab = AudioTab::Original;
                    self.reload_active_audio();

                    self.status_message = format!(
                        "Uploaded '{}' ({:.1}s). First 2.0s calibrated as noise reference. Ready to clean!",
                        file_name, duration_secs
                    );
                    self.is_uploading = false;
                    self.upload_rx = None;
                }
                Ok(UploadWorkerMessage::Error(e)) => {
                    self.status_message = format!("Upload failed: {}", e);
                    self.is_uploading = false;
                    self.upload_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.is_uploading = false;
                    self.upload_rx = None;
                }
            }
        }

        // Poll for asynchronous DSP cleaning completion
        if let Some(ref rx) = self.dsp_rx {
            match rx.try_recv() {
                Ok(DspWorkerMessage::Success { result }) => {
                    // Invalidate subsequent Phase 6 deep clean results
                    self.denoise_report = None;
                    let _ = std::fs::remove_file(self.deep_cleaned_path());
                    let _ = std::fs::remove_file(self.removed_deep_noise_path());

                    let att = result.report.attenuation_db;
                    let ms = result.report.processing_time_ms;
                    let notches = result.report.notched_frequencies.clone();
                    let res_level = result.residual.level;
                    let res_decision = result.residual.decision;

                    self.dsp_report = Some(result.report);
                    self.residual_report = Some(result.residual);
                    self.dsp_preservation_report = Some(result.preservation);
                    self.active_tab = AudioTab::DspCleaned;
                    self.reload_active_audio_seamless(true);

                    let notch_msg = if notches.is_empty() {
                        String::new()
                    } else {
                        let freqs_str: Vec<String> =
                            notches.iter().map(|f| format!("{:.1} Hz", f)).collect();
                        format!(" (Hum notched: {})", freqs_str.join(", "))
                    };
                    self.status_message = format!(
                        "DSP cleaning complete: {:.1} dB noise reduction in {:.0} ms! Residual: {} -> {}{}",
                        att, ms, res_level.as_str(), res_decision.as_str(), notch_msg
                    );
                    self.is_dsp_processing = false;
                    self.dsp_rx = None;
                }
                Ok(DspWorkerMessage::Error(e)) => {
                    self.status_message = e;
                    self.is_dsp_processing = false;
                    self.dsp_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.is_dsp_processing = false;
                    self.dsp_rx = None;
                }
            }
        }

        // Poll for asynchronous AI deep clean completion
        if let Some(ref rx) = self.ai_rx {
            match rx.try_recv() {
                Ok(AiWorkerMessage::Success {
                    cleaned,
                    removed,
                    report,
                }) => {
                    let deep_path = self.deep_cleaned_path();
                    let noise_path = self.removed_deep_noise_path();

                    if let Err(e) = audio_core::write_wav_f32(&deep_path, &cleaned, 48000, 1) {
                        self.status_message = format!("Failed to write deep_cleaned.wav: {}", e);
                    } else if let Err(e) =
                        audio_core::write_wav_f32(&noise_path, &removed, 48000, 1)
                    {
                        self.status_message =
                            format!("Failed to write removed_deep_noise.wav: {}", e);
                    } else {
                        self.status_message = format!(
                            "🧠 Neural Deep Clean complete: {:.1} dB attenuation in {:.0} ms (RTF: {:.3})!",
                            report.attenuation_db, report.inference_time_ms, report.rtf
                        );
                        self.denoise_report = Some(report);
                        self.active_tab = AudioTab::DeepCleaned;
                        self.reload_active_audio_seamless(true);
                    }
                    self.is_ai_processing = false;
                    self.ai_rx = None;
                }
                Ok(AiWorkerMessage::Error(e)) => {
                    self.status_message = format!("Neural denoising failed: {}", e);
                    self.is_ai_processing = false;
                    self.ai_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still processing in background thread
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status_message =
                        "Neural denoiser worker disconnected unexpectedly.".into();
                    self.is_ai_processing = false;
                    self.ai_rx = None;
                }
            }
        }

        let status = self.recorder.status();

        if status.mode != RecorderMode::Idle {
            self.active_capture_mode = Some(status.mode);
        }

        // Check if recording or calibration finished writing to disk asynchronously
        let cur_finalized = self.recorder.finalized_count();
        if cur_finalized > self.last_finalized_count {
            self.last_finalized_count = cur_finalized;
            let finished_mode = self.active_capture_mode.take();
            self.waveform.clear();
            if finished_mode == Some(RecorderMode::Calibrating) {
                // Invalidate stale downstream analysis and processed audio files
                self.signal_report = None;
                self.dsp_report = None;
                self.residual_report = None;
                self.denoise_report = None;
                self.dsp_preservation_report = None;
                let _ = std::fs::remove_file(self.dsp_cleaned_path());
                let _ = std::fs::remove_file(self.removed_noise_path());
                let _ = std::fs::remove_file(self.deep_cleaned_path());
                let _ = std::fs::remove_file(self.removed_deep_noise_path());

                self.status_message =
                    "Noise reference calibrated (2s). Ready to record voice!".into();
                self.run_dsp_analysis();
                // Keep active_tab as Original (or reload reference only if user is on that tab)
                if self.active_tab == AudioTab::NoiseReference {
                    self.reload_active_audio();
                }
            } else {
                // Invalidate stale DSP and AI results for the new vocal take
                self.dsp_report = None;
                self.residual_report = None;
                self.denoise_report = None;
                self.dsp_preservation_report = None;
                let _ = std::fs::remove_file(self.dsp_cleaned_path());
                let _ = std::fs::remove_file(self.removed_noise_path());
                let _ = std::fs::remove_file(self.deep_cleaned_path());
                let _ = std::fs::remove_file(self.removed_deep_noise_path());

                self.active_tab = AudioTab::Original;
                self.run_dsp_analysis();
                self.reload_active_audio();
                self.status_message = "Recording saved to original.wav and loaded.".into();
            }
        }

        // Request continuous repaint while recording, calibrating, saving, playing, uploading, or running AI/DSP inference
        if status.mode != RecorderMode::Idle
            || status.is_saving
            || self.player.is_playing()
            || self.is_ai_processing
            || self.is_dsp_processing
            || self.is_uploading
        {
            ctx.request_repaint();
        } else {
            // Still update periodically for meters
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }

        if status.has_clipped {
            self.clipping_highlight_frames = 20;
        } else if self.clipping_highlight_frames > 0 {
            self.clipping_highlight_frames -= 1;
        }

        // Pinned status bar at the bottom of the window
        TopBottomPanel::bottom("bottom_status_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&self.status_message)
                        .color(Color32::from_rgb(148, 163, 184))
                        .italics(),
                );

                if status.dropped_samples > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("⚠ Dropped samples: {}", status.dropped_samples))
                                .color(Color32::from_rgb(234, 179, 8))
                                .strong(),
                        );
                    });
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Top Header
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.heading(
                            RichText::new("🎙 Voice & Singing Noise Remover")
                                .size(22.0)
                                .strong()
                                .color(Color32::from_rgb(56, 189, 248)),
                        );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    let max_label = if is_max { "🗗 Restore" } else { "🗖 Maximize" };
                    if ui.small_button(max_label).on_hover_text("Toggle window maximize / restore").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                    }
                    ui.label(
                        RichText::new("Offline · CPU-first · 48 kHz")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                });
            });
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(8.0);

            // Top Stream Error Banner if stream disconnected or failed
            if let Some(ref err) = status.last_error {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("⚠ Audio Stream Error:")
                                .color(Color32::from_rgb(239, 68, 68))
                                .strong(),
                        );
                        ui.label(RichText::new(err).color(Color32::from_rgb(248, 113, 113)));
                    });
                });
                ui.add_space(4.0);
            }

            // Device Selection Section (disabled while actively capturing or saving)
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Input Device:").strong());
                    let prev_idx = self.selected_device_idx;
                    ui.add_enabled_ui(status.mode == RecorderMode::Idle && !status.is_saving, |ui| {
                        egui::ComboBox::from_id_salt("device_select")
                            .width(360.0)
                            .selected_text(
                                self.devices
                                    .iter()
                                    .find(|(i, _)| *i == self.selected_device_idx)
                                    .map(|(_, n)| n.as_str())
                                    .unwrap_or("No device found"),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, name) in &self.devices {
                                    ui.selectable_value(&mut self.selected_device_idx, *idx, name);
                                }
                            });

                        if ui.button("🔄").on_hover_text("Refresh audio devices").clicked() {
                            if let Ok(updated) = self.recorder.device_manager().list_input_devices() {
                                self.devices = updated;
                                if let Ok(()) = self.recorder.select_default_device() {
                                    if let Some(idx) = self.recorder.selected_device_idx() {
                                        self.selected_device_idx = idx;
                                    }
                                }
                            }
                        }
                    });

                    if self.selected_device_idx != prev_idx {
                        if let Err(e) = self.recorder.select_device(self.selected_device_idx) {
                            self.status_message = format!("Error switching device: {}", e);
                        } else {
                            self.status_message = "Device switched successfully.".into();
                        }
                    }

                    if status.sample_rate > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{} Hz · {} ch",
                                status.sample_rate, status.channels
                            ))
                            .color(Color32::from_rgb(148, 163, 184)),
                        );
                    }
                });
            });

            ui.add_space(8.0);

            // Mute Warning Banner if microphone is muted in Linux sound settings
            if self.recorder.is_selected_device_muted() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("🔇 Microphone is MUTED in Linux system settings!")
                                .color(Color32::from_rgb(239, 68, 68))
                                .strong(),
                        );
                        if ui.button(
                            RichText::new("🔊 Unmute Microphone")
                                .color(Color32::WHITE)
                                .background_color(Color32::from_rgb(37, 99, 235)),
                        ).clicked() {
                            let _ = self.recorder.unmute_selected_device();
                            self.status_message = "Microphone unmuted in system settings.".into();
                        }
                    });
                });
                ui.add_space(4.0);
            }

            // Level Meter & Clipping Indicator
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎤 Live Microphone Input").strong());
                    ui.label(
                        RichText::new("(monitors ambient room sound from selected mic)")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );

                    // Mic Input Gain slider
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(vol) = self.recorder.get_selected_device_volume() {
                            let mut vol_val = vol as f32;
                            ui.label("%");
                            let slider = egui::Slider::new(&mut vol_val, 5.0..=100.0)
                                .text("Gain");
                            let resp = ui.add(slider);
                            if resp.changed() {
                                let _ = self.recorder.set_selected_device_volume(vol_val as u32);
                            }
                        }
                    });
                });
                ui.add_space(2.0);

                // Decay ballistics: instant rise, smooth fall
                let raw_peak = status.peak_dbfs;
                if raw_peak > self.smoothed_peak_dbfs {
                    self.smoothed_peak_dbfs = raw_peak;
                } else {
                    self.smoothed_peak_dbfs = (self.smoothed_peak_dbfs - 0.8).max(raw_peak).max(-96.0);
                }

                ui.horizontal(|ui| {
                    ui.label("Input:");
                    // Scale: -50 dBFS (silence) to 0 dBFS (max)
                    let peak_fraction = ((self.smoothed_peak_dbfs + 50.0) / 50.0).clamp(0.0, 1.0);
                    let bar_color = if self.smoothed_peak_dbfs > -3.0 {
                        Color32::from_rgb(239, 68, 68) // Red
                    } else if self.smoothed_peak_dbfs > -12.0 {
                        Color32::from_rgb(234, 179, 8) // Yellow
                    } else {
                        Color32::from_rgb(34, 197, 94) // Green
                    };

                    let progress_bar = ProgressBar::new(peak_fraction)
                        .desired_width(ui.available_width() - 170.0)
                        .fill(bar_color);
                    ui.add(progress_bar);

                    let readout = if self.smoothed_peak_dbfs <= -48.0 {
                        "Quiet".to_string()
                    } else {
                        format!("{:.1} dBFS", self.smoothed_peak_dbfs)
                    };
                    ui.label(readout);

                    if self.clipping_highlight_frames > 0 {
                        ui.label(
                            RichText::new(" CLIP ")
                                .background_color(Color32::from_rgb(220, 38, 38))
                                .color(Color32::WHITE)
                                .strong(),
                        );
                    }
                });

                // Auto-Leveling banner if input is clipping from excessive analog hardware boost
                if self.clipping_highlight_frames > 0 && self.smoothed_peak_dbfs > -1.0 {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("⚠️ Hardware input is clipping! Linux mic gain is over-amplifying.")
                                .small()
                                .color(Color32::from_rgb(239, 68, 68)),
                        );
                        if ui.button(
                            RichText::new("Auto-Level to Safe Gain (25%)")
                                .small()
                                .color(Color32::WHITE)
                                .background_color(Color32::from_rgb(37, 99, 235)),
                        ).clicked() {
                            let _ = self.recorder.set_selected_device_volume(25);
                            self.status_message = "Microphone gain leveled to clean 25%.".into();
                        }
                    });
                }
            });

            let is_hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
            if is_hovering_file {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("📥 Drop audio file here to upload (first 2s will be calibrated as noise)")
                                .color(Color32::from_rgb(56, 189, 248))
                                .strong(),
                        );
                    });
                });
                ui.add_space(4.0);
            }

            // Actions & Recording Controls
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let is_calibrating = status.mode == RecorderMode::Calibrating;
                    let is_recording = status.mode == RecorderMode::Recording;

                    // Calibration Button
                    let calib_btn = ui.add_enabled(
                        !is_calibrating && !is_recording && !status.is_saving,
                        egui::Button::new(RichText::new("🎯 1. Calibrate Noise (2s)").size(15.0)),
                    );

                    if calib_btn.clicked() {
                        self.recorder.start_calibration();
                        self.status_message =
                            "Calibrating: please remain silent for 2 seconds...".into();
                    }

                    if is_calibrating {
                        let cancel_btn = ui.button(RichText::new("❌ Cancel").size(15.0));
                        if cancel_btn.clicked() {
                            self.recorder.cancel_calibration();
                            self.active_capture_mode = None;
                            self.status_message = "Calibration canceled.".into();
                        }
                    }

                    // Record / Stop Button
                    if !is_recording {
                        let rec_btn = ui.add_enabled(
                            !is_calibrating && !status.is_saving,
                            egui::Button::new(
                                RichText::new("⏺ 2. Start Recording")
                                    .size(15.0)
                                    .color(Color32::from_rgb(239, 68, 68)),
                            ),
                        );
                        if rec_btn.clicked() {
                            self.recorder.start_recording();
                            self.status_message = "Recording voice/singing...".into();
                        }
                    } else {
                        let stop_btn = ui.button(
                            RichText::new("⏹ Stop Recording")
                                .size(15.0)
                                .color(Color32::WHITE)
                                .background_color(Color32::from_rgb(220, 38, 38)),
                        );
                        if stop_btn.clicked() {
                            self.recorder.stop_recording_or_calibration();
                            self.status_message = "Finalizing recording...".into();
                        }
                    }

                    ui.separator();

                    // Upload Pre-Recorded Audio Button
                    let can_upload = !is_recording
                        && !is_calibrating
                        && !status.is_saving
                        && !self.is_ai_processing
                        && !self.is_dsp_processing
                        && !self.is_uploading;

                    let upload_btn = ui.add_enabled(
                        can_upload,
                        egui::Button::new(
                            RichText::new("📁 Upload Audio")
                                .size(15.0)
                                .color(if can_upload {
                                    Color32::from_rgb(56, 189, 248)
                                } else {
                                    Color32::from_rgb(100, 116, 139)
                                }),
                        ),
                    ).on_hover_text("Upload an audio file (WAV, MP3, FLAC, OGG, M4A, etc.). The first 2 seconds are calibrated as background noise reference.");

                    if upload_btn.clicked() {
                        self.start_upload_file_dialog(ctx);
                    }

                    if self.is_uploading {
                        ui.spinner();
                        ui.label(
                            RichText::new("📁 Loading audio...")
                                .color(Color32::from_rgb(56, 189, 248))
                                .strong(),
                        );
                    }

                    ui.separator();

                    // 3. DSP Noise Removal Button
                    let can_clean = !is_recording
                        && !is_calibrating
                        && !status.is_saving
                        && !self.is_uploading
                        && !self.is_dsp_processing
                        && !self.is_ai_processing
                        && self.noise_profile.is_some()
                        && self
                            .recorder
                            .last_recording_path()
                            .map(|p| p.exists())
                            .unwrap_or(false);

                    let clean_btn = ui.add_enabled(
                        can_clean,
                        egui::Button::new(
                            RichText::new(if self.is_dsp_processing {
                                "⚡ Cleaning..."
                            } else {
                                "⚡ 3. Clean Noise (DSP)"
                            })
                            .size(15.0)
                            .color(if can_clean {
                                Color32::from_rgb(52, 211, 153)
                            } else {
                                Color32::from_rgb(100, 116, 139)
                            }),
                        ),
                    );
                    if clean_btn.clicked() {
                        self.run_dsp_cleaning(ctx);
                    }

                    if self.is_dsp_processing {
                        ui.spinner();
                        ui.label(
                            RichText::new("⚡ DSP Cleaning...")
                                .color(Color32::from_rgb(52, 211, 153))
                                .strong(),
                        );
                    }

                    // DSP Intensity Preset Selector
                    egui::ComboBox::from_id_salt("dsp_intensity_combo")
                        .selected_text(self.dsp_intensity.as_str())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.dsp_intensity, DspIntensity::Gentle, DspIntensity::Gentle.as_str());
                            ui.selectable_value(&mut self.dsp_intensity, DspIntensity::Balanced, DspIntensity::Balanced.as_str());
                            ui.selectable_value(&mut self.dsp_intensity, DspIntensity::Aggressive, DspIntensity::Aggressive.as_str());
                        });

                    ui.separator();

                    // Phase 7 Preservation Layer Controls
                    ui.checkbox(&mut self.adaptive_preservation, "🛡 Vocal Safe")
                        .on_hover_text("Phase 7 Preservation Layer: Protects vocal harmonics and subtle timbre, preventing comb-filtering and phasing artifacts.");

                    if !self.adaptive_preservation {
                        ui.add(
                            egui::Slider::new(&mut self.manual_preservation_alpha, 0.0..=1.0)
                                .text("Alpha")
                                .show_value(true),
                        )
                        .on_hover_text("Manual preservation blend: 0.0 = 100% Original, 1.0 = 100% Processed");
                    }

                    // 4. Phase 6 & 8 AI Deep Clean
                    let model_label = match self.selected_neural_model {
                        NeuralModel::Dpdfnet2_48k => "DPDFNet2",
                        NeuralModel::DeepFilterNet3 => "DeepFilter3",
                    };
                    let (ai_btn_text, ai_tooltip) = match self.residual_report.as_ref().map(|r| r.decision) {
                        Some(DenoiseDecision::FinishWithoutAi) => (
                            format!("🧠 4. Deep Clean ({model_label} Optional)"),
                            "Residual noise is already low. Classical DSP was sufficient, but neural deep clean is available.".to_string(),
                        ),
                        Some(DenoiseDecision::InvokeNeuralBackend) => (
                            format!("🧠 4. Deep Clean ({model_label} Recommended)"),
                            format!("Residual noise detected. Click to run full-band 48 kHz {} neural enhancement.", self.selected_neural_model.display_name()),
                        ),
                        None => (
                            format!("🧠 4. Deep Clean ({model_label})"),
                            format!("Click to run full-band 48 kHz {} neural enhancement on vocal audio.", self.selected_neural_model.display_name()),
                        ),
                    };

                    // Neural Model Selector
                    egui::ComboBox::from_id_salt("neural_model_select")
                        .selected_text(match self.selected_neural_model {
                            NeuralModel::Dpdfnet2_48k => "DPDFNet2-48k",
                            NeuralModel::DeepFilterNet3 => "DeepFilterNet3",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.selected_neural_model,
                                NeuralModel::Dpdfnet2_48k,
                                "DPDFNet2-48k (sherpa-onnx)",
                            );
                            ui.selectable_value(
                                &mut self.selected_neural_model,
                                NeuralModel::DeepFilterNet3,
                                "DeepFilterNet3 (tract/musl)",
                            );
                        });

                    if self.selected_neural_model == NeuralModel::DeepFilterNet3 {
                        ui.checkbox(&mut self.deepfilter_post_filter, "Post-Filter")
                            .on_hover_text("Enable DeepFilterNet3 post-filtering for stronger suppression");
                    }

                    let can_ai_clean = !is_recording
                        && !is_calibrating
                        && !status.is_saving
                        && !self.is_ai_processing
                        && !self.is_dsp_processing
                        && !self.is_uploading
                        && self.denoiser_available
                        && (self.dsp_cleaned_path().exists()
                            || self
                                .recorder
                                .last_recording_path()
                                .map(|p| p.exists())
                                .unwrap_or(false));

                    let ai_btn = ui.add_enabled(
                        can_ai_clean,
                        egui::Button::new(
                            RichText::new(if self.is_ai_processing {
                                "🧠 4. Processing AI..."
                            } else {
                                &ai_btn_text
                            })
                            .size(15.0)
                            .color(if can_ai_clean {
                                Color32::from_rgb(168, 85, 247)
                            } else {
                                Color32::from_rgb(100, 116, 139)
                            }),
                        ),
                    ).on_hover_text(ai_tooltip);

                    if ai_btn.clicked() {
                        self.run_deep_cleaning();
                    }

                    if self.is_ai_processing {
                        ui.spinner();
                        ui.label(
                            RichText::new("🧠 Neural Denoising...")
                                .color(Color32::from_rgb(168, 85, 247))
                                .strong(),
                        );
                    }

                    // Time display or Saving status
                    if status.is_saving {
                        ui.spinner();
                        ui.label(
                            RichText::new("Saving audio...")
                                .color(Color32::from_rgb(56, 189, 248))
                                .strong(),
                        );
                    } else if is_recording {
                        ui.label(
                            RichText::new(format!("⏱ {:04.1}s", status.recorded_seconds))
                                .size(16.0)
                                .strong()
                                .color(Color32::from_rgb(239, 68, 68)),
                        );
                    } else if is_calibrating {
                        ui.label(
                            RichText::new(format!(
                                "Calibrating: {:.0}%",
                                status.calibration_progress * 100.0
                            ))
                            .size(15.0)
                            .color(Color32::from_rgb(56, 189, 248)),
                        );
                    }
                });

                if status.mode == RecorderMode::Calibrating {
                    ui.add_space(4.0);
                    ui.add(
                        ProgressBar::new(status.calibration_progress)
                            .fill(Color32::from_rgb(56, 189, 248)),
                    );
                }
            });

            ui.add_space(8.0);

            // Waveform & Playback Controls Section
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Waveform Preview:").strong());

                    let prev_tab = self.active_tab;
                    ui.selectable_value(&mut self.active_tab, AudioTab::Original, "Original (Vocal)");
                    ui.selectable_value(
                        &mut self.active_tab,
                        AudioTab::NoiseReference,
                        "Noise Reference",
                    );
                    if self.dsp_cleaned_path().exists() {
                        ui.selectable_value(
                            &mut self.active_tab,
                            AudioTab::DspCleaned,
                            "⚡ DSP Cleaned",
                        );
                    }
                    if self.removed_noise_path().exists() {
                        ui.selectable_value(
                            &mut self.active_tab,
                            AudioTab::RemovedNoise,
                            "🗑 DSP Noise",
                        );
                    }
                    if self.deep_cleaned_path().exists() {
                        ui.selectable_value(
                            &mut self.active_tab,
                            AudioTab::DeepCleaned,
                            "🧠 ✨ Deep Cleaned (AI)",
                        );
                    }
                    if self.removed_deep_noise_path().exists() {
                        ui.selectable_value(
                            &mut self.active_tab,
                            AudioTab::RemovedDeepNoise,
                            "🗑 AI Removed Noise",
                        );
                    }

                    if self.active_tab != prev_tab {
                        let seamless = prev_tab.is_full_take() && self.active_tab.is_full_take();
                        self.reload_active_audio_seamless(seamless);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let current_pos = self.player.position_seconds();
                        let total_dur = self.player.duration_seconds();
                        ui.label(format!(
                            "{:02}:{:04.1} / {:02}:{:04.1}",
                            (current_pos / 60.0) as u32,
                            current_pos % 60.0,
                            (total_dur / 60.0) as u32,
                            total_dur % 60.0
                        ));
                    });
                });

                ui.add_space(4.0);

                // Waveform rendering with interactive scrub
                let duration = self.player.duration_seconds();
                let progress = if duration > 0.0 {
                    self.player.position_seconds() / duration
                } else {
                    0.0
                };

                if self.waveform.is_empty() && !self.player.samples().is_empty() {
                    self.waveform.update(self.player.samples(), 600);
                }

                let waveform_response = self.waveform.show(ui, progress, 140.0);
                if (waveform_response.clicked() || waveform_response.dragged()) && duration > 0.0 {
                    if let Some(pos) = waveform_response.interact_pointer_pos() {
                        let fraction = ((pos.x - waveform_response.rect.left())
                            / waveform_response.rect.width())
                            .clamp(0.0, 1.0);
                        self.player.seek(fraction * duration);
                    }
                }

                ui.add_space(6.0);

                // Playback Transport Buttons
                ui.horizontal(|ui| {
                    let has_audio = self.player.duration_seconds() > 0.0;

                    if !self.player.is_playing() {
                        if ui.add_enabled(has_audio, egui::Button::new("▶ Play")).clicked() {
                            self.player.play();
                        }
                    } else {
                        if ui.button("⏸ Pause").clicked() {
                            self.player.pause();
                        }
                    }

                    if ui.add_enabled(has_audio, egui::Button::new("⏹ Stop")).clicked() {
                        self.player.stop();
                    }

                    if ui.button("🔄 Reload Audio").clicked() {
                        self.reload_active_audio();
                    }
                });
            });

            // Collapsible Audio Diagnostics (Phases 3-6)
            ui.add_space(4.0);
            ui.collapsing("🔍 Audio Analysis & Pipeline Diagnostics", |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⚡ Re-run DSP Analysis").clicked() {
                        self.run_dsp_analysis();
                    }
                });
                ui.add_space(2.0);

                if let Some(ref prof) = self.noise_profile {
                    ui.label(RichText::new("Noise Reference Profile:").strong().color(Color32::from_rgb(56, 189, 248)));
                    ui.label(format!("• Estimated Noise Floor: {:.1} dBFS", prof.noise_floor_dbfs));
                    let stationarity_label = if prof.stationarity_score >= 0.70 {
                        "Stationary (Fan/Hiss/Hum)"
                    } else if prof.stationarity_score >= 0.50 {
                        "Semi-stationary (Codec-gated / Modulated)"
                    } else {
                        "Non-stationary (Traffic/Bursts)"
                    };
                    ui.label(format!(
                        "• Stationarity: {:.2} ({})",
                        prof.stationarity_score,
                        stationarity_label
                    ));
                    if prof.tonal_peaks.is_empty() {
                        ui.label("• Tonal Peaks: None detected");
                    } else {
                        ui.label(format!("• Tonal Peaks ({} detected):", prof.tonal_peaks.len()));
                        for p in &prof.tonal_peaks {
                            ui.label(format!(
                                "    - {:.1} Hz (+{:.1} dB prominence, {:.0}% conf)",
                                p.frequency_hz, p.strength_db, p.confidence * 100.0
                            ));
                        }
                    }
                } else {
                    ui.label(RichText::new("No noise reference calibrated yet. Click 'Calibrate Noise' to sample background.").italics());
                }

                if let Some(ref sig) = self.signal_report {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(RichText::new("Voice Recording Analysis:").strong().color(Color32::from_rgb(56, 189, 248)));
                    ui.label(format!("• Overall Vocal Confidence: {:.2}", sig.activity.overall_confidence));
                    ui.label(format!("• Vocal-Band SNR: {:.1} dB", sig.activity.vocal_snr_db));
                    ui.label(format!("• Active Speech Frame Ratio: {:.1}%", sig.activity.active_frame_ratio * 100.0));
                    ui.label(format!("• Recording RMS: {:.1} dBFS (Peak: {:.1} dBFS)", sig.rms_dbfs, sig.peak_dbfs));
                }

                if let Some(ref report) = self.dsp_report {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(RichText::new("DSP Cleaning Results (Phase 4):").strong().color(Color32::from_rgb(52, 211, 153)));
                    ui.label(format!("• DSP Mode: {}", self.dsp_intensity.as_str()));
                    ui.label(format!("• Effective Attenuation: {:.1} dB", report.attenuation_db));
                    ui.label(format!(
                        "• Level Change: {:.1} dBFS -> {:.1} dBFS (Removed Noise RMS: {:.1} dBFS)",
                        report.input_rms_dbfs, report.cleaned_rms_dbfs, report.removed_noise_rms_dbfs
                    ));
                    if report.notched_frequencies.is_empty() {
                        ui.label("• Hum Notch: None needed (no persistent hum in profile)");
                    } else {
                        let f_str: Vec<String> = report.notched_frequencies.iter().map(|f| format!("{:.1} Hz", f)).collect();
                        ui.label(format!("• Hum Notch: Active on [{}]", f_str.join(", ")));
                    }
                    ui.label(format!("• Processing Duration: {:.1} ms", report.processing_time_ms));
                }

                if let Some(ref res) = self.residual_report {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Residual Noise Decision (Phase 5):").strong().color(Color32::from_rgb(168, 85, 247)));
                        let (badge_text, badge_color) = match res.level {
                            ResidualLevel::Low => ("LOW (Clean)", Color32::from_rgb(52, 211, 153)),
                            ResidualLevel::Moderate => ("MODERATE (Noticeable)", Color32::from_rgb(251, 191, 36)),
                            ResidualLevel::High => ("HIGH (Significant)", Color32::from_rgb(239, 68, 68)),
                        };
                        ui.label(RichText::new(format!("[{}]", badge_text)).strong().color(badge_color));
                    });
                    ui.label(format!("• Post-DSP Noise Floor: {:.1} dBFS (Original: {:.1} dBFS)", res.residual_noise_dbfs, res.original_noise_dbfs));
                    ui.label(format!("• Post-DSP Speech SNR: {:.1} dB (Speech RMS: {:.1} dBFS)", res.post_dsp_snr_db, res.speech_rms_dbfs));
                    ui.label(format!("• Total Classical Attenuation: {:.1} dB", res.noise_attenuation_db));

                    let decision_color = match res.decision {
                        DenoiseDecision::FinishWithoutAi => Color32::from_rgb(52, 211, 153),
                        DenoiseDecision::InvokeNeuralBackend => Color32::from_rgb(192, 132, 252),
                    };
                    ui.label(RichText::new(format!("• Architectural Decision: {}", res.decision.as_str())).strong().color(decision_color));
                    ui.label(RichText::new(format!("  ↳ {}", res.explanation)).italics().color(Color32::from_rgb(203, 213, 225)));
                }

                if let Some(ref d_rep) = self.denoise_report {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(RichText::new(format!("Neural AI Deep Clean ({}):", d_rep.model_name)).strong().color(Color32::from_rgb(168, 85, 247)));
                    ui.label("• Architecture: Full-Band 48 kHz High-Resolution Neural Speech & Singing Enhancer");
                    ui.label(format!("• Effective Neural Attenuation: {:.1} dB", d_rep.attenuation_db));
                    ui.label(format!(
                        "• Level Change: {:.1} dBFS -> {:.1} dBFS (Removed Noise RMS: {:.1} dBFS)",
                        d_rep.input_rms_dbfs, d_rep.cleaned_rms_dbfs, d_rep.removed_noise_rms_dbfs
                    ));
                    ui.label(format!("• Inference Latency: {} samples ({:.1} ms)", d_rep.latency_samples, d_rep.latency_ms));
                    ui.label(format!("• Execution Time: {:.1} ms (RTF: {:.3}x real-time)", d_rep.inference_time_ms, d_rep.rtf));
                } else if self.denoiser_available {
                    ui.add_space(4.0);
                    ui.separator();
                    let dpdf_status = if self.dpdfnet_available { "Ready" } else { "Missing" };
                    let df_status = if self.deepfilter_available { "Ready" } else { "Missing" };
                    ui.label(RichText::new("Neural AI Deep Clean (Phase 6 & 8 Backends):").strong().color(Color32::from_rgb(168, 85, 247)));
                    ui.label(format!("• DPDFNet2-48k (sherpa-onnx): {}", dpdf_status));
                    ui.label(format!("• DeepFilterNet3 (tract/musl): {}", df_status));
                }

                if let Some(ref pres) = self.dsp_preservation_report {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(RichText::new("Preservation Layer (Phase 7):").strong().color(Color32::from_rgb(56, 189, 248)));
                    ui.label(format!("• Vocal Energy Preserved: {:.1}%", pres.vocal_preservation_percentage));
                    ui.label(format!("• Vocal Leakage Attenuation: {:.1} dB in removed noise", pres.vocal_leakage_attenuation_db));
                    ui.label(format!("• Mean Blending Alpha: {:.2}", pres.mean_alpha));
                    let comb_text = if pres.comb_metrics.has_comb_filtering {
                        "⚠ Comb-filtering detected"
                    } else {
                        "✓ None (Constructive phase-coherent mix)"
                    };
                    ui.label(format!("• Phasing & Comb Filtering: {}", comb_text));
                    ui.label(format!("• Delay Compensation: {} samples ({:.1} ms)", pres.latency_samples, pres.latency_ms));
                }
            });
            ui.add_space(8.0);
        });
    });
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Offline Voice & Singing Noise Remover")
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([680.0, 500.0])
            .with_resizable(true)
            .with_maximize_button(true),
        ..Default::default()
    };

    eframe::run_native(
        "Voice Cleaner",
        options,
        Box::new(|cc| Ok(Box::new(NoiseRemoverApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_devices() {
        let output_dir = PathBuf::from("recordings");
        let recorder = AudioRecorder::new(&output_dir);
        let devices = recorder.device_manager().list_input_devices().unwrap();
        println!("=== APP DEVICES LIST ===");
        for (idx, name) in &devices {
            println!("  [{}] {}", idx, name);
        }
        println!("========================");
    }
}
