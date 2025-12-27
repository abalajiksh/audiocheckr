//! Digital Signal Processing utilities

use rustfft::{FftPlanner, num_complex::Complex};
use std::f64::consts::PI;

/// Window functions for spectral analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFunction {
    Rectangular,
    Hann,
    Hamming,
    Blackman,
    BlackmanHarris,
    Kaiser(u32), // Beta parameter * 100
}

impl Default for WindowFunction {
    fn default() -> Self {
        Self::Hann
    }
}

impl WindowFunction {
    /// Generate window coefficients
    pub fn generate(&self, size: usize) -> Vec<f64> {
        match self {
            WindowFunction::Rectangular => vec![1.0; size],
            WindowFunction::Hann => Self::hann(size),
            WindowFunction::Hamming => Self::hamming(size),
            WindowFunction::Blackman => Self::blackman(size),
            WindowFunction::BlackmanHarris => Self::blackman_harris(size),
            WindowFunction::Kaiser(beta) => Self::kaiser(size, *beta as f64 / 100.0),
        }
    }

    fn hann(size: usize) -> Vec<f64> {
        (0..size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (size - 1) as f64).cos()))
            .collect()
    }

    fn hamming(size: usize) -> Vec<f64> {
        (0..size)
            .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f64 / (size - 1) as f64).cos())
            .collect()
    }

    fn blackman(size: usize) -> Vec<f64> {
        (0..size)
            .map(|i| {
                let x = 2.0 * PI * i as f64 / (size - 1) as f64;
                0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
            })
            .collect()
    }

    fn blackman_harris(size: usize) -> Vec<f64> {
        (0..size)
            .map(|i| {
                let x = 2.0 * PI * i as f64 / (size - 1) as f64;
                0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
            })
            .collect()
    }

    fn kaiser(size: usize, beta: f64) -> Vec<f64> {
        let i0_beta = bessel_i0(beta);
        (0..size)
            .map(|i| {
                let x = 2.0 * i as f64 / (size - 1) as f64 - 1.0;
                bessel_i0(beta * (1.0 - x * x).sqrt()) / i0_beta
            })
            .collect()
    }
}

/// Modified Bessel function of the first kind, order 0
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let x_half = x / 2.0;
    
    for k in 1..50 {
        term *= (x_half / k as f64).powi(2);
        sum += term;
        if term < 1e-15 * sum {
            break;
        }
    }
    
    sum
}

/// Spectral analyzer for audio processing
pub struct SpectralAnalyzer {
    fft_size: usize,
    hop_size: usize,
    window: Vec<f64>,
    planner: FftPlanner<f64>,
}

impl SpectralAnalyzer {
    pub fn new(fft_size: usize, hop_size: usize, window_fn: WindowFunction) -> Self {
        Self {
            fft_size,
            hop_size,
            window: window_fn.generate(fft_size),
            planner: FftPlanner::new(),
        }
    }

    /// Compute magnitude spectrum for a frame
    pub fn compute_spectrum(&mut self, samples: &[f64]) -> Vec<f64> {
        let fft = self.planner.plan_fft_forward(self.fft_size);
        
        // Apply window and convert to complex
        let mut buffer: Vec<Complex<f64>> = samples
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();
        
        // Pad if necessary
        buffer.resize(self.fft_size, Complex::new(0.0, 0.0));
        
        // Perform FFT
        fft.process(&mut buffer);
        
        // Compute magnitude spectrum (only positive frequencies)
        buffer[..self.fft_size / 2 + 1]
            .iter()
            .map(|c| c.norm())
            .collect()
    }

    /// Compute power spectrum in dB
    pub fn compute_power_spectrum_db(&mut self, samples: &[f64]) -> Vec<f64> {
        let magnitude = self.compute_spectrum(samples);
        magnitude
            .iter()
            .map(|&m| {
                if m > 0.0 {
                    20.0 * (m / self.fft_size as f64).log10()
                } else {
                    -200.0
                }
            })
            .collect()
    }

    /// Compute spectrogram for entire signal
    pub fn compute_spectrogram(&mut self, samples: &[f64]) -> Vec<Vec<f64>> {
        let num_frames = (samples.len().saturating_sub(self.fft_size)) / self.hop_size + 1;
        let mut spectrogram = Vec::with_capacity(num_frames);
        
        for i in 0..num_frames {
            let start = i * self.hop_size;
            let end = (start + self.fft_size).min(samples.len());
            let frame = &samples[start..end];
            
            let spectrum = self.compute_power_spectrum_db(frame);
            spectrogram.push(spectrum);
        }
        
        spectrogram
    }

    /// Detect spectral cutoff frequency
    pub fn detect_cutoff(&mut self, samples: &[f64], sample_rate: u32, threshold_db: f64) -> Option<f64> {
        let spectrum = self.compute_power_spectrum_db(samples);
        let freq_resolution = sample_rate as f64 / self.fft_size as f64;
        
        // Find noise floor (average of bottom 10% of spectrum)
        let mut sorted_spectrum = spectrum.clone();
        sorted_spectrum.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let noise_floor = sorted_spectrum[..sorted_spectrum.len() / 10]
            .iter()
            .sum::<f64>() / (sorted_spectrum.len() / 10) as f64;
        
        // Find cutoff (where signal drops to threshold above noise floor)
        let cutoff_level = noise_floor + threshold_db;
        
        for (i, &level) in spectrum.iter().enumerate().rev() {
            if level > cutoff_level {
                return Some(i as f64 * freq_resolution);
            }
        }
        
        None
    }

    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    pub fn hop_size(&self) -> usize {
        self.hop_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_generation() {
        let hann = WindowFunction::Hann.generate(1024);
        assert_eq!(hann.len(), 1024);
        assert!((hann[0] - 0.0).abs() < 1e-10);
        assert!((hann[512] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_spectral_analyzer() {
        let mut analyzer = SpectralAnalyzer::new(1024, 512, WindowFunction::Hann);
        let samples: Vec<f64> = (0..1024).map(|i| (2.0 * PI * 440.0 * i as f64 / 44100.0).sin()).collect();
        
        let spectrum = analyzer.compute_spectrum(&samples);
        assert_eq!(spectrum.len(), 513); // fft_size/2 + 1
    }
}
