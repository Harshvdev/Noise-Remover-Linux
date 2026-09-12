//! Audio player with waveform access and atomic playhead position.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use audio_core::read_wav_f32;
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

    /// Load audio from a WAV file for playback.
    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), PlaybackError> {
        self.stop();
        let (samples, spec) = read_wav_f32(path)?;
        self.samples = Arc::new(samples);
        self.sample_rate = spec.sample_rate;
        self.playhead.store(0, Ordering::SeqCst);
        self.setup_stream()?;
        Ok(())
    }

    /// Load raw samples directly into the player.
    pub fn load_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<(), PlaybackError> {
        self.stop();
        self.samples = Arc::new(samples);
        self.sample_rate = sample_rate;
        self.playhead.store(0, Ordering::SeqCst);
        self.setup_stream()?;
        Ok(())
    }

    pub fn play(&self) {
        if !self.samples.is_empty() {
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

    fn setup_stream(&mut self) -> Result<(), PlaybackError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlaybackError::NoOutputDevice)?;

        let default_config = device.default_output_config()?;
        let mut config: StreamConfig = default_config.into();
        config.sample_rate = self.sample_rate;

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
