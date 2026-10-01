//! Global application state shared across Tauri command handlers.

use playback::AudioPlayer;
use recorder::AudioRecorder;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::tracks::TrackManager;

pub struct AppState {
    pub recorder: Arc<Mutex<AudioRecorder>>,
    pub player: Arc<Mutex<AudioPlayer>>,
    pub track_manager: Arc<TrackManager>,
    pub current_playing_track: Arc<Mutex<Option<String>>>,
    pub current_playing_is_clean: Arc<Mutex<bool>>,
    pub active_track_id: Arc<Mutex<Option<String>>>,
    #[allow(dead_code)]
    pub is_processing: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(recordings_dir: PathBuf) -> Self {
        let track_manager = Arc::new(TrackManager::new(&recordings_dir));
        let mut rec = AudioRecorder::new(&recordings_dir);
        if let Err(e) = rec.select_default_device() {
            log::warn!("Could not initialize default audio input device on startup: {}", e);
        }
        let recorder = Arc::new(Mutex::new(rec));
        let player = Arc::new(Mutex::new(AudioPlayer::new()));

        Self {
            recorder,
            player,
            track_manager,
            current_playing_track: Arc::new(Mutex::new(None)),
            current_playing_is_clean: Arc::new(Mutex::new(true)),
            active_track_id: Arc::new(Mutex::new(None)),
            is_processing: Arc::new(AtomicBool::new(false)),
        }
    }
}
