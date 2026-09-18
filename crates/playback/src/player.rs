//! Audio player with waveform access and atomic playhead position.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use audio_core::read_wav_canonical_f32;
use crate::error::PlaybackError;

pub struct AudioPlayer {
    stream: Option<Stream>,
    samples: Arc<std::sync::RwLock<Arc<Vec<f32>>>>,
    cached_samples: Arc<Vec<f32>>,
    sample_rate: u32,
    write_head: Arc<AtomicUsize>,
    seek_anchor: Arc<AtomicUsize>,
    playhead: Arc<AtomicUsize>, // Audible head (calibrated to DAC output)
    last_sync: Arc<Mutex<Option<Instant>>>,
    is_playing: Arc<AtomicBool>,
    output_config: Option<StreamConfig>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let empty = Arc::new(Vec::new());
        Self {
            stream: None,
            samples: Arc::new(std::sync::RwLock::new(empty.clone())),
            cached_samples: empty,
            sample_rate: 48000,
            write_head: Arc::new(AtomicUsize::new(0)),
            seek_anchor: Arc::new(AtomicUsize::new(0)),
            playhead: Arc::new(AtomicUsize::new(0)),
            last_sync: Arc::new(Mutex::new(None)),
            is_playing: Arc::new(AtomicBool::new(false)),
            output_config: None,
        }
    }

    pub fn with_samples(samples: Vec<f32>, sample_rate: u32) -> Self {
        let arc_samples = Arc::new(samples);
        Self {
            stream: None,
            samples: Arc::new(std::sync::RwLock::new(arc_samples.clone())),
            cached_samples: arc_samples,
            sample_rate,
            write_head: Arc::new(AtomicUsize::new(0)),
            seek_anchor: Arc::new(AtomicUsize::new(0)),
            playhead: Arc::new(AtomicUsize::new(0)),
            last_sync: Arc::new(Mutex::new(None)),
            is_playing: Arc::new(AtomicBool::new(false)),
            output_config: None,
        }
    }

    /// Load audio from a WAV file for playback (downmixing multi-channel to mono).
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), PlaybackError> {
        self.load_file_preserve_position(path, false)
    }

    /// Load audio from a WAV file, optionally preserving playhead position and playback state for seamless A/B comparison.
    pub fn load_file_preserve_position<P: AsRef<Path>>(
        &mut self,
        path: P,
        preserve_head: bool,
    ) -> Result<(), PlaybackError> {
        let (samples, spec) = read_wav_canonical_f32(path)?;
        self.load_samples_preserve_position(samples, spec.sample_rate, preserve_head)
    }

    /// Load raw samples directly into the player, adapting to device sample rate if needed.
    pub fn load_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<(), PlaybackError> {
        self.load_samples_preserve_position(samples, sample_rate, false)
    }

    /// Load raw samples directly into the player, optionally preserving playhead position and playback state.
    pub fn load_samples_preserve_position(
        &mut self,
        samples: Vec<f32>,
        sample_rate: u32,
        preserve_head: bool,
    ) -> Result<(), PlaybackError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlaybackError::NoOutputDevice)?;
        let default_config = device.default_output_config()?;
        let output_rate = default_config.sample_rate();

        let final_samples = if sample_rate != output_rate {
            audio_core::resample_mono(&samples, sample_rate, output_rate)
                .unwrap_or(samples)
        } else {
            samples
        };

        let new_len = final_samples.len();
        let arc_samples = Arc::new(final_samples);

        // If a stream is already active and we want to preserve position for A/B comparison:
        if preserve_head && self.stream.is_some() && self.sample_rate == output_rate {
            *self.samples.write().unwrap() = arc_samples.clone();
            self.cached_samples = arc_samples;

            let cur_head = self.playhead.load(Ordering::SeqCst);
            if new_len == 0 {
                self.write_head.store(0, Ordering::SeqCst);
                self.seek_anchor.store(0, Ordering::SeqCst);
                self.playhead.store(0, Ordering::SeqCst);
                self.is_playing.store(false, Ordering::SeqCst);
            } else if cur_head >= new_len {
                self.write_head.store(new_len, Ordering::SeqCst);
                self.seek_anchor.store(new_len, Ordering::SeqCst);
                self.playhead.store(new_len, Ordering::SeqCst);
            } else {
                self.write_head.store(cur_head, Ordering::SeqCst);
                self.seek_anchor.store(cur_head, Ordering::SeqCst);
            }
            if let Ok(mut sync) = self.last_sync.lock() {
                *sync = Some(Instant::now());
            }
            return Ok(());
        }

        self.stop();
        *self.samples.write().unwrap() = arc_samples.clone();
        self.cached_samples = arc_samples;
        self.sample_rate = output_rate;
        self.write_head.store(0, Ordering::SeqCst);
        self.seek_anchor.store(0, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
        self.setup_stream_with_device(&device, default_config)?;
        Ok(())
    }

    pub fn play(&self) {
        if !self.cached_samples.is_empty() {
            let aud = self.playhead.load(Ordering::SeqCst);
            let total = self.cached_samples.len();
            let start_pos = if aud >= total { 0 } else { aud };
            self.write_head.store(start_pos, Ordering::SeqCst);
            self.seek_anchor.store(start_pos, Ordering::SeqCst);
            self.playhead.store(start_pos, Ordering::SeqCst);
            if let Ok(mut sync) = self.last_sync.lock() {
                *sync = Some(Instant::now());
            }
            self.is_playing.store(true, Ordering::SeqCst);
        }
    }

    pub fn pause(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
        let cur_audible = self.audible_position_samples();
        self.write_head.store(cur_audible, Ordering::SeqCst);
        self.seek_anchor.store(cur_audible, Ordering::SeqCst);
        self.playhead.store(cur_audible, Ordering::SeqCst);
        if let Ok(mut sync) = self.last_sync.lock() {
            *sync = None;
        }
    }

    pub fn stop(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.write_head.store(0, Ordering::SeqCst);
        self.seek_anchor.store(0, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
        if let Ok(mut sync) = self.last_sync.lock() {
            *sync = None;
        }
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self, position_seconds: f32) {
        let sample_idx = ((position_seconds.max(0.0) * self.sample_rate as f32) as usize)
            .min(self.cached_samples.len());
        self.write_head.store(sample_idx, Ordering::SeqCst);
        self.seek_anchor.store(sample_idx, Ordering::SeqCst);
        self.playhead.store(sample_idx, Ordering::SeqCst);
        if let Ok(mut sync) = self.last_sync.lock() {
            *sync = Some(Instant::now());
        }
    }

    pub fn position_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.audible_position_samples() as f32 / self.sample_rate as f32
        }
    }

    pub fn audible_position_samples(&self) -> usize {
        if !self.is_playing.load(Ordering::Relaxed) {
            return self.playhead.load(Ordering::Relaxed);
        }

        let base = self.playhead.load(Ordering::Relaxed);
        let anchor = self.seek_anchor.load(Ordering::Relaxed);
        let max_samples = self.cached_samples.len();

        // While audio hardware buffer is initialising output up to DAC, hold steady at anchor
        if base <= anchor {
            return anchor.min(max_samples);
        }

        let elapsed_samples = self
            .last_sync
            .lock()
            .ok()
            .and_then(|sync| {
                sync.map(|t| (t.elapsed().as_secs_f64() * self.sample_rate as f64).round() as usize)
            })
            .unwrap_or(0);

        (base + elapsed_samples).min(max_samples)
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.cached_samples.len() as f32 / self.sample_rate as f32
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.cached_samples
    }

    fn setup_stream_with_device(
        &mut self,
        device: &cpal::Device,
        default_config: cpal::SupportedStreamConfig,
    ) -> Result<(), PlaybackError> {
        let sample_format = default_config.sample_format();
        let mut config: StreamConfig = default_config.into();
        // Request low-latency buffer (512 frames = ~10.6ms at 48kHz)
        config.buffer_size = cpal::BufferSize::Fixed(512);

        let samples_lock = self.samples.clone();
        let write_head = self.write_head.clone();
        let seek_anchor = self.seek_anchor.clone();
        let playhead = self.playhead.clone();
        let last_sync = self.last_sync.clone();
        let is_playing = self.is_playing.clone();
        let channels = config.channels.max(1) as usize;
        let sample_rate = config.sample_rate;

        let err_fn = |err| {
            log::error!("CPAL audio playback error: {:?}", err);
        };

        let stream_res = match sample_format {
            cpal::SampleFormat::F32 => {
                let w_lock = samples_lock.clone();
                let w_head_arc = write_head.clone();
                let anchor_arc = seek_anchor.clone();
                let p_head_arc = playhead.clone();
                let sync_arc = last_sync.clone();
                let play_arc = is_playing.clone();

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                        data.fill(0.0);
                        if !play_arc.load(Ordering::Relaxed) {
                            return;
                        }
                        let samples = match w_lock.read() {
                            Ok(s) => s.clone(),
                            Err(_) => return,
                        };
                        let total_samples = samples.len();
                        let mut w_head = w_head_arc.load(Ordering::Relaxed);
                        let anchor = anchor_arc.load(Ordering::Relaxed);
                        let frames = data.len() / channels;

                        let latency_frames = {
                            let cb = info.timestamp().callback;
                            let pb = info.timestamp().playback;
                            let dur = pb.duration_since(cb);
                            (dur.as_secs_f64() * sample_rate as f64).round() as usize
                        };

                        for frame_idx in 0..frames {
                            if w_head < total_samples {
                                let s = samples[w_head];
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = s;
                                }
                                w_head += 1;
                            } else {
                                // Flush out tail buffer with silence
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = 0.0;
                                }
                                w_head += 1;
                            }
                        }

                        w_head_arc.store(w_head, Ordering::Relaxed);

                        let in_flight = frames + latency_frames;
                        let written_since_anchor = w_head.saturating_sub(anchor);
                        let audible = if written_since_anchor <= in_flight {
                            anchor
                        } else {
                            anchor + (written_since_anchor - in_flight)
                        };

                        p_head_arc.store(audible, Ordering::Relaxed);
                        if audible > anchor {
                            if let Ok(mut sync) = sync_arc.lock() {
                                *sync = Some(Instant::now());
                            }
                        }

                        if audible >= total_samples {
                            play_arc.store(false, Ordering::Relaxed);
                            p_head_arc.store(total_samples, Ordering::Relaxed);
                            w_head_arc.store(0, Ordering::Relaxed);
                            anchor_arc.store(0, Ordering::Relaxed);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let w_lock = samples_lock.clone();
                let w_head_arc = write_head.clone();
                let anchor_arc = seek_anchor.clone();
                let p_head_arc = playhead.clone();
                let sync_arc = last_sync.clone();
                let play_arc = is_playing.clone();

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [i16], info: &cpal::OutputCallbackInfo| {
                        data.fill(0);
                        if !play_arc.load(Ordering::Relaxed) {
                            return;
                        }
                        let samples = match w_lock.read() {
                            Ok(s) => s.clone(),
                            Err(_) => return,
                        };
                        let total_samples = samples.len();
                        let mut w_head = w_head_arc.load(Ordering::Relaxed);
                        let anchor = anchor_arc.load(Ordering::Relaxed);
                        let frames = data.len() / channels;

                        let latency_frames = {
                            let cb = info.timestamp().callback;
                            let pb = info.timestamp().playback;
                            let dur = pb.duration_since(cb);
                            (dur.as_secs_f64() * sample_rate as f64).round() as usize
                        };

                        for frame_idx in 0..frames {
                            if w_head < total_samples {
                                let s = audio_core::pcm::f32_to_i16(samples[w_head]);
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = s;
                                }
                                w_head += 1;
                            } else {
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = 0;
                                }
                                w_head += 1;
                            }
                        }

                        w_head_arc.store(w_head, Ordering::Relaxed);

                        let in_flight = frames + latency_frames;
                        let written_since_anchor = w_head.saturating_sub(anchor);
                        let audible = if written_since_anchor <= in_flight {
                            anchor
                        } else {
                            anchor + (written_since_anchor - in_flight)
                        };

                        p_head_arc.store(audible, Ordering::Relaxed);
                        if audible > anchor {
                            if let Ok(mut sync) = sync_arc.lock() {
                                *sync = Some(Instant::now());
                            }
                        }

                        if audible >= total_samples {
                            play_arc.store(false, Ordering::Relaxed);
                            p_head_arc.store(total_samples, Ordering::Relaxed);
                            w_head_arc.store(0, Ordering::Relaxed);
                            anchor_arc.store(0, Ordering::Relaxed);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let w_lock = samples_lock.clone();
                let w_head_arc = write_head.clone();
                let anchor_arc = seek_anchor.clone();
                let p_head_arc = playhead.clone();
                let sync_arc = last_sync.clone();
                let play_arc = is_playing.clone();

                device.build_output_stream(
                    config.clone(),
                    move |data: &mut [u16], info: &cpal::OutputCallbackInfo| {
                        data.fill(32768);
                        if !play_arc.load(Ordering::Relaxed) {
                            return;
                        }
                        let samples = match w_lock.read() {
                            Ok(s) => s.clone(),
                            Err(_) => return,
                        };
                        let total_samples = samples.len();
                        let mut w_head = w_head_arc.load(Ordering::Relaxed);
                        let anchor = anchor_arc.load(Ordering::Relaxed);
                        let frames = data.len() / channels;

                        let latency_frames = {
                            let cb = info.timestamp().callback;
                            let pb = info.timestamp().playback;
                            let dur = pb.duration_since(cb);
                            (dur.as_secs_f64() * sample_rate as f64).round() as usize
                        };

                        for frame_idx in 0..frames {
                            if w_head < total_samples {
                                let s = audio_core::pcm::f32_to_i16(samples[w_head]);
                                let u_val = (s as i32 + 32768) as u16;
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = u_val;
                                }
                                w_head += 1;
                            } else {
                                for ch in 0..channels {
                                    data[frame_idx * channels + ch] = 32768;
                                }
                                w_head += 1;
                            }
                        }

                        w_head_arc.store(w_head, Ordering::Relaxed);

                        let in_flight = frames + latency_frames;
                        let written_since_anchor = w_head.saturating_sub(anchor);
                        let audible = if written_since_anchor <= in_flight {
                            anchor
                        } else {
                            anchor + (written_since_anchor - in_flight)
                        };

                        p_head_arc.store(audible, Ordering::Relaxed);
                        if audible > anchor {
                            if let Ok(mut sync) = sync_arc.lock() {
                                *sync = Some(Instant::now());
                            }
                        }

                        if audible >= total_samples {
                            play_arc.store(false, Ordering::Relaxed);
                            p_head_arc.store(total_samples, Ordering::Relaxed);
                            w_head_arc.store(0, Ordering::Relaxed);
                            anchor_arc.store(0, Ordering::Relaxed);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => return Err(PlaybackError::NoSupportedConfig),
        };

        // If low-latency 512 buffer failed, fallback to default buffer size
        let stream = match stream_res {
            Ok(s) => s,
            Err(_) => {
                config.buffer_size = cpal::BufferSize::Default;
                // Re-attempt with default buffer
                match sample_format {
                    cpal::SampleFormat::F32 => {
                        let w_lock = samples_lock.clone();
                        let w_head_arc = write_head.clone();
                        let anchor_arc = seek_anchor.clone();
                        let p_head_arc = playhead.clone();
                        let sync_arc = last_sync.clone();
                        let play_arc = is_playing.clone();

                        device.build_output_stream(
                            config.clone(),
                            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                                data.fill(0.0);
                                if !play_arc.load(Ordering::Relaxed) {
                                    return;
                                }
                                let samples = match w_lock.read() {
                                    Ok(s) => s.clone(),
                                    Err(_) => return,
                                };
                                let total_samples = samples.len();
                                let mut w_head = w_head_arc.load(Ordering::Relaxed);
                                let anchor = anchor_arc.load(Ordering::Relaxed);
                                let frames = data.len() / channels;

                                let latency_frames = {
                                    let cb = info.timestamp().callback;
                                    let pb = info.timestamp().playback;
                                    let dur = pb.duration_since(cb);
                                    (dur.as_secs_f64() * sample_rate as f64).round() as usize
                                };

                                for frame_idx in 0..frames {
                                    if w_head < total_samples {
                                        let s = samples[w_head];
                                        for ch in 0..channels {
                                            data[frame_idx * channels + ch] = s;
                                        }
                                        w_head += 1;
                                    } else {
                                        for ch in 0..channels {
                                            data[frame_idx * channels + ch] = 0.0;
                                        }
                                        w_head += 1;
                                    }
                                }

                                w_head_arc.store(w_head, Ordering::Relaxed);

                                let in_flight = frames + latency_frames;
                                let written_since_anchor = w_head.saturating_sub(anchor);
                                let audible = if written_since_anchor <= in_flight {
                                    anchor
                                } else {
                                    anchor + (written_since_anchor - in_flight)
                                };

                                p_head_arc.store(audible, Ordering::Relaxed);
                                if audible > anchor {
                                    if let Ok(mut sync) = sync_arc.lock() {
                                        *sync = Some(Instant::now());
                                    }
                                }

                                if audible >= total_samples {
                                    play_arc.store(false, Ordering::Relaxed);
                                    p_head_arc.store(total_samples, Ordering::Relaxed);
                                    w_head_arc.store(0, Ordering::Relaxed);
                                    anchor_arc.store(0, Ordering::Relaxed);
                                }
                            },
                            err_fn,
                            None,
                        )?
                    }
                    _ => return Err(PlaybackError::NoSupportedConfig),
                }
            }
        };

        stream.play()?;
        self.stream = Some(stream);
        self.output_config = Some(config);

        Ok(())
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_initial_state() {
        let player = AudioPlayer::new();
        assert!(!player.is_playing());
        assert_eq!(player.position_seconds(), 0.0);
        assert_eq!(player.duration_seconds(), 0.0);
        assert!(player.samples().is_empty());
    }

    #[test]
    fn test_player_seek_and_replay() {
        let player = AudioPlayer::with_samples(vec![0.1; 48000], 48000);

        assert_eq!(player.duration_seconds(), 1.0);
        player.seek(0.5);
        assert!((player.position_seconds() - 0.5).abs() < 1e-4);

        // Simulate reaching EOF
        player.seek(1.0);
        assert_eq!(player.playhead.load(Ordering::SeqCst), 48000);

        // Triggering play at EOF should reset playhead to 0
        player.play();
        assert_eq!(player.playhead.load(Ordering::SeqCst), 0);
        assert!(player.is_playing());

        player.stop();
        assert!(!player.is_playing());
        assert_eq!(player.playhead.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_player_preserve_position_on_switch() {
        let mut player = AudioPlayer::with_samples(vec![0.1; 48000], 48000);
        player.seek(0.5);
        player.play();
        assert!(player.is_playing());
        assert_eq!(player.playhead.load(Ordering::SeqCst), 24000);

        // Switch to a new 48kHz audio buffer of identical duration (e.g. Cleaned audio)
        let new_samples = vec![0.05; 48000];
        let arc_samples = Arc::new(new_samples);
        *player.samples.write().unwrap() = arc_samples.clone();
        player.cached_samples = arc_samples;

        // Position and playing state must remain intact
        assert_eq!(player.playhead.load(Ordering::SeqCst), 24000);
        assert!(player.is_playing());
        assert_eq!(player.samples()[0], 0.05);
    }

    #[test]
    fn test_player_real_playback_sync() {
        let mut player = AudioPlayer::with_samples(vec![0.0; 48000 * 2], 48000);
        let res = player.load_samples(vec![0.0; 48000 * 2], 48000);
        if res.is_ok() {
            player.play();
            std::thread::sleep(std::time::Duration::from_millis(50));
            // Should be bounded by duration
            assert!(player.position_seconds() <= player.duration_seconds());
            player.pause();
            assert!(!player.is_playing());
            player.seek(0.5);
            assert!((player.position_seconds() - 0.5).abs() < 1e-3);
            player.stop();
            assert_eq!(player.position_seconds(), 0.0);
        }
    }
}
