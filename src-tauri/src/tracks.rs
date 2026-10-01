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

pub fn get_audio_duration_secs(path: &Path) -> Option<f32> {
    if let Ok(reader) = hound::WavReader::open(path) {
        let spec = reader.spec();
        if spec.sample_rate > 0 {
            return Some(reader.duration() as f32 / spec.sample_rate as f32);
        }
    }
    if let Ok((samples, spec)) = audio_core::read_wav_canonical_f32(path) {
        if spec.sample_rate > 0 {
            return Some(samples.len() as f32 / spec.sample_rate as f32);
        }
    }
    None
}

pub struct TrackManager {
    base_dir: PathBuf,
}

impl TrackManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        let dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).ok();
        Self { base_dir: dir }
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
                    let orig_file = path.join("original.wav");
                    let clean_file = path.join("cleaned.wav");

                    let has_orig = orig_file.is_file()
                        && orig_file.metadata().map(|m| m.len() > 0).unwrap_or(false);
                    let has_clean = clean_file.is_file()
                        && clean_file.metadata().map(|m| m.len() > 0).unwrap_or(false);

                    // Only show tracks that actually have recorded audio files inside the app's directory
                    if !has_orig && !has_clean {
                        continue;
                    }

                    let audio_file = if has_clean { &clean_file } else { &orig_file };

                    let meta_path = path.join("meta.json");
                    let track_id = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("track")
                        .to_string();

                    let mut meta = if let Ok(content) = fs::read_to_string(&meta_path) {
                        serde_json::from_str::<TrackMetadata>(&content).ok()
                    } else {
                        None
                    };

                    let mut needs_save = false;

                    // If meta.json is missing, reconstruct it from the audio file
                    if meta.is_none() {
                        let mut duration_secs = 0.0;
                        let mut waveform = Vec::new();
                        if let Some(dur) = get_audio_duration_secs(audio_file) {
                            duration_secs = dur;
                        } else if let Ok((samples, spec)) =
                            audio_core::read_wav_canonical_f32(audio_file)
                        {
                            if spec.sample_rate > 0 {
                                duration_secs = samples.len() as f32 / spec.sample_rate as f32;
                            }
                        }
                        if let Ok((samples, _)) = audio_core::read_wav_canonical_f32(audio_file) {
                            waveform = compute_waveform_peaks(&samples, 120);
                        }
                        let mins = (duration_secs / 60.0).floor() as u32;
                        let secs = (duration_secs % 60.0).floor() as u32;
                        let dur_fmt = format!("{:02}:{:02}", mins, secs);

                        let new_meta = TrackMetadata {
                            id: track_id.clone(),
                            title: track_id.replace('_', " "),
                            created_at: "Recorded".to_string(),
                            timestamp: path
                                .metadata()
                                .and_then(|m| m.modified())
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0),
                            duration_secs,
                            formatted_duration: dur_fmt,
                            is_favorite: false,
                            has_raw: has_orig,
                            has_clean,
                            model_used: "DPDFNet2 (High Quality)".to_string(),
                            raw_waveform: waveform.clone(),
                            clean_waveform: waveform,
                        };
                        needs_save = true;
                        meta = Some(new_meta);
                    }

                    if let Some(mut m) = meta {
                        m.has_raw = has_orig;
                        m.has_clean = has_clean;

                        // Ensure duration matches actual audio file on disk
                        if let Some(real_dur) = get_audio_duration_secs(audio_file) {
                            if (m.duration_secs - real_dur).abs() > 0.5 || m.formatted_duration.is_empty() {
                                m.duration_secs = real_dur;
                                let mins = (real_dur / 60.0).floor() as u32;
                                let secs = (real_dur % 60.0).floor() as u32;
                                m.formatted_duration = format!("{:02}:{:02}", mins, secs);
                                needs_save = true;
                            }
                        }

                        // Recompute raw waveform if empty or flat dummy
                        let is_flat_raw = m.raw_waveform.is_empty()
                            || (m.raw_waveform.len() > 1
                                && m.raw_waveform
                                    .iter()
                                    .all(|&v| (v - m.raw_waveform[0]).abs() < 1e-4));
                        if is_flat_raw && has_orig {
                            if let Ok((samples, _)) =
                                audio_core::read_wav_canonical_f32(&orig_file)
                            {
                                m.raw_waveform = compute_waveform_peaks(&samples, 120);
                                needs_save = true;
                            }
                        }

                        // Recompute clean waveform if empty or flat dummy
                        let is_flat_clean = m.clean_waveform.is_empty()
                            || (m.clean_waveform.len() > 1
                                && m.clean_waveform
                                    .iter()
                                    .all(|&v| (v - m.clean_waveform[0]).abs() < 1e-4));
                        if is_flat_clean && has_clean {
                            if let Ok((samples, _)) =
                                audio_core::read_wav_canonical_f32(&clean_file)
                            {
                                m.clean_waveform = compute_waveform_peaks(&samples, 120);
                                needs_save = true;
                            }
                        }

                        if needs_save {
                            let _ = self.save_track(&m);
                        }

                        tracks.push(m);
                    }
                }
            }
        }
        // Sort descending by timestamp (newest first)
        tracks.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        tracks
    }

    pub fn get_track(&self, id: &str) -> Option<TrackMetadata> {
        let dir = self.get_track_dir(id);
        let meta_path = dir.join("meta.json");
        let mut meta: TrackMetadata = fs::read_to_string(&meta_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())?;

        let orig_file = dir.join("original.wav");
        let clean_file = dir.join("cleaned.wav");
        let has_orig = orig_file.is_file();
        let has_clean = clean_file.is_file();
        if !has_orig && !has_clean {
            return None;
        }
        let audio_file = if has_clean { &clean_file } else { &orig_file };

        if let Some(real_dur) = get_audio_duration_secs(audio_file) {
            if (meta.duration_secs - real_dur).abs() > 0.5 || meta.formatted_duration.is_empty() {
                meta.duration_secs = real_dur;
                let mins = (real_dur / 60.0).floor() as u32;
                let secs = (real_dur % 60.0).floor() as u32;
                meta.formatted_duration = format!("{:02}:{:02}", mins, secs);
                let _ = self.save_track(&meta);
            }
        }

        Some(meta)
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
}
