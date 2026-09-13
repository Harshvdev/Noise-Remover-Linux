//! Desktop GUI application for Offline Voice & Singing Noise Remover.

mod waveform;

use std::path::PathBuf;
use dsp::{
    DspConfig, DspProcessingReport, DspProcessor, NoiseAnalyzer, NoiseProfile,
    SignalAnalysisReport,
};
use eframe::egui::{self, Color32, ProgressBar, RichText};
use playback::AudioPlayer;
use recorder::{AudioRecorder, RecorderMode};
use waveform::WaveformRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioTab {
    Original,
    NoiseReference,
    DspCleaned,
    RemovedNoise,
}

struct NoiseRemoverApp {
    recorder: AudioRecorder,
    player: AudioPlayer,
    waveform: WaveformRenderer,
    analyzer: Option<NoiseAnalyzer>,
    noise_profile: Option<NoiseProfile>,
    signal_report: Option<SignalAnalysisReport>,
    dsp_processor: Option<DspProcessor>,
    dsp_report: Option<DspProcessingReport>,
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
            dsp_processor: DspProcessor::new(48000).ok(),
            dsp_report: None,
            devices,
            selected_device_idx: selected_idx,
            active_tab: AudioTab::Original,
            status_message: status,
            clipping_highlight_frames: 0,
            smoothed_peak_dbfs: -96.0,
            last_finalized_count: 0,
            active_capture_mode: None,
        };
        app.run_dsp_analysis();
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
                        log::info!("[INFO] Noise stationarity: {:.2}", profile.stationarity_score);
                        log::info!("[INFO] Estimated noise floor: {:.1} dBFS", profile.noise_floor_dbfs);
                        for peak in &profile.tonal_peaks {
                            log::info!(
                                "[INFO] Tonal peak: {:.1} Hz (prominence: {:.1} dB, conf: {:.2})",
                                peak.frequency_hz, peak.strength_db, peak.confidence
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
                                report.activity.overall_confidence, report.activity.vocal_snr_db
                            );
                            self.signal_report = Some(report);
                        }
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

    fn run_dsp_cleaning(&mut self) {
        let profile = match self.noise_profile.as_ref() {
            Some(p) => p,
            None => {
                self.status_message = "Please calibrate noise reference first.".into();
                return;
            }
        };

        let rec_path = match self.recorder.last_recording_path() {
            Some(p) if p.exists() => p,
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

        let processor = match self.dsp_processor.as_ref() {
            Some(p) => p,
            None => {
                self.status_message = "DSP processor not initialized.".into();
                return;
            }
        };

        let config = DspConfig::default();
        let activity = self.signal_report.as_ref().map(|r| &r.activity);

        match processor.process(&samples, profile, activity, &config) {
            Ok(result) => {
                let cleaned_path = self.dsp_cleaned_path();
                let noise_path = self.removed_noise_path();

                if let Err(e) =
                    audio_core::write_wav_f32(&cleaned_path, &result.cleaned_samples, 48000, 1)
                {
                    self.status_message = format!("Failed to write dsp_cleaned.wav: {}", e);
                    return;
                }

                if let Err(e) =
                    audio_core::write_wav_f32(&noise_path, &result.removed_noise_samples, 48000, 1)
                {
                    self.status_message = format!("Failed to write removed_noise.wav: {}", e);
                    return;
                }

                let att = result.report.attenuation_db;
                let ms = result.report.processing_time_ms;
                let notches = result.report.notched_frequencies.clone();
                self.dsp_report = Some(result.report);
                self.active_tab = AudioTab::DspCleaned;
                self.reload_active_audio();

                let notch_msg = if notches.is_empty() {
                    String::new()
                } else {
                    let freqs_str: Vec<String> =
                        notches.iter().map(|f| format!("{:.1} Hz", f)).collect();
                    format!(" (Hum notched: {})", freqs_str.join(", "))
                };
                self.status_message = format!(
                    "DSP cleaning complete: {:.1} dB noise reduction in {:.0} ms!{}",
                    att, ms, notch_msg
                );
            }
            Err(e) => {
                self.status_message = format!("DSP processing failed: {}", e);
            }
        }
    }

    fn reload_active_audio(&mut self) {
        let path = match self.active_tab {
            AudioTab::Original => self.recorder.last_recording_path(),
            AudioTab::NoiseReference => self.recorder.last_calibration_path(),
            AudioTab::DspCleaned => Some(self.dsp_cleaned_path()),
            AudioTab::RemovedNoise => Some(self.removed_noise_path()),
        };

        if let Some(p) = path {
            if p.exists() {
                if let Err(e) = self.player.load_file(&p) {
                    self.status_message = format!("Playback error: {}", e);
                } else {
                    self.waveform.update(self.player.samples(), 600);
                }
            }
        }
        self.run_dsp_analysis();
    }
}

