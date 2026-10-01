//! Voice Cleaner Tauri v2 Desktop Application Entry.

mod commands;
mod state;
mod tracks;
mod waveform_helper;

use state::AppState;
use std::path::PathBuf;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let recordings_dir = if PathBuf::from("../recordings").exists() {
        std::fs::canonicalize(PathBuf::from("../recordings"))
            .unwrap_or_else(|_| PathBuf::from("../recordings"))
    } else {
        std::fs::canonicalize(PathBuf::from("recordings"))
            .unwrap_or_else(|_| PathBuf::from("recordings"))
    };

    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unmaximize();
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: 1040.0,
                    height: 660.0,
                }));
                let _ = win.center();
            }
            Ok(())
        })
        .manage(AppState::new(recordings_dir))
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
