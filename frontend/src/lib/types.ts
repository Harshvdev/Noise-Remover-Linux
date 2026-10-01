export interface TrackMetadata {
  id: string;
  title: string;
  created_at: string;
  timestamp: number;
  duration_secs: number;
  formatted_duration: string;
  is_favorite: boolean;
  has_raw: boolean;
  has_clean: boolean;
  model_used: string;
  raw_waveform: number[];
  clean_waveform: number[];
}

export interface DeviceDto {
  index: number;
  name: string;
  is_default: boolean;
}

export interface MicStatsDto {
  peak_dbfs: number;
  rms_dbfs: number;
  has_clipped: boolean;
  mode: 'idle' | 'calibrating' | 'recording';
  recorded_seconds: number;
  calibration_progress: number;
  spectrum?: number[];
}

export interface PlaybackStatusDto {
  is_playing: boolean;
  position_seconds: number;
  duration_seconds: number;
  track_id: string | null;
  is_clean: boolean;
}

export interface AdvancedSettingsDto {
  conservative_bias: boolean;
  declicker: boolean;
  plosive_guard: boolean;
  harmonic_preservation: number; // 0.0 - 1.0
  model_id: string;
}

export interface DiagnosticsDto {
  sample_rate: number;
  estimated_snr_db: number;
  stationarity_score: number;
  tonal_peaks_count: number;
  peak_noise_level_dbfs: number;
  spectrum_bins: number[];
}

export type ViewScreen = 'home' | 'recordings' | 'track_detail';