impl eframe::App for NoiseRemoverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let status = self.recorder.status();

        if status.mode != RecorderMode::Idle {
            self.active_capture_mode = Some(status.mode);
        }

        // Check if recording or calibration finished writing to disk asynchronously
        let cur_finalized = self.recorder.finalized_count();
        if cur_finalized > self.last_finalized_count {
            self.last_finalized_count = cur_finalized;
            let finished_mode = self.active_capture_mode.take();
            if finished_mode == Some(RecorderMode::Calibrating) {
                self.status_message = "Noise reference calibrated (2s). Ready to record voice!".into();
                self.run_dsp_analysis();
                // Keep active_tab as Original (or reload reference only if user is on that tab)
                if self.active_tab == AudioTab::NoiseReference {
                    self.reload_active_audio();
                }
            } else {
                self.active_tab = AudioTab::Original;
                self.reload_active_audio();
                self.status_message = "Recording saved to original.wav and loaded.".into();
            }
        }

        // Request continuous repaint while recording, calibrating, saving, or playing
        if status.mode != RecorderMode::Idle || status.is_saving || self.player.is_playing() {
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

        egui::CentralPanel::default().show(ctx, |ui| {
            // Top Header
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("🎙 Voice & Singing Noise Remover")
                        .size(22.0)
                        .strong()
                        .color(Color32::from_rgb(56, 189, 248)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

            // Level Meter & Clipping Indicator
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎤 Live Microphone Input").strong());
                    ui.label(
                        RichText::new("(monitors ambient room sound from selected mic)")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
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
            });

            ui.add_space(8.0);

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

                    // 3. DSP Noise Removal Button
                    let can_clean = !is_recording
                        && !is_calibrating
                        && !status.is_saving
                        && self.noise_profile.is_some()
                        && self
                            .recorder
                            .last_recording_path()
                            .map(|p| p.exists())
                            .unwrap_or(false);

                    let clean_btn = ui.add_enabled(
                        can_clean,
                        egui::Button::new(
                            RichText::new("⚡ 3. Clean Noise (DSP)")
                                .size(15.0)
                                .color(if can_clean {
                                    Color32::from_rgb(52, 211, 153)
                                } else {
                                    Color32::from_rgb(100, 116, 139)
                                }),
                        ),
                    );
                    if clean_btn.clicked() {
                        self.run_dsp_cleaning();
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
                            "✨ DSP Cleaned",
                        );
                    }
                    if self.removed_noise_path().exists() {
                        ui.selectable_value(
                            &mut self.active_tab,
                            AudioTab::RemovedNoise,
                            "🗑 Removed Noise",
                        );
                    }

                    if self.active_tab != prev_tab {
                        self.reload_active_audio();
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

            // Collapsible DSP Noise Diagnostics (Section 68: Internal diagnostic display)
            ui.add_space(4.0);
            ui.collapsing("🔍 DSP Diagnostics (Phase 3)", |ui| {
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
            });

            // Status Bar at Bottom
            ui.add_space(8.0);
            ui.separator();
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
        });
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Offline Voice & Singing Noise Remover")
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([680.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Voice Cleaner",
        options,
        Box::new(|cc| Ok(Box::new(NoiseRemoverApp::new(cc)))),
    )
}
