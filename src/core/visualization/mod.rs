//! Visualization utilities for spectrograms and analysis results

/// ASCII spectrogram renderer
pub struct SpectrogramRenderer {
    width: usize,
    height: usize,
    min_db: f64,
    max_db: f64,
}

impl Default for SpectrogramRenderer {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
            min_db: -96.0,
            max_db: 0.0,
        }
    }
}

impl SpectrogramRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            ..Default::default()
        }
    }

    pub fn with_db_range(mut self, min: f64, max: f64) -> Self {
        self.min_db = min;
        self.max_db = max;
        self
    }

    /// Render spectrogram as ASCII art
    pub fn render(&self, spectrogram: &[Vec<f64>]) -> String {
        if spectrogram.is_empty() {
            return String::new();
        }

        let chars = [' ', '░', '▒', '▓', '█'];
        let num_frames = spectrogram.len();
        let num_bins = spectrogram[0].len();

        // Downsample to fit dimensions
        let frame_step = num_frames.max(1) / self.width.max(1);
        let bin_step = num_bins.max(1) / self.height.max(1);

        let mut output = String::new();
        
        // Render from top (high freq) to bottom (low freq)
        for row in (0..self.height).rev() {
            let bin_start = row * bin_step;
            let bin_end = ((row + 1) * bin_step).min(num_bins);

            for col in 0..self.width {
                let frame_start = col * frame_step;
                let frame_end = ((col + 1) * frame_step).min(num_frames);

                // Average over the region
                let mut sum = 0.0;
                let mut count = 0;
                
                for frame_idx in frame_start..frame_end {
                    if frame_idx < spectrogram.len() {
                        for bin_idx in bin_start..bin_end {
                            if bin_idx < spectrogram[frame_idx].len() {
                                sum += spectrogram[frame_idx][bin_idx];
                                count += 1;
                            }
                        }
                    }
                }

                let avg_db = if count > 0 { sum / count as f64 } else { self.min_db };
                
                // Map to character
                let normalized = ((avg_db - self.min_db) / (self.max_db - self.min_db)).clamp(0.0, 1.0);
                let char_idx = (normalized * (chars.len() - 1) as f64).round() as usize;
                output.push(chars[char_idx]);
            }
            output.push('\n');
        }

        output
    }

    /// Render with frequency axis labels
    pub fn render_with_labels(&self, spectrogram: &[Vec<f64>], sample_rate: u32, fft_size: usize) -> String {
        let base_render = self.render(spectrogram);
        let nyquist = sample_rate as f64 / 2.0;
        
        let mut output = String::new();
        output.push_str(&format!("{:.0} Hz ─┐\n", nyquist));
        
        for (i, line) in base_render.lines().enumerate() {
            let freq = nyquist * (self.height - i) as f64 / self.height as f64;
            if i % 4 == 0 {
                output.push_str(&format!("{:>6.0} Hz │{}\n", freq, line));
            } else {
                output.push_str(&format!("        │{}\n", line));
            }
        }
        
        output.push_str(&format!("    0 Hz ─┘{}\n", "─".repeat(self.width)));
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_spectrogram() {
        let renderer = SpectrogramRenderer::new(40, 10);
        let result = renderer.render(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_basic_render() {
        let renderer = SpectrogramRenderer::new(20, 5);
        let spectrogram: Vec<Vec<f64>> = (0..40)
            .map(|_| (0..100).map(|i| -96.0 + i as f64 * 0.96).collect())
            .collect();
        
        let result = renderer.render(&spectrogram);
        assert!(!result.is_empty());
        assert!(result.contains('█') || result.contains('▓'));
    }
}
