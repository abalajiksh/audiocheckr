//! Dithering detection module
//!
//! Detects various types of dithering including:
//! - RPDF (Rectangular Probability Density Function)
//! - TPDF (Triangular Probability Density Function)
//! - Noise Shaped dither (various curves)
//! - Truncation (lack of dither)

use serde::{Deserialize, Serialize};
use crate::core::dsp::{SpectralAnalyzer, WindowFunction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DitheringResult {
    pub is_dithered: bool,
    pub dither_type: DitherType,
    pub bit_depth: u16,
    pub noise_shaping: bool,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DitherType {
    None,
    Truncated,
    RPDF,
    TPDF,
    Gaussian,
    Shaped,
    Unknown,
}

pub struct DitheringDetector {
    // Configurable thresholds could go here
}

impl DitheringDetector {
    pub fn new() -> Self {
        Self {}
    }

    pub fn detect(&self, samples: &[f32], bit_depth: u16) -> DitheringResult {
        // We focus on the LSBs.
        // For 16-bit, we look at the 16th bit. For 24-bit, the 24th.
        
        let max_amp = match bit_depth {
            16 => 32768.0,
            24 => 8388608.0,
            _ => 1.0, // Fallback, though dithering usually implies PCM scaling
        };

        if max_amp == 1.0 {
            return DitheringResult {
                is_dithered: false,
                dither_type: DitherType::Unknown,
                bit_depth,
                noise_shaping: false,
                confidence: 0.0,
            };
        }
        
        // Let's collect LSBs
        let mut lsb_values = Vec::with_capacity(samples.len().min(65536));
        let scale = max_amp - 1.0; // 32767.0 for 16-bit
        
        for &s in samples.iter().take(65536) {
             let val = s * scale;
             // We cast to i32.
             let int_val = val.round() as i32;
             lsb_values.push(int_val);
        }

        let mut noise_shaping = false;
        let mut dither_type = DitherType::None;
        let mut confidence = 0.0;

        // Check for Noise Shaping (Rising High Frequency in LSBs)
        let lsb_spectrum_score = self.detect_noise_shaping(samples);
        if lsb_spectrum_score > 0.3 {
            noise_shaping = true;
            dither_type = DitherType::Shaped;
            confidence = lsb_spectrum_score;
        } else {
             // Check for LSB entropy
             let lsb_entropy = self.calculate_lsb_entropy(&lsb_values);
             
             if lsb_entropy > 0.95 {
                 // High entropy in LSB -> likely dithered (RPDF/TPDF)
                 if !noise_shaping {
                     dither_type = DitherType::TPDF; // Assumption
                     confidence = 0.5;
                 }
             } else if lsb_entropy < 0.5 {
                 dither_type = DitherType::Truncated;
                 confidence = 0.8;
             }
        }

        DitheringResult {
            is_dithered: dither_type != DitherType::None && dither_type != DitherType::Truncated,
            dither_type,
            bit_depth,
            noise_shaping,
            confidence,
        }
    }

    fn detect_noise_shaping(&self, samples: &[f32]) -> f64 {
        // Analyze the spectrum of the extracted LSBs (approximate noise floor)
        
        let mut diffs: Vec<f64> = Vec::with_capacity(samples.len().min(16384));
        for i in 1..samples.len().min(16384) {
            // Convert to f64 for DSP processing
            diffs.push((samples[i] - samples[i-1]) as f64);
        }
        
        // Compute FFT of the difference signal using DB method
        let mut analyzer = SpectralAnalyzer::new(4096, 1024, WindowFunction::Hann);
        let spectrum_db = analyzer.compute_power_spectrum_db(&diffs);
        
        // Check for rising slope at high freqs
        // Compare energy in 0-10kHz vs 15-22kHz
        let bin_size = 44100.0 / 4096.0; // Assuming 44.1 for bin calc, ratio is invariant
        let low_end_bin = (10000.0 / bin_size) as usize;
        let high_start_bin = (16000.0 / bin_size) as usize;
        
        if high_start_bin >= spectrum_db.len() { return 0.0; }
        
        // Convert dB back to linear for simple energy ratio comparison, 
        // or just compare average dB levels
        
        // Average dB in low band
        let low_db: f64 = spectrum_db[..low_end_bin].iter().sum::<f64>() / low_end_bin as f64;
        
        // Average dB in high band
        let high_db: f64 = spectrum_db[high_start_bin..].iter().sum::<f64>() / (spectrum_db.len() - high_start_bin) as f64;
        
        // If high frequencies are significantly louder (e.g. > 6dB difference)
        if high_db > low_db + 6.0 {
            // Significant HF rise -> Noise Shaping
            let diff = high_db - low_db;
            let prob = (diff / 20.0).min(1.0); // Map 0-20dB diff to 0-1 confidence
            return prob;
        }
        
        0.0
    }
    
    fn calculate_lsb_entropy(&self, values: &[i32]) -> f64 {
        let mut counts = std::collections::HashMap::new();
        for &v in values {
            let lsb = v & 1; // Look at bottom bit only
            *counts.entry(lsb).or_insert(0) += 1;
        }
        
        let total = values.len() as f64;
        let mut entropy = 0.0;
        for &count in counts.values() {
            let p = count as f64 / total;
            entropy -= p * p.log2();
        }
        
        // Max entropy for 1 bit is 1.0
        entropy
    }
}
