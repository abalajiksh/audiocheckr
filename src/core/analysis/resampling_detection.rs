//! Resampling detection module
//!
//! Detects evidence of sample rate conversion (Upsampling or Downsampling)
//! by analyzing spectral cutoffs, filter rolloff characteristics, and
//! signal-to-noise ratios at expected Nyquist frequencies.

use serde::{Deserialize, Serialize};
use crate::core::dsp::{SpectralAnalyzer, WindowFunction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResamplingResult {
    pub is_resampled: bool,
    pub original_rate: Option<u32>,
    pub target_rate: u32,
    pub quality: String, // e.g. "SoXR VHQ", "Linear Interpolation"
    pub confidence: f64,
}

pub struct ResamplingDetector {
    // Configs
}

impl ResamplingDetector {
    pub fn new() -> Self {
        Self {}
    }

    pub fn detect(&self, samples: &[f32], sample_rate: u32) -> ResamplingResult {
        // Use a high-resolution FFT to find the precise cutoff frequency
        let mut analyzer = SpectralAnalyzer::new(16384, 4096, WindowFunction::BlackmanHarris);
        let samples_f64: Vec<f64> = samples.iter().map(|&s| s as f64).collect();
        let spectrum = analyzer.compute_power_spectrum_db(&samples_f64);
        
        let freq_per_bin = sample_rate as f64 / 16384.0;
        
        // Find the "knee" or cutoff point where energy drops precipitously
        // Scan from Nyquist down
        let mut cutoff_bin = 0;
        let mut found_cutoff = false;
        let _noise_floor_db = -100.0; // Assumption (unused, prefixed with _)
        let signal_threshold = -80.0;
        
        for i in (100..spectrum.len()).rev() {
            if spectrum[i] > signal_threshold {
                // Found the signal edge
                cutoff_bin = i;
                found_cutoff = true;
                break;
            }
        }
        
        if !found_cutoff {
             return ResamplingResult {
                 is_resampled: false,
                 original_rate: None,
                 target_rate: sample_rate,
                 quality: "Unknown".to_string(),
                 confidence: 0.0,
             };
        }
        
        let cutoff_freq = cutoff_bin as f64 * freq_per_bin;
        
        // Analyze cutoff frequency
        // Common 44.1k resampling filters cut off around 20k-22k.
        // Common 48k filters around 22k-24k.
        // 96k filters around 44k-48k.
        
        // Check if cutoff matches a known lower sample rate
        let common_rates = [44100, 48000, 88200, 96000];
        let mut matched_rate = None;
        let mut match_confidence = 0.0;
        
        for &rate in &common_rates {
            if rate >= sample_rate { continue; }
            let nyquist = rate as f64 / 2.0;
            
            // Filters usually cut off at 90-99% of Nyquist
            // SoXR default is ~91%. VHQ is ~95%?
            
            if cutoff_freq < nyquist && cutoff_freq > nyquist * 0.8 {
                // Matches this rate's Nyquist
                matched_rate = Some(rate);
                
                // Calculate filter steepness (rolloff)
                // Look at bins just past the cutoff
                // If it drops fast -> Digital Resampling
                // If slow -> Analog or poor resampling
                let slope = self.calculate_slope(&spectrum, cutoff_bin, 10);
                
                if slope < -5.0 { // Sharp drop (dB/bin)
                     match_confidence = 0.9;
                } else {
                     match_confidence = 0.5;
                }
                break;
            }
        }
        
        if let Some(orig) = matched_rate {
             return ResamplingResult {
                 is_resampled: true,
                 original_rate: Some(orig),
                 target_rate: sample_rate,
                 quality: "Digital Filter".to_string(),
                 confidence: match_confidence,
             };
        } 
        
        // Check for 96k file with brickwall at ~43k-47k
        let current_nyquist = sample_rate as f64 / 2.0;
        if cutoff_freq > current_nyquist * 0.9 && cutoff_freq < current_nyquist {
             let slope = self.calculate_slope(&spectrum, cutoff_bin, 5);
             if slope < -10.0 {
                 return ResamplingResult {
                     is_resampled: true,
                     original_rate: None, // Can't tell original if it was higher
                     target_rate: sample_rate,
                     quality: "Sharp Digital Filter (Possible Downsampling)".to_string(),
                     confidence: 0.7,
                 };
             }
        }
        
        ResamplingResult {
            is_resampled: false,
            original_rate: None,
            target_rate: sample_rate,
            quality: "Unknown".to_string(),
            confidence: 0.0,
        }
    }
    
    fn calculate_slope(&self, spectrum: &[f64], start_bin: usize, width: usize) -> f64 {
        if start_bin + width >= spectrum.len() { return 0.0; }
        let start_amp = spectrum[start_bin];
        let end_amp = spectrum[start_bin + width];
        (end_amp - start_amp) / width as f64
    }
}
