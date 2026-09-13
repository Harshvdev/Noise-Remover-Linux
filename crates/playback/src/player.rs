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
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    playhead: Arc<AtomicUsize>,
    is_playing: Arc<AtomicBool>,
    output_config: Option<StreamConfig>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            stream: None,
            samples: Arc::new(Vec::new()),
            sample_rate: 48000,
            playhead: Arc::new(AtomicUsize::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            output_config: None,
        }
    }

    /// Load audio from a WAV file for playback (downmixing multi-channel to mono).
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), PlaybackError> {
        self.stop();
        let (samples, spec) = read_wav_canonical_f32(path)?;
        self.load_samples(samples, spec.sample_rate)
    }

    /// Load raw samples directly into the player, adapting to device sample rate if needed.
    pub fn load_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<(), PlaybackError> {
        self.stop();
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

        self.samples = Arc::new(final_samples);
        self.sample_rate = output_rate;
        self.playhead.store(0, Ordering::SeqCst);
        self.setup_stream_with_device(&device, default_config)?;
        Ok(())
    }

    pub fn play(&self) {
        if !self.samples.is_empty() {
            let head = self.playhead.load(Ordering::SeqCst);
            if head >= self.samples.len() {
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
        let sample_idx = ((position_seconds * self.sample_rate as f32) as usize)
            .min(self.samples.len());
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
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    fn setup_stream_with_device(
        &mut self,
        device: &cpal::Device,
        default_config: cpal::SupportedStreamConfig,
    ) -> Result<(), PlaybackError> {
        let config: StreamConfig = default_config.into();
        let samples = self.samples.clone();
        let playhead = self.playhead.clone();
        let is_playing = self.is_playing.clone();
        let channels = config.channels as usize;

        let err_fn = |err| {
            log::error!("CPAL audio playback error: {:?}", err);
        };

        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let playing = is_playing.load(Ordering::Relaxed);
                if !playing {
                    data.fill(0.0);
                    return;
                }

                let mut head = playhead.load(Ordering::Relaxed);
                let total_samples = samples.len();

                let frames = data.len() / channels;
                for frame_idx in 0..frames {
                    if head < total_samples {
                        let sample = samples[head];
                        for ch in 0..channels {
                            data[frame_idx * channels + ch] = sample;
                        }
                        head += 1;
                    } else {
                        for ch in 0..channels {
                            data[frame_idx * channels + ch] = 0.0;
                        }
                        is_playing.store(false, Ordering::Relaxed);
                    }
                }
                playhead.store(head, Ordering::Relaxed);
            },
            err_fn,
            None,
        )?;

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
        let player = AudioPlayer {
            stream: None,
            samples: Arc::new(vec![0.1; 48000]), // 1.0 second
            sample_rate: 48000,
            playhead: Arc::new(AtomicUsize::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            output_config: None,
        };

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
}
