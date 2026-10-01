//! Multi-session track management and persistence.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::waveform_helper::compute_waveform_peaks;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrackMetadata {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub timestamp: u64,
    pub duration_secs: f32,
    pub formatted_duration: String,
    pub is_favorite: bool,
    pub has_raw: bool,
    pub has_clean: bool,
    pub model_used: String,
    pub raw_waveform: Vec<f32>,
    pub clean_waveform: Vec<f32>,
}

pub struct TrackManager {
    base_dir: PathBuf,
}

impl TrackManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        let dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).ok();
        let manager = Self { base_dir: dir };
        manager.ensure_seed_tracks();
        manager
    }

    #[allow(dead_code)]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn get_track_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    pub fn list_tracks(&self) -> Vec<TrackMetadata> {
        let mut tracks = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let meta_path = path.join("meta.json");
                    if let Ok(content) = fs::read_to_string(&meta_path) {
                        if let Ok(meta) = serde_json::from_str::<TrackMetadata>(&content) {
                            tracks.push(meta);
                        }
                    }
                }
            }
        }
        // Sort descending by timestamp (newest first)
        tracks.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        tracks
    }

    pub fn get_track(&self, id: &str) -> Option<TrackMetadata> {
        let meta_path = self.get_track_dir(id).join("meta.json");
        fs::read_to_string(meta_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    }

    pub fn save_track(&self, meta: &TrackMetadata) -> Result<(), String> {
        let track_dir = self.get_track_dir(&meta.id);
        fs::create_dir_all(&track_dir).map_err(|e| e.to_string())?;
        let meta_path = track_dir.join("meta.json");
        let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
        fs::write(meta_path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool, String> {
        let mut track = self.get_track(id).ok_or("Track not found")?;
        track.is_favorite = !track.is_favorite;
        self.save_track(&track)?;
        Ok(track.is_favorite)
    }

    pub fn delete_track(&self, id: &str) -> Result<(), String> {
        let track_dir = self.get_track_dir(id);
        if track_dir.exists() {
            fs::remove_dir_all(track_dir).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn rename_track(&self, id: &str, new_title: &str) -> Result<(), String> {
        let mut track = self.get_track(id).ok_or("Track not found")?;
        track.title = new_title.to_string();
        self.save_track(&track)?;
        Ok(())
    }

    /// Pre-populate tracks from `misc/*.wav` if the recordings directory has no tracks
    fn ensure_seed_tracks(&self) {
        let existing = self.list_tracks();
        if !existing.is_empty() {
            return;
        }

        let seed_sources = [
            ("Track 01", "Mon, Oct 27, 2025 • 12:14", 1761567240, true, "misc/original.wav", "misc/deep_cleaned.wav", 134.0, "02:14"),
            ("Track 02", "Sun, Oct 26, 2025 • 18:03", 1761501780, false, "misc/test_sample.wav", "misc/dsp_cleaned.wav", 96.0, "01:36"),
            ("Track 03", "Sat, Oct 25, 2025 • 10:21", 1761387660, true, "misc/original.wav", "misc/deep_cleaned.wav", 188.0, "03:08"),
            ("Track 04", "Fri, Oct 24, 2025 • 22:17", 1761344220, false, "misc/test_10s.wav", "misc/out_adapt.wav", 54.0, "00:54"),
            ("Track 05", "Thu, Oct 23, 2025 • 15:09", 1761232140, false, "misc/original.wav", "misc/deep_cleaned.wav", 261.0, "04:21"),
            ("Track 06", "Wed, Oct 22, 2025 • 09:33", 1761125580, true, "misc/test_sample.wav", "misc/dsp_cleaned.wav", 108.0, "01:48"),
            ("Track 07", "Tue, Oct 21, 2025 • 16:05", 1761062700, false, "misc/original.wav", "misc/out_pure.wav", 157.0, "02:37"),
        ];

        for (i, (title, date_str, ts, is_fav, raw_src, clean_src, duration, dur_fmt)) in seed_sources.iter().enumerate() {
            let id = format!("track_{:02}", i + 1);
            let dir = self.get_track_dir(&id);
            fs::create_dir_all(&dir).ok();

            let orig_dest = dir.join("original.wav");
            let clean_dest = dir.join("cleaned.wav");

            let mut raw_wave = Vec::new();
            let mut clean_wave = Vec::new();

            if Path::new(raw_src).exists() {
                fs::copy(raw_src, &orig_dest).ok();
                if let Ok((samples, _)) = audio_core::read_wav_canonical_f32(&orig_dest) {
                    raw_wave = compute_waveform_peaks(&samples, 120);
                }
            }

            if Path::new(clean_src).exists() {
                fs::copy(clean_src, &clean_dest).ok();
                if let Ok((samples, _)) = audio_core::read_wav_canonical_f32(&clean_dest) {
                    clean_wave = compute_waveform_peaks(&samples, 120);
                }
            }

            if raw_wave.is_empty() {
                raw_wave = vec![0.3; 120];
            }
            if clean_wave.is_empty() {
                clean_wave = raw_wave.clone();
            }

            let meta = TrackMetadata {
                id: id.clone(),
                title: title.to_string(),
                created_at: date_str.to_string(),
                timestamp: *ts,
                duration_secs: *duration,
                formatted_duration: dur_fmt.to_string(),
                is_favorite: *is_fav,
                has_raw: orig_dest.exists(),
                has_clean: clean_dest.exists(),
                model_used: "DPDFNet2 (High Quality)".to_string(),
                raw_waveform: raw_wave,
                clean_waveform: clean_wave,
            };

            let _ = self.save_track(&meta);
        }
    }
}
