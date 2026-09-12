//! Desktop GUI application for Offline Voice & Singing Noise Remover.

mod waveform;

use std::path::PathBuf;
use eframe::egui::{self, Color32, ProgressBar, RichText};
use playback::AudioPlayer;
use recorder::{AudioRecorder, RecorderMode};
use waveform::WaveformRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioTab {
    Original,
    NoiseReference,
}

struct NoiseRemoverApp {
    recorder: AudioRecorder,
    player: AudioPlayer,
    waveform: WaveformRenderer,
    devices: Vec<(usize, String)>,
    selected_device_idx: usize,
    active_tab: AudioTab,
    status_message: String,
    clipping_highlight_frames: usize,
    smoothed_peak_dbfs: f32,
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

        let mut selected_idx = 0;
        let mut status = "Select a microphone to begin monitoring.".to_string();

        if let Ok(()) = recorder.select_default_device() {
            if let Some(idx) = recorder.selected_device_idx() {
                selected_idx = idx;
            }
            if let Some((_, name)) = devices.iter().find(|(i, _)| *i == selected_idx) {
                status = format!("Monitoring default device: {}", name);
            }
        } else if !devices.is_empty() {
            selected_idx = devices[0].0;
            if let Err(e) = recorder.select_device(selected_idx) {
                status = format!("Failed to open microphone: {}", e);
            } else {
                status = format!("Monitoring: {}", devices[0].1);
            }
        }

        Self {
            recorder,
            player: AudioPlayer::new(),
            waveform: WaveformRenderer::new(),
            devices,
            selected_device_idx: selected_idx,
            active_tab: AudioTab::Original,
            status_message: status,
            clipping_highlight_frames: 0,
            smoothed_peak_dbfs: -96.0,
        }
    }

    fn reload_active_audio(&mut self) {
        let path = match self.active_tab {
            AudioTab::Original => self.recorder.last_recording_path(),
            AudioTab::NoiseReference => self.recorder.last_calibration_path(),
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
    }
}

impl eframe::App for NoiseRemoverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repaint while recording, calibrating, or playing
        let status = self.recorder.status();
        if status.mode != RecorderMode::Idle || self.player.is_playing() {
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

            // Device Selection Section
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Input Device:").strong());
                    let prev_idx = self.selected_device_idx;
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
                    // Calibration Button
                    let is_calibrating = status.mode == RecorderMode::Calibrating;
                    let is_recording = status.mode == RecorderMode::Recording;

                    let calib_btn = ui.add_enabled(
                        !is_calibrating && !is_recording,
                        egui::Button::new(RichText::new("🎯 1. Calibrate Noise (2s)").size(15.0)),
                    );

                    if calib_btn.clicked() {
                        self.recorder.start_calibration();
                        self.status_message =
                            "Calibrating: please remain silent for 2 seconds...".into();
                    }

                    // Record / Stop Button
                    if !is_recording {
                        let rec_btn = ui.add_enabled(
                            !is_calibrating,
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
                            self.status_message = "Recording saved to original.wav".into();
                            self.reload_active_audio();
                        }
                    }

                    // Time display
                    if is_recording {
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

                // Waveform rendering
                let duration = self.player.duration_seconds();
                let progress = if duration > 0.0 {
                    self.player.position_seconds() / duration
                } else {
                    0.0
                };

                let waveform_response = self.waveform.show(ui, progress, 140.0);
                if waveform_response.clicked() {
                    if let Some(pos) = waveform_response.interact_pointer_pos() {
                        let fraction = (pos.x - waveform_response.rect.left())
                            / waveform_response.rect.width();
                        self.player.seek((fraction.clamp(0.0, 1.0)) * duration);
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

            // Status Bar at Bottom
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&self.status_message)
                        .color(Color32::from_rgb(148, 163, 184))
                        .italics(),
                );
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
