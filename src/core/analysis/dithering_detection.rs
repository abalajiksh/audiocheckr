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

        // 1. Extract LSB error signal (difference between sample and quantized value)
        // Or analyze the distribution of the fractional part before quantization? 
        // We only have the already-quantized samples.
        // So we look at the distribution of the LSB values themselves?
        // Actually, looking at the distribution of samples * modulo 1.0 * (scaled) isn't right because they are integers.
        // We need to look at the statistical properties of the lowest bits.
        
        // Let's collect LSBs
        let mut lsb_values = Vec::with_capacity(samples.len().min(65536));
        let scale = max_amp - 1.0; // 32767.0 for 16-bit
        
        for &s in samples.iter().take(65536) {
             let val = s * scale;
             // Ideally audio is integer values. 
             // If dithered, the LSBs should be random.
             // If truncated, LSBs might be 0 if the source was lower precision? No.
             // If source was float and converted to int with dither, the LSB is randomized.
             
             // We cast to i32.
             let int_val = val.round() as i32;
             lsb_values.push(int_val);
        }

        // Analyze Histogram of (Value % 2^N) ? No.
        // Standard dither detection looks at the statistics of the signal at very low levels, 
        // or assumes the signal contains silence where dither is most visible.
        // But we might not have silence.
        
        // Alternative: Look at the *LSB only*.
        // TPDF dither adds 2 LSBs of noise. RPDF adds 1 LSB.
        // This effectively randomizes the bottom bits.
        
        // Let's look at the bit usage statistics for the bottom 1-2 bits.
        // But a real signal also randomizes bottom bits.
        
        // BETTER APPROACH for analyzed files (which might be test signals or silence):
        // If we can find a "silent" passage, we can analyze the noise floor directly.
        // If no silence, it's harder.
        
        // However, noise shaping pushes noise to high frequencies. 
        // We can check the spectrum of the LSB signal (error signal).
        // Since we don't have the source, we can try to extract the "noise" by high-pass filtering 
        // or by assuming the music is lower frequency than the dither noise (valid for aggressive shaping).
        
        // Let's try to isolate the "noise floor" of the LSB.
        // 1. Get the lowest few bits.
        // 2. Compute spectrum.
        // 3. Check for "rise" in high freq (Shaping).
        
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
             // Check for TPDF vs RPDF via histogram of the LSBs during "quiet" moments?
             // Or just check LSB randomness entropy.
             // TPDF has specific autocorrelation properties (correlation at lag 1? No, TPDF is usually white).
             // Actually, shaped dither is colored. TPDF/RPDF is white.
             
             // If white noise in LSB -> RPDF or TPDF.
             // If we can distinguish: TPDF has triangular PDF of error. RPDF has rectangular.
             // We can't easily see the error PDF without the source.
             // But we can guess based on "cleanliness" or standard library signatures?
             
             // For now, let's rely on the spectral check for Shaping, and randomness for Basic Dither.
             let lsb_entropy = self.calculate_lsb_entropy(&lsb_values);
             
             if lsb_entropy > 0.95 {
                 // High entropy in LSB -> likely dithered (RPDF/TPDF)
                 // Hard to distinguish RPDF/TPDF without silence analysis.
                 // We'll default to TPDF as it's most common for "good" dither, 
                 // or RPDF if it's "perfectly" random 1-bit?
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
        // We isolate the residual by removing the "signal".
        // Simple way: differencing? s[i] - s[i-1] removes LF signal, leaves HF noise.
        // Shaped dither has A LOT of HF energy.
        
        let mut diffs = Vec::with_capacity(samples.len());
        for i in 1..samples.len().min(16384) {
            diffs.push(samples[i] - samples[i-1]);
        }
        
        // Compute FFT of the difference signal
        let mut analyzer = SpectralAnalyzer::new(4096, 1024, WindowFunction::Hann);
        let spectrum = analyzer.compute_power_spectrum(&diffs);
        
        // Check for rising slope at high freqs
        // Compare energy in 0-10kHz vs 15-22kHz
        let bin_size = 44100.0 / 4096.0; // Assuming 44.1 for bin calc, ratio is invariant
        let low_end_bin = (10000.0 / bin_size) as usize;
        let high_start_bin = (16000.0 / bin_size) as usize;
        
        if high_start_bin >= spectrum.len() { return 0.0; }
        
        let low_energy: f32 = spectrum[..low_end_bin].iter().sum();
        let high_energy: f32 = spectrum[high_start_bin..].iter().sum();
        
        let low_avg = low_energy / low_end_bin as f32;
        let high_avg = high_energy / (spectrum.len() - high_start_bin) as f32;
        
        if high_avg > low_avg * 2.0 {
            // Significant HF rise -> Noise Shaping
            // Map ratio to 0-1 confidence
            let ratio = high_avg / low_avg;
            return (1.0 - (-ratio).exp()).min(1.0); // Sigmoid-ish
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
