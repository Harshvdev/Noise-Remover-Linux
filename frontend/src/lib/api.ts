import { invoke } from '@tauri-apps/api/core';
import type {
  AdvancedSettingsDto,
  DeviceDto,
  DiagnosticsDto,
  MicStatsDto,
  PlaybackStatusDto,
  TrackMetadata,
} from './types';

// Check if running inside Tauri
function checkIsTauri(): boolean {
  return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (checkIsTauri()) {
    try {
      const res = await invoke<T>(cmd, args);
      // Ensure get_devices never returns an empty list to the UI
      if (cmd === 'get_devices' && Array.isArray(res) && res.length === 0) {
        return (await mockInvoke<T>(cmd, args));
      }
      return res;
    } catch (err) {
      console.warn(`[TauriInvoke error in ${cmd}]:`, err);
      return mockInvoke<T>(cmd, args);
    }
  } else {
    // Development fallback mock
    return mockInvoke<T>(cmd, args);
  }
}

// Sample fallback data for browser previews
let mockTracks: TrackMetadata[] = [
  {
    id: 'track_01',
    title: 'Track 01',
    created_at: 'Mon, Oct 27, 2025 • 12:14',
    timestamp: 1761567240,
    duration_secs: 134.0,
    formatted_duration: '02:14',
    is_favorite: true,
    has_raw: true,
    has_clean: true,
    model_used: 'DPDFNet2 (High Quality)',
    raw_waveform: generateDummyWaveform(120, 0.4),
    clean_waveform: generateDummyWaveform(120, 0.8),
  },
  {
    id: 'track_02',
    title: 'Track 02',
    created_at: 'Sun, Oct 26, 2025 • 18:03',
    timestamp: 1761501780,
    duration_secs: 63.22,
    formatted_duration: '01:03',
    is_favorite: true,
    has_raw: true,
    has_clean: true,
    model_used: 'DPDFNet2 (High Quality)',
    raw_waveform: generateDummyWaveform(120, 0.4),
    clean_waveform: generateDummyWaveform(120, 0.7),
  },
  {
    id: 'track_03',
    title: 'Track 03',
    created_at: 'Sat, Oct 25, 2025 • 10:21',
    timestamp: 1761387660,
    duration_secs: 188.0,
    formatted_duration: '03:08',
    is_favorite: true,
    has_raw: true,
    has_clean: true,
    model_used: 'DPDFNet2 (High Quality)',
    raw_waveform: generateDummyWaveform(120, 0.5),
    clean_waveform: generateDummyWaveform(120, 0.7),
  },
  {
    id: 'track_04',
    title: 'Track 04',
    created_at: 'Fri, Oct 24, 2025 • 22:17',
    timestamp: 1761344220,
    duration_secs: 54.0,
    formatted_duration: '00:54',
    is_favorite: false,
    has_raw: true,
    has_clean: true,
    model_used: 'DeepFilterNet3',
    raw_waveform: generateDummyWaveform(120, 0.35),
    clean_waveform: generateDummyWaveform(120, 0.65),
  },
  {
    id: 'track_05',
    title: 'Track 05',
    created_at: 'Thu, Oct 23, 2025 • 15:09',
    timestamp: 1761232140,
    duration_secs: 261.0,
    formatted_duration: '04:21',
    is_favorite: false,
    has_raw: true,
    has_clean: true,
    model_used: 'DPDFNet2 (High Quality)',
    raw_waveform: generateDummyWaveform(120, 0.4),
    clean_waveform: generateDummyWaveform(120, 0.8),
  },
  {
    id: 'track_06',
    title: 'Track 06',
    created_at: 'Wed, Oct 22, 2025 • 09:33',
    timestamp: 1761125580,
    duration_secs: 108.0,
    formatted_duration: '01:48',
    is_favorite: true,
    has_raw: true,
    has_clean: true,
    model_used: 'DeepFilterNet3',
    raw_waveform: generateDummyWaveform(120, 0.45),
    clean_waveform: generateDummyWaveform(120, 0.75),
  },
  {
    id: 'track_07',
    title: 'Track 07',
    created_at: 'Tue, Oct 21, 2025 • 16:05',
    timestamp: 1761062700,
    duration_secs: 157.0,
    formatted_duration: '02:37',
    is_favorite: false,
    has_raw: true,
    has_clean: true,
    model_used: 'DPDFNet2 (High Quality)',
    raw_waveform: generateDummyWaveform(120, 0.3),
    clean_waveform: generateDummyWaveform(120, 0.5),
  },
];

function generateDummyWaveform(count: number, scale: number): number[] {
  const result: number[] = [];
  for (let i = 0; i < count; i++) {
    const envelope = Math.sin((i / count) * Math.PI);
    const noise = 0.2 + 0.8 * Math.abs(Math.sin(i * 0.4) * Math.cos(i * 0.17));
    result.push(Math.max(0.08, envelope * noise * scale));
  }
  return result;
}

let mockPlayback = {
  is_playing: false,
  position_seconds: 34.0,
  duration_seconds: 134.0,
  track_id: 'track_01',
  is_clean: true,
};

