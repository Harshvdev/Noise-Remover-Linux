use egui::{Color32, Pos2, Stroke, Ui, Vec2};

pub struct WaveformRenderer {
    peaks: Vec<(f32, f32)>, // (min, max) per bucket
    sample_count: usize,
    target_buckets: usize,
    fingerprint: u64,
}

fn compute_fingerprint(samples: &[f32]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut hash = (samples.len() as u64).wrapping_mul(0x517cc1b727220a95);
    // Probe 16 evenly spaced samples across the buffer
    let stride = (samples.len() / 16).max(1);
    let mut i = 0;
    while i < samples.len() {
        let bits = samples[i].to_bits() as u64;
        hash = hash.rotate_left(5) ^ bits;
        hash = hash.wrapping_mul(0x9e3779b97f4a7c15);
        i += stride;
    }
    hash
}

impl WaveformRenderer {
    pub fn new() -> Self {
        Self {
            peaks: Vec::new(),
            sample_count: 0,
            target_buckets: 0,
            fingerprint: 0,
        }
    }

    /// Check if waveform is currently empty.
    pub fn is_empty(&self) -> bool {
        self.peaks.is_empty()
    }

    /// Invalidate and clear cached waveform peaks.
    pub fn clear(&mut self) {
        self.peaks.clear();
        self.sample_count = 0;
        self.target_buckets = 0;
        self.fingerprint = 0;
    }

    /// Recompute peak buckets from audio samples if the buffer content or length changed.
    pub fn update(&mut self, samples: &[f32], target_buckets: usize) {
        let fp = compute_fingerprint(samples);
        if samples.len() == self.sample_count && self.target_buckets == target_buckets && self.fingerprint == fp {
            return;
        }

        self.sample_count = samples.len();
        self.target_buckets = target_buckets;
        self.fingerprint = fp;
        self.peaks.clear();

        if samples.is_empty() || target_buckets == 0 {
            return;
        }

        let chunk_size = (samples.len() as f32 / target_buckets as f32).max(1.0);
        for i in 0..target_buckets {
            let start = (i as f32 * chunk_size) as usize;
            let end = (((i + 1) as f32 * chunk_size) as usize).min(samples.len());

            if start >= samples.len() {
                break;
            }

            let slice = &samples[start..end];
            let mut min_val = 0.0f32;
            let mut max_val = 0.0f32;

            for &s in slice {
                if s < min_val {
                    min_val = s;
                }
                if s > max_val {
                    max_val = s;
                }
            }

            self.peaks.push((min_val, max_val));
        }
    }

    /// Draw waveform inside the given Ui area with interactive click-and-drag scrubbing.
    pub fn show(&self, ui: &mut Ui, playhead_progress: f32, height: f32) -> egui::Response {
        let desired_size = Vec2::new(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Background with subtle border
            painter.rect_filled(rect, 6.0, Color32::from_rgb(15, 23, 42));
            painter.rect_stroke(
                rect,
                6.0,
                Stroke::new(1.0_f32, Color32::from_rgb(30, 41, 59)),
                egui::StrokeKind::Inside,
            );

            // Center line
            let mid_y = rect.center().y;
            painter.line_segment(
                [Pos2::new(rect.left(), mid_y), Pos2::new(rect.right(), mid_y)],
                Stroke::new(1.0_f32, Color32::from_rgb(51, 65, 85)),
            );

            if !self.peaks.is_empty() {
                let bucket_width = rect.width() / self.peaks.len() as f32;
                let half_height = rect.height() * 0.44;
                let mut shapes = Vec::with_capacity(self.peaks.len() + 1);

                // Auto-scale peak gain so low-level recordings remain visually legible
                let max_peak = self
                    .peaks
                    .iter()
                    .map(|&(min, max)| min.abs().max(max.abs()))
                    .fold(0.0f32, f32::max);
                let gain = if max_peak > 0.001 {
                    (0.85 / max_peak).clamp(1.0, 5.0)
                } else {
                    1.0
                };

                for (i, &(min, max)) in self.peaks.iter().enumerate() {
                    // Center the line stroke within each bucket
                    let x = rect.left() + (i as f32 + 0.5) * bucket_width;
                    let bucket_progress = (i as f32 + 0.5) / self.peaks.len() as f32;

                    let y_top = mid_y - ((max * gain).clamp(0.0, 1.0) * half_height);
                    let y_bottom = mid_y - ((min * gain).clamp(-1.0, 0.0) * half_height);

                    // Waveform color: bright cyan for played portion vs muted slate for unplayed
                    let color = if bucket_progress <= playhead_progress {
                        Color32::from_rgb(56, 189, 248) // sky-400 (played)
                    } else {
                        Color32::from_rgb(71, 85, 105) // slate-600 (unplayed)
                    };

                    shapes.push(egui::Shape::line_segment(
                        [Pos2::new(x, y_top), Pos2::new(x, y_bottom.max(y_top + 1.5))],
                        Stroke::new(bucket_width.max(1.0), color),
                    ));
                }

                painter.extend(shapes);
            }

            // Playhead indicator: clean, thin 1.0px needle line
            let clamped_progress = playhead_progress.clamp(0.0, 1.0);
            let playhead_x = rect.left() + clamped_progress * rect.width();
            painter.line_segment(
                [Pos2::new(playhead_x, rect.top()), Pos2::new(playhead_x, rect.bottom())],
                Stroke::new(1.0_f32, Color32::from_rgb(244, 63, 94)),
            );
        }

        response
    }
}
