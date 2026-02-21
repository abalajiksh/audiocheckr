//! Digital Signal Processing utilities

use rustfft::{num_complex::Complex, FftPlanner};
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

    /// Compute power spectrum in dB (median across multiple distributed frames)
    pub fn compute_power_spectrum_db(&mut self, samples: &[f64]) -> Vec<f64> {
        let num_windows = 40; // Sample 40 windows across the file

        // If the file is too short to skip around, just process what we have as a block
        if samples.len() < self.fft_size * 2 {
            let magnitude = self.compute_spectrum(samples);
            return magnitude
                .iter()
                .map(|&m| {
                    if m > 1e-10 {
                        20.0 * (m / self.fft_size as f64).log10()
                    } else {
                        -200.0
                    }
                })
                .collect();
        }

        // Calculate a safe stride to distribute our 40 windows across the entire file
        let stride = (samples.len() - self.fft_size) / num_windows;
        let mut all_spectra: Vec<Vec<f64>> = Vec::with_capacity(num_windows);

        for i in 0..num_windows {
            let start = i * stride;
            let frame = &samples[start..start + self.fft_size];
            let magnitude = self.compute_spectrum(frame);

            // Convert to power space first (not dB yet) so we can median filter real energy
            let power_spectrum: Vec<f64> = magnitude
                .iter()
                .map(|&m| (m / self.fft_size as f64).powi(2))
                .collect();

            all_spectra.push(power_spectrum);
        }

        let num_bins = all_spectra[0].len();
        let mut median_db_spectrum = Vec::with_capacity(num_bins);

        // Find the median energy per frequency bin
        for bin in 0..num_bins {
            let mut bin_energies: Vec<f64> = all_spectra.iter().map(|s| s[bin]).collect();
            // Sort to find median
            bin_energies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median_energy = bin_energies[num_windows / 2];

            // Convert the median energy back to dB
            let db = if median_energy > 1e-20 {
                10.0 * median_energy.log10()
            } else {
                -200.0
            };
            median_db_spectrum.push(db);
        }

        median_db_spectrum
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

    /// Detect spectral cutoff frequency.
    ///
    /// # P0 FIX — complete rewrite
    ///
    /// The previous implementation estimated the noise floor as the average
    /// of the bottom 10 % of spectral bins, then scanned *backwards* from
    /// Nyquist for the first bin above `noise_floor + threshold_db`.
    ///
    /// **Why it was broken:**
    /// - The bottom 10 % includes DC leakage and empty bins, giving an
    ///   extremely low noise floor (e.g. −170 dB).
    /// - Adding 10 dB to that yields −160 dB, far below actual noise.
    /// - Scanning backwards, even residual noise near Nyquist (−120 dB)
    ///   exceeded −160 dB, so the method *always* returned near-Nyquist.
    /// - Result: `None` was never returned for lossy files because the
    ///   Nyquist-proximity check then discarded it.  Actually it returned
    ///   `Some(near_nyquist)` which the caller then treated as "no cutoff".
    ///
    /// **New algorithm (matches `spectral.rs` approach):**
    /// 1. Find peak signal level in 2–8 kHz reference band.
    /// 2. Set drop threshold = peak − 25 dB.
    /// 3. Scan forward from 10 kHz for a run of ≥ 30 consecutive bins
    ///    below the threshold (sustained energy drop).
    /// 4. Return `Some(cutoff_hz)` only if the cutoff is below 95 % of
    ///    Nyquist; return `None` for genuine full-bandwidth signals.
    pub fn detect_cutoff(
        &mut self,
        samples: &[f64],
        sample_rate: u32,
        _threshold_db: f64, // kept for API compat; internally we use adaptive logic
    ) -> Option<f64> {
        let spectrum = self.compute_power_spectrum_db(samples);
        let freq_resolution = sample_rate as f64 / self.fft_size as f64;
        let nyquist = sample_rate as f64 / 2.0;

        if spectrum.len() < 100 {
            return None;
        }

        // ── 1. Reference energy in 2–8 kHz ────────────────────────
        let ref_start = (2_000.0 / freq_resolution).ceil() as usize;
        let ref_end = (8_000.0 / freq_resolution).floor() as usize;

        if ref_end <= ref_start || ref_end >= spectrum.len() {
            return None;
        }

        let ref_peak: f64 = spectrum[ref_start..ref_end]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        // If the reference band itself is very quiet, the file is near-silence
        // or extremely unusual — bail out to avoid false positives.
        if ref_peak < -80.0 {
            return None;
        }

        // ── 2. Adaptive drop threshold ─────────────────────────────
        // Lossy codecs typically exhibit a 30–60 dB cliff at their
        // cutoff frequency.  25 dB catches even gentle rolloffs while
        // staying above normal spectral tilt in music.
        let drop_threshold = ref_peak - 25.0;

        // ── 3. Forward scan from 10 kHz for sustained drop ─────────
        let search_start = (10_000.0 / freq_resolution).ceil() as usize;
        let consecutive_required: usize = 30;
        let mut consecutive_below: usize = 0;
        let mut first_drop_bin: usize = spectrum.len() - 1;

        for i in search_start..spectrum.len() {
            if spectrum[i] < drop_threshold {
                if consecutive_below == 0 {
                    first_drop_bin = i;
                }
                consecutive_below += 1;
                if consecutive_below >= consecutive_required {
                    let cutoff_hz = first_drop_bin as f64 * freq_resolution;

                    // ── 4. Only report if meaningfully below Nyquist ───
                    if cutoff_hz < nyquist * 0.95 {
                        return Some(cutoff_hz);
                    } else {
                        // Cutoff right at Nyquist → genuine full-bandwidth
                        return None;
                    }
                }
            } else {
                consecutive_below = 0;
            }
        }

        // No sustained drop found → genuine lossless
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
        let samples: Vec<f64> = (0..1024)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / 44100.0).sin())
            .collect();

        let spectrum = analyzer.compute_spectrum(&samples);
        assert_eq!(spectrum.len(), 513); // fft_size/2 + 1
    }

    #[test]
    fn test_detect_cutoff_full_bandwidth() {
        // Synthesise a signal with harmonics up to near-Nyquist → should return None
        let sr = 44100u32;
        let n = sr as usize * 2; // 2 seconds
        let samples: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / sr as f64;
                let mut s = 0.0;
                for h in 1..200 {
                    let f = 100.0 * h as f64;
                    if f < 20_000.0 {
                        s += (2.0 * PI * f * t).sin() / h as f64;
                    }
                }
                s * 0.3
            })
            .collect();

        let mut analyzer = SpectralAnalyzer::new(8192, 2048, WindowFunction::BlackmanHarris);
        let cutoff = analyzer.detect_cutoff(&samples, sr, 10.0);
        assert!(
            cutoff.is_none(),
            "Full-bandwidth signal should not trigger cutoff; got {:?}",
            cutoff
        );
    }

    #[test]
    fn test_detect_cutoff_lossy_simulation() {
        // Synthesise a signal with content only up to 16 kHz (like MP3 128k)
        let sr = 44100u32;
        let n = sr as usize * 2;
        let samples: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / sr as f64;
                let mut s = 0.0;
                for h in 1..200 {
                    let f = 100.0 * h as f64;
                    if f < 16_000.0 {
                        s += (2.0 * PI * f * t).sin() / h as f64;
                    }
                }
                s * 0.3
            })
            .collect();

        let mut analyzer = SpectralAnalyzer::new(8192, 2048, WindowFunction::BlackmanHarris);
        let cutoff = analyzer.detect_cutoff(&samples, sr, 10.0);
        assert!(
            cutoff.is_some(),
            "Signal with 16 kHz cutoff should be detected"
        );
        let hz = cutoff.unwrap();
        assert!(
            hz > 14_000.0 && hz < 18_000.0,
            "Cutoff should be near 16 kHz, got {:.0}",
            hz
        );
    }
}