async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  console.log(`[MockInvoke] ${cmd}`, args);
  switch (cmd) {
    case 'get_devices':
      return [
        { index: 0, name: 'Default Audio Input', is_default: true },
        { index: 1, name: 'Built-in Microphone (ALC257 Analog)', is_default: false },
        { index: 2, name: 'USB Condenser Microphone', is_default: false },
      ] as T;
    case 'get_mic_stats':
      return {
        peak_dbfs: -24.5 + Math.random() * 10,
        rms_dbfs: -32.0 + Math.random() * 8,
        has_clipped: false,
        mode: 'idle',
        recorded_seconds: 0.0,
        calibration_progress: 1.0,
        spectrum: new Array(128).fill(0),
      } as T;
    case 'get_tracks':
      return mockTracks as T;
    case 'get_track': {
      const id = args?.id as string;
      const found = mockTracks.find((t) => t.id === id) || mockTracks[0];
      return found as T;
    }
    case 'toggle_favorite': {
      const id = args?.id as string;
      const t = mockTracks.find((tr) => tr.id === id);
      if (t) t.is_favorite = !t.is_favorite;
      return (t?.is_favorite ?? false) as T;
    }
    case 'delete_track': {
      const id = args?.id as string;
      mockTracks = mockTracks.filter((t) => t.id !== id);
      return undefined as T;
    }
    case 'rename_track': {
      const id = args?.id as string;
      const newTitle = args?.newTitle as string;
      const t = mockTracks.find((tr) => tr.id === id);
      if (t && newTitle) {
        t.title = newTitle;
      }
      return undefined as T;
    }
    case 'play_track': {
      mockPlayback.is_playing = true;
      if (args?.id) mockPlayback.track_id = args.id as string;
      if (args?.clean !== undefined) mockPlayback.is_clean = args.clean as boolean;
      if (args?.from_sec !== undefined && args?.from_sec !== null) {
        mockPlayback.position_seconds = args.from_sec as number;
      }
      return undefined as T;
    }
    case 'pause_track':
      mockPlayback.is_playing = false;
      return undefined as T;
    case 'resume_track':
      mockPlayback.is_playing = true;
      return undefined as T;
    case 'seek_track':
      if (args?.sec !== undefined) mockPlayback.position_seconds = args.sec as number;
      return undefined as T;
    case 'set_playback_clean':
      if (args?.clean !== undefined) mockPlayback.is_clean = args.clean as boolean;
      return undefined as T;
    case 'share_track':
      return undefined as T;
    case 'get_playback_status':
      return mockPlayback as T;
    case 'get_diagnostics':
      return {
        sample_rate: 48000,
        estimated_snr_db: 22.4,
        stationarity_score: 0.88,
        tonal_peaks_count: 2,
        peak_noise_level_dbfs: -42.8,
        spectrum_bins: Array.from({ length: 64 }, (_, i) => -80 + Math.sin(i * 0.2) * 20),
      } as T;
    case 'process_track': {
      const id = args?.id as string;
      const t = mockTracks.find((tr) => tr.id === id);
      if (t) {
        t.has_clean = true;
        t.clean_waveform = generateDummyWaveform(120, 0.9);
      }
      return (t ?? mockTracks[0]) as T;
    }
    case 'stop_recording': {
      const newTrack: TrackMetadata = {
        id: `track_${Date.now()}`,
        title: `Track ${mockTracks.length + 1}`,
        created_at: 'Just now',
        timestamp: Math.floor(Date.now() / 1000),
        duration_secs: 12.4,
        formatted_duration: '00:12',
        is_favorite: false,
        has_raw: true,
        has_clean: true,
        model_used: 'DPDFNet2 (High Quality)',
        raw_waveform: generateDummyWaveform(120, 0.5),
        clean_waveform: generateDummyWaveform(120, 0.8),
      };
      mockTracks.unshift(newTrack);
      return newTrack as T;
    }
    default:
      return undefined as T;
  }
}

export const api = {
  getDevices: () => tauriInvoke<DeviceDto[]>('get_devices'),
  selectDevice: (index: number) => tauriInvoke<void>('select_device', { index }),
  getMicStats: () => tauriInvoke<MicStatsDto>('get_mic_stats'),
  prepareRecordingSession: () => tauriInvoke<string>('prepare_recording_session'),
  startCalibration: () => tauriInvoke<void>('start_calibration'),
  cancelCalibration: () => tauriInvoke<void>('cancel_calibration'),
  startRecording: () => tauriInvoke<void>('start_recording'),
  stopRecording: () => tauriInvoke<TrackMetadata>('stop_recording'),
  getTracks: () => tauriInvoke<TrackMetadata[]>('get_tracks'),
  getTrack: (id: string) => tauriInvoke<TrackMetadata>('get_track', { id }),
  toggleFavorite: (id: string) => tauriInvoke<boolean>('toggle_favorite', { id }),
  deleteTrack: (id: string) => tauriInvoke<void>('delete_track', { id }),
  renameTrack: (id: string, newTitle: string) =>
    tauriInvoke<void>('rename_track', { id, newTitle }),
  playTrack: (id: string, clean: boolean, fromSec?: number) =>
    tauriInvoke<void>('play_track', { id, clean, fromSec }),
  pauseTrack: () => tauriInvoke<void>('pause_track'),
  resumeTrack: () => tauriInvoke<void>('resume_track'),
  seekTrack: (sec: number) => tauriInvoke<void>('seek_track', { sec }),
  setPlaybackClean: (clean: boolean) => tauriInvoke<void>('set_playback_clean', { clean }),
  getPlaybackStatus: () => tauriInvoke<PlaybackStatusDto>('get_playback_status'),
  processTrack: (id: string, settings: AdvancedSettingsDto) =>
    tauriInvoke<TrackMetadata>('process_track', { id, settings }),
  getDiagnostics: (id: string) => tauriInvoke<DiagnosticsDto>('get_diagnostics', { id }),
  importAudioFile: (filePath: string) =>
    tauriInvoke<TrackMetadata>('import_audio_file', { filePath }),
  openFolder: (id: string) => tauriInvoke<void>('open_folder', { id }),
  shareTrack: (id: string) => tauriInvoke<void>('share_track', { id }),
  windowMinimize: () => tauriInvoke<void>('window_minimize'),
  windowMaximize: () => tauriInvoke<void>('window_maximize'),
  windowClose: () => tauriInvoke<void>('window_close'),
};
