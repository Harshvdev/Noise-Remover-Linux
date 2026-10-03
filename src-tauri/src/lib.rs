//! Voice Cleaner Tauri v2 Desktop Application Entry.

mod commands;
mod state;
mod tracks;
mod waveform_helper;

use state::AppState;
use std::path::PathBuf;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unmaximize();
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: 1040.0,
                    height: 660.0,
                }));
                let _ = win.center();
            }

            // Cross-platform recordings directory resolution
            let recordings_dir = if cfg!(target_os = "android") {
                app.path()
                    .app_data_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("recordings")
            } else if PathBuf::from("../recordings").exists() {
                std::fs::canonicalize(PathBuf::from("../recordings"))
                    .unwrap_or_else(|_| PathBuf::from("../recordings"))
            } else if PathBuf::from("recordings").exists() {
                std::fs::canonicalize(PathBuf::from("recordings"))
                    .unwrap_or_else(|_| PathBuf::from("recordings"))
            } else {
                app.path()
                    .app_data_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("recordings")
            };

            if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
                log::warn!("Could not create recordings directory {:?}: {}", recordings_dir, e);
            }

            // Register neural model location if available in app data or bundled resources
            if let Ok(data_dir) = app.path().app_data_dir() {
                let extracted_model = data_dir.join("models").join("dpdfnet2_48khz_hr.onnx");
                if extracted_model.exists() {
                    std::env::set_var("VOICE_CLEANER_MODEL_PATH", &extracted_model);
                }
            }
            if std::env::var("VOICE_CLEANER_MODEL_PATH").is_err() {
                if let Ok(res_dir) = app.path().resource_dir() {
                    let bundled_model = res_dir.join("models").join("dpdfnet2_48khz_hr.onnx");
                    if bundled_model.exists() {
                        std::env::set_var("VOICE_CLEANER_MODEL_PATH", bundled_model);
                    }
                }
            }

            app.manage(AppState::new(recordings_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_devices,
            commands::select_device,
            commands::get_mic_stats,
            commands::prepare_recording_session,
            commands::start_calibration,
            commands::start_recording,
            commands::stop_recording,
            commands::get_tracks,
            commands::get_track,
            commands::toggle_favorite,
            commands::delete_track,
            commands::rename_track,
            commands::play_track,
            commands::pause_track,
            commands::resume_track,
            commands::seek_track,
            commands::get_playback_status,
            commands::process_track,
            commands::get_diagnostics,
            commands::import_audio_file,
            commands::open_folder,
            commands::window_minimize,
            commands::window_maximize,
            commands::window_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Voice Cleaner desktop application");
}
