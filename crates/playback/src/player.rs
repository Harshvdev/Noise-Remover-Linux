//! Audio player with waveform access and atomic playhead position.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use audio_core::read_wav_canonical_f32;
use crate::error::PlaybackError;

pub struct AudioPlayer {
    stream: Option<Stream>,
    samples: Arc<std::sync::RwLock<Arc<Vec<f32>>>>,
    cached_samples: Arc<Vec<f32>>,
    sample_rate: u32,
    playhead: Arc<AtomicUsize>,
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
            playhead: Arc::new(AtomicUsize::new(0)),
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
            playhead: Arc::new(AtomicUsize::new(0)),
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
                self.playhead.store(0, Ordering::SeqCst);
                self.is_playing.store(false, Ordering::SeqCst);
            } else if cur_head >= new_len {
                self.playhead.store(new_len, Ordering::SeqCst);
            }
            return Ok(());
        }

        self.stop();
        *self.samples.write().unwrap() = arc_samples.clone();
        self.cached_samples = arc_samples;
        self.sample_rate = output_rate;
        self.playhead.store(0, Ordering::SeqCst);
        self.setup_stream_with_device(&device, default_config)?;
        Ok(())
    }

    pub fn play(&self) {
        if !self.cached_samples.is_empty() {
            let head = self.playhead.load(Ordering::SeqCst);
            if head >= self.cached_samples.len() {
                self.playhead.store(0, Ordering::SeqCst);
            }
            self.is_playing.store(true, Ordering::SeqCst);
        }
    }

    pub fn pause(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.playhead.store(0, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self, position_seconds: f32) {
        let sample_idx = ((position_seconds.max(0.0) * self.sample_rate as f32) as usize)
            .min(self.cached_samples.len());
        self.playhead.store(sample_idx, Ordering::SeqCst);
    }

    pub fn position_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.playhead.load(Ordering::Relaxed) as f32 / self.sample_rate as f32
        }
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
        let config: StreamConfig = default_config.into();
        let samples_lock = self.samples.clone();
        let playhead = self.playhead.clone();
        let is_playing = self.is_playing.clone();
        let channels = config.channels.max(1) as usize;

        let err_fn = |err| {
            log::error!("CPAL audio playback error: {:?}", err);
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    data.fill(0.0);
                    if !is_playing.load(Ordering::Relaxed) {
                        return;
                    }
                    let samples = match samples_lock.read() {
                        Ok(s) => s.clone(),
                        Err(_) => return,
                    };
                    let total_samples = samples.len();
                    let mut head = playhead.load(Ordering::Relaxed);
                    let frames = data.len() / channels;

                    for frame_idx in 0..frames {
                        if head < total_samples {
                            let s = samples[head];
                            for ch in 0..channels {
                                data[frame_idx * channels + ch] = s;
                            }
                            head += 1;
                        } else {
                            is_playing.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                    playhead.store(head, Ordering::Relaxed);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    data.fill(0);
                    if !is_playing.load(Ordering::Relaxed) {
                        return;
                    }
                    let samples = match samples_lock.read() {
                        Ok(s) => s.clone(),
                        Err(_) => return,
                    };
                    let total_samples = samples.len();
                    let mut head = playhead.load(Ordering::Relaxed);
                    let frames = data.len() / channels;

                    for frame_idx in 0..frames {
                        if head < total_samples {
                            let s = audio_core::pcm::f32_to_i16(samples[head]);
                            for ch in 0..channels {
                                data[frame_idx * channels + ch] = s;
                            }
                            head += 1;
                        } else {
                            is_playing.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                    playhead.store(head, Ordering::Relaxed);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_output_stream(
                config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    data.fill(32768);
                    if !is_playing.load(Ordering::Relaxed) {
                        return;
                    }
                    let samples = match samples_lock.read() {
                        Ok(s) => s.clone(),
                        Err(_) => return,
                    };
                    let total_samples = samples.len();
                    let mut head = playhead.load(Ordering::Relaxed);
                    let frames = data.len() / channels;

                    for frame_idx in 0..frames {
                        if head < total_samples {
                            let i16_s = audio_core::pcm::f32_to_i16(samples[head]);
                            let s = (i16_s as i32 + 32768) as u16;
                            for ch in 0..channels {
                                data[frame_idx * channels + ch] = s;
                            }
                            head += 1;
                        } else {
                            is_playing.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                    playhead.store(head, Ordering::Relaxed);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I32 => device.build_output_stream(
                config,
                move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                    data.fill(0);
                    if !is_playing.load(Ordering::Relaxed) {
                        return;
                    }
                    let samples = match samples_lock.read() {
                        Ok(s) => s.clone(),
                        Err(_) => return,
                    };
                    let total_samples = samples.len();
                    let mut head = playhead.load(Ordering::Relaxed);
                    let frames = data.len() / channels;

                    for frame_idx in 0..frames {
                        if head < total_samples {
                            let s = (samples[head].clamp(-1.0, 1.0) * 2147483647.0) as i32;
                            for ch in 0..channels {
                                data[frame_idx * channels + ch] = s;
                            }
                            head += 1;
                        } else {
                            is_playing.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                    playhead.store(head, Ordering::Relaxed);
                },
                err_fn,
                None,
            )?,
            _ => return Err(PlaybackError::NoSupportedConfig),
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
}
