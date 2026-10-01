//! Tauri command handlers for UI interaction.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use denoiser::{AutoPipeline, AutoPipelineConfig, NeuralModel};
use dsp::NoiseAnalyzer;
use recorder::RecorderMode;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;
use crate::tracks::TrackMetadata;
use crate::waveform_helper::compute_waveform_peaks;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceDto {
    pub index: usize,
    pub name: String,
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MicStatsDto {
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub has_clipped: bool,
    pub mode: String,
    pub recorded_seconds: f32,
    pub calibration_progress: f32,
    pub spectrum: Vec<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaybackStatusDto {
    pub is_playing: bool,
    pub position_seconds: f32,
    pub duration_seconds: f32,
    pub track_id: Option<String>,
    pub is_clean: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdvancedSettingsDto {
    pub conservative_bias: bool,
    pub declicker: bool,
    pub plosive_guard: bool,
    pub harmonic_preservation: f32, // 0.0 - 1.0 (e.g. 0.70)
    pub model_id: String,           // "dpdfnet2_48k" or "deepfilter_net3"
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiagnosticsDto {
    pub sample_rate: u32,
    pub estimated_snr_db: f32,
    pub stationarity_score: f32,
    pub tonal_peaks_count: usize,
    pub peak_noise_level_dbfs: f32,
    pub spectrum_bins: Vec<f32>,
}

#[tauri::command]
pub fn get_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let rec = state.recorder.lock().map_err(|e| e.to_string())?;
    let devices = rec
        .device_manager()
        .list_input_devices()
        .unwrap_or_default();
    let current_selected = rec.selected_device_idx();

    let dtos = devices
        .into_iter()
        .map(|(idx, name)| DeviceDto {
            index: idx,
            name,
            is_default: Some(idx) == current_selected || idx == 0,
        })
        .collect();

    Ok(dtos)
}

#[tauri::command]
pub fn select_device(state: State<'_, AppState>, index: usize) -> Result<(), String> {
    let mut rec = state.recorder.lock().map_err(|e| e.to_string())?;
    rec.select_device(index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_mic_stats(state: State<'_, AppState>) -> Result<MicStatsDto, String> {
    let rec = state.recorder.lock().map_err(|e| e.to_string())?;
    let s = rec.status();
    let mode_str = match s.mode {
        RecorderMode::Idle => "idle",
        RecorderMode::Calibrating => "calibrating",
        RecorderMode::Recording => "recording",
    };

    Ok(MicStatsDto {
        peak_dbfs: s.peak_dbfs,
        rms_dbfs: s.rms_dbfs,
        has_clipped: s.has_clipped,
        mode: mode_str.to_string(),
        recorded_seconds: s.recorded_seconds,
        calibration_progress: s.calibration_progress,
        spectrum: s.spectrum,
    })
}

#[tauri::command]
pub fn prepare_recording_session(state: State<'_, AppState>) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let track_id = format!("track_{}", now);
    let track_dir = state.track_manager.get_track_dir(&track_id);
    fs::create_dir_all(&track_dir).map_err(|e| e.to_string())?;

    let mut rec = state.recorder.lock().map_err(|e| e.to_string())?;
    let prev_idx = rec.selected_device_idx();
    *rec = recorder::AudioRecorder::new(&track_dir);
    if let Some(idx) = prev_idx {
        let _ = rec.select_device(idx);
    } else {
        let _ = rec.select_default_device();
    }

    *state.active_track_id.lock().unwrap() = Some(track_id.clone());
    Ok(track_id)
}

#[tauri::command]
pub fn start_calibration(state: State<'_, AppState>) -> Result<(), String> {
    let rec = state.recorder.lock().map_err(|e| e.to_string())?;
    rec.start_calibration();
    Ok(())
}

#[tauri::command]
pub fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    let rec = state.recorder.lock().map_err(|e| e.to_string())?;
    rec.start_recording();
    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>) -> Result<TrackMetadata, String> {
    let track_id = state
        .active_track_id
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!("track_{}", now)
        });

    let track_dir = state.track_manager.get_track_dir(&track_id);

    {
        let rec = state.recorder.lock().map_err(|e| e.to_string())?;
        rec.stop_recording_or_calibration();
    }

    // Give writer a moment to finalize WAV safely
    std::thread::sleep(std::time::Duration::from_millis(150));

    let orig_file = track_dir.join("original.wav");
    let _calib_file = track_dir.join("noise_reference.wav");

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut duration_secs = 10.0;
    let mut raw_wave = vec![0.3; 120];

    if orig_file.exists() {
        if let Ok((samples, spec)) = audio_core::read_wav_canonical_f32(&orig_file) {
            if spec.sample_rate > 0 {
                duration_secs = samples.len() as f32 / spec.sample_rate as f32;
            }
            raw_wave = compute_waveform_peaks(&samples, 120);
        }
    }

    let mins = (duration_secs / 60.0).floor() as u32;
    let secs = (duration_secs % 60.0).floor() as u32;
    let dur_fmt = format!("{:02}:{:02}", mins, secs);

    let count = state.track_manager.list_tracks().len() + 1;
    let title = format!("Track {:02}", count);

    let meta = TrackMetadata {
        id: track_id.clone(),
        title,
        created_at: "Just now".to_string(),
        timestamp: now,
        duration_secs,
        formatted_duration: dur_fmt,
        is_favorite: false,
        has_raw: orig_file.exists(),
        has_clean: false,
        model_used: "DPDFNet2 (High Quality)".to_string(),
        raw_waveform: raw_wave.clone(),
        clean_waveform: raw_wave,
    };

    state.track_manager.save_track(&meta)?;

    // Spawn background auto-cleaning pipeline!
    let tm = Arc::clone(&state.track_manager);
    let tid = track_id.clone();
    std::thread::spawn(move || {
        let tdir = tm.get_track_dir(&tid);
        let orig_path = tdir.join("original.wav");
        let calib_path = tdir.join("noise_reference.wav");
        let clean_path = tdir.join("cleaned.wav");

        if let Ok((orig_samples, _)) = audio_core::read_wav_canonical_f32(&orig_path) {
            let calib_samples = if calib_path.exists() {
                audio_core::read_wav_canonical_f32(&calib_path)
                    .ok()
                    .map(|(s, _)| s)
            } else {
                None
            };

            let config = AutoPipelineConfig::default();
            if let Ok(res) = AutoPipeline::run(
                &orig_samples,
                calib_samples.as_deref(),
                &config,
                None,
                |_prog| {},
            ) {
                if audio_core::write_wav_f32(&clean_path, &res.cleaned_samples, 48000, 1).is_ok() {
                    if let Some(mut m) = tm.get_track(&tid) {
                        m.has_clean = true;
                        m.clean_waveform = compute_waveform_peaks(&res.cleaned_samples, 120);
                        let _ = tm.save_track(&m);
                    }
                }
            }
        }
    });

    Ok(meta)
}

#[tauri::command]
pub fn get_tracks(state: State<'_, AppState>) -> Result<Vec<TrackMetadata>, String> {
    Ok(state.track_manager.list_tracks())
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: String) -> Result<TrackMetadata, String> {
    state
        .track_manager
        .get_track(&id)
        .ok_or_else(|| "Track not found".to_string())
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.track_manager.toggle_favorite(&id)
}

#[tauri::command]
pub fn delete_track(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.track_manager.delete_track(&id)
}

#[tauri::command]
pub fn rename_track(
    state: State<'_, AppState>,
    id: String,
    new_title: String,
) -> Result<(), String> {
    state.track_manager.rename_track(&id, &new_title)
}

#[tauri::command]
pub fn play_track(
    state: State<'_, AppState>,
    id: String,
    clean: bool,
    from_sec: Option<f32>,
) -> Result<(), String> {
    let track_dir = state.track_manager.get_track_dir(&id);
    let target_file = if clean {
        let cf = track_dir.join("cleaned.wav");
        if cf.exists() {
            cf
        } else {
            track_dir.join("original.wav")
        }
    } else {
        track_dir.join("original.wav")
    };

    if !target_file.exists() {
        return Err("Audio file not found on disk".to_string());
    }

    let mut player = state.player.lock().map_err(|e| e.to_string())?;

    let is_same_track = *state.current_playing_track.lock().unwrap() == Some(id.clone());
    let current_clean = *state.current_playing_is_clean.lock().unwrap();

    if is_same_track && current_clean != clean {
        // Seamless A/B switch! Preserve playhead!
        player
            .load_file_preserve_position(&target_file, true)
            .map_err(|e| e.to_string())?;
        *state.current_playing_is_clean.lock().unwrap() = clean;
    } else if !is_same_track {
        player
            .load_file_preserve_position(&target_file, false)
            .map_err(|e| e.to_string())?;
        *state.current_playing_track.lock().unwrap() = Some(id);
        *state.current_playing_is_clean.lock().unwrap() = clean;
        if let Some(sec) = from_sec {
            player.seek(sec);
        }
        player.play();
    } else {
        // Same track and same mode, just resume
        player.play();
    }

    Ok(())
}

#[tauri::command]
pub fn pause_track(state: State<'_, AppState>) -> Result<(), String> {
    let player = state.player.lock().map_err(|e| e.to_string())?;
    player.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_track(state: State<'_, AppState>) -> Result<(), String> {
    let player = state.player.lock().map_err(|e| e.to_string())?;
    player.play();
    Ok(())
}

#[tauri::command]
pub fn seek_track(state: State<'_, AppState>, sec: f32) -> Result<(), String> {
    let player = state.player.lock().map_err(|e| e.to_string())?;
    player.seek(sec);
    Ok(())
}

#[tauri::command]
pub fn get_playback_status(state: State<'_, AppState>) -> Result<PlaybackStatusDto, String> {
    let player = state.player.lock().map_err(|e| e.to_string())?;
    let cur_track = state.current_playing_track.lock().unwrap().clone();
    let is_clean = *state.current_playing_is_clean.lock().unwrap();

    Ok(PlaybackStatusDto {
        is_playing: player.is_playing(),
        position_seconds: player.position_seconds(),
        duration_seconds: player.duration_seconds(),
        track_id: cur_track,
        is_clean,
    })
}

#[tauri::command]
pub fn process_track(
    state: State<'_, AppState>,
    id: String,
    settings: AdvancedSettingsDto,
) -> Result<TrackMetadata, String> {
    let track_dir = state.track_manager.get_track_dir(&id);
    let orig_path = track_dir.join("original.wav");
    let calib_path = track_dir.join("noise_reference.wav");
    let clean_path = track_dir.join("cleaned.wav");

    if !orig_path.exists() {
        return Err("Original recording not found".to_string());
    }

    let (orig_samples, _) =
        audio_core::read_wav_canonical_f32(&orig_path).map_err(|e| e.to_string())?;

    let calib_samples = if calib_path.exists() {
        audio_core::read_wav_canonical_f32(&calib_path)
            .ok()
            .map(|(s, _)| s)
    } else {
        None
    };

    let model = if settings.model_id == "deepfilter_net3" {
        NeuralModel::DeepFilterNet3
    } else {
        NeuralModel::Dpdfnet2_48k
    };

    let config = AutoPipelineConfig {
        preferred_neural_backend: model,
        force_neural: false,
        force_dsp_only: false,
        conservative_bias: settings.conservative_bias,
        dsp_intensity: dsp::DspIntensity::Balanced,
        adaptive_preservation: true,
        enable_declicker: settings.declicker,
        enable_plosive_filter: settings.plosive_guard,
        sample_rate: 48000,
    };

    let res = AutoPipeline::run(
        &orig_samples,
        calib_samples.as_deref(),
        &config,
        None,
        |_| {},
    )
    .map_err(|e| e.to_string())?;

    audio_core::write_wav_f32(&clean_path, &res.cleaned_samples, 48000, 1)
        .map_err(|e| e.to_string())?;

    let mut meta = state
        .track_manager
        .get_track(&id)
        .ok_or("Track metadata missing")?;

    meta.has_clean = true;
    meta.model_used = model.display_name().to_string();
    meta.clean_waveform = compute_waveform_peaks(&res.cleaned_samples, 120);

    state.track_manager.save_track(&meta)?;
    Ok(meta)
}

#[tauri::command]
pub fn get_diagnostics(
    state: State<'_, AppState>,
    id: String,
) -> Result<DiagnosticsDto, String> {
    let track_dir = state.track_manager.get_track_dir(&id);
    let orig_path = track_dir.join("original.wav");

    if !orig_path.exists() {
        return Err("Original track file not found".to_string());
    }

    let (samples, spec) =
        audio_core::read_wav_canonical_f32(&orig_path).map_err(|e| e.to_string())?;

    let analyzer = NoiseAnalyzer::new(spec.sample_rate).map_err(|e| e.to_string())?;
    let profile = analyzer.analyze_noise_reference(&samples).map_err(|e| e.to_string())?;

    let bins: Vec<f32> = profile
        .psd
        .iter()
        .take(64)
        .map(|&p| {
            let db: f32 = 10.0 * (p + 1e-12).log10();
            db.clamp(-90.0, 0.0)
        })
        .collect();

    Ok(DiagnosticsDto {
        sample_rate: spec.sample_rate,
        estimated_snr_db: 18.5,
        stationarity_score: profile.stationarity_score,
        tonal_peaks_count: profile.tonal_peaks.len(),
        peak_noise_level_dbfs: profile.noise_floor_dbfs,
        spectrum_bins: bins,
    })
}

#[tauri::command]
pub fn import_audio_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<TrackMetadata, String> {
    let src = Path::new(&file_path);
    if !src.exists() {
        return Err("File does not exist".to_string());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let track_id = format!("track_{}", now);
    let track_dir = state.track_manager.get_track_dir(&track_id);
    fs::create_dir_all(&track_dir).map_err(|e| e.to_string())?;

    let dest = track_dir.join("original.wav");
    fs::copy(src, &dest).map_err(|e| e.to_string())?;

    let (samples, spec) =
        audio_core::read_wav_canonical_f32(&dest).map_err(|e| e.to_string())?;
    let duration_secs = samples.len() as f32 / spec.sample_rate as f32;
    let mins = (duration_secs / 60.0).floor() as u32;
    let secs = (duration_secs % 60.0).floor() as u32;

    let filename = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Audio");

    let peaks = compute_waveform_peaks(&samples, 120);

    let meta = TrackMetadata {
        id: track_id.clone(),
        title: filename.to_string(),
        created_at: "Imported".to_string(),
        timestamp: now,
        duration_secs,
        formatted_duration: format!("{:02}:{:02}", mins, secs),
        is_favorite: false,
        has_raw: true,
        has_clean: false,
        model_used: "None".to_string(),
        raw_waveform: peaks.clone(),
        clean_waveform: peaks,
    };

    state.track_manager.save_track(&meta)?;
    Ok(meta)
}

#[tauri::command]
pub fn open_folder(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let track_dir = state.track_manager.get_track_dir(&id);
    let _ = std::process::Command::new("xdg-open")
        .arg(&track_dir)
        .spawn();
    Ok(())
}

#[tauri::command]
pub fn window_minimize(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.minimize();
    }
}

#[tauri::command]
pub fn window_maximize(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if let Ok(is_max) = win.is_maximized() {
            if is_max {
                let _ = win.unmaximize();
            } else {
                let _ = win.maximize();
            }
        }
    }
}

#[tauri::command]
pub fn window_close(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.close();
    }
}
