//! FFT processing with windowing

use rustfft::{FftPlanner, num_complex::Complex};
use super::windows::{WindowType, create_window};

/// FFT computation with windowing
pub struct FftProcessor {
    planner: FftPlanner<f32>,
    window: Vec<f32>,
    fft_size: usize,
}

impl FftProcessor {
    pub fn new(fft_size: usize, window_type: WindowType) -> Self {
        let window = create_window(fft_size, window_type);
        Self {
            planner: FftPlanner::new(),
            window,
            fft_size,
        }
    }

    /// Compute magnitude spectrum
    pub fn magnitude_spectrum(&mut self, samples: &[f32]) -> Vec<f32> {
        let fft = self.planner.plan_fft_forward(self.fft_size);
        
        let mut buffer: Vec<Complex<f32>> = samples
            .iter()
            .take(self.fft_size)
            .enumerate()
            .map(|(i, &s)| Complex::new(s * self.window[i], 0.0))
            .collect();
        
        // Zero-pad if necessary
        buffer.resize(self.fft_size, Complex::new(0.0, 0.0));
        
        fft.process(&mut buffer);
        
        buffer[..self.fft_size / 2]
            .iter()
            .map(|c| (c.re * c.re + c.im * c.im).sqrt())
            .collect()
    }

    /// Compute power spectrum in dB
    pub fn power_spectrum_db(&mut self, samples: &[f32]) -> Vec<f32> {
        let mags = self.magnitude_spectrum(samples);
        mags.iter()
            .map(|&m| {
                if m > 1e-10 {
                    20.0 * m.log10()
                } else {
                    -200.0
                }
            })
            .collect()
    }

    /// Compute complex spectrum (for phase analysis)
    pub fn complex_spectrum(&mut self, samples: &[f32]) -> Vec<Complex<f32>> {
        let fft = self.planner.plan_fft_forward(self.fft_size);
        
        let mut buffer: Vec<Complex<f32>> = samples
            .iter()
            .take(self.fft_size)
            .enumerate()
            .map(|(i, &s)| Complex::new(s * self.window[i], 0.0))
            .collect();
        
        buffer.resize(self.fft_size, Complex::new(0.0, 0.0));
        fft.process(&mut buffer);
        
        buffer[..self.fft_size / 2].to_vec()
    }

    pub fn fft_size(&self) -> usize {
        self.fft_size
    }
}
