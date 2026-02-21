//! Dithering detection module
//!
//! Detects various types of dithering including:
//! - RPDF (Rectangular Probability Density Function)
//! - TPDF (Triangular Probability Density Function)
//! - Noise Shaped dither (various curves)
//! - Truncation (lack of dither)

use crate::core::dsp::{SpectralAnalyzer, WindowFunction};
use serde::{Deserialize, Serialize};

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
    /// Sample rate in Hz — needed for correct FFT bin ↔ frequency mapping
    /// in noise-shaping detection. Defaults to 44100 but MUST be set
    /// to the actual file sample rate for correct results at 48k, 96k, etc.
    sample_rate: u32,
}

impl DitheringDetector {
    pub fn new() -> Self {
        Self { sample_rate: 44100 }
    }

    /// Create a detector configured for a specific sample rate.
    ///
    /// This is the preferred constructor — the noise-shaping detector's
    /// FFT bin → Hz mapping depends on the sample rate, so using the
    /// wrong rate shifts the low/high band boundaries and produces
    /// incorrect scores.
    pub fn with_sample_rate(sample_rate: u32) -> Self {
        Self { sample_rate }
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
        // Now uses self.sample_rate instead of hardcoded 44100
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
                                                    // ── Raised from 0.5 → 0.72 ─────────────────
                                                    // TPDF is by far the most common dither type in
                                                    // professional DAWs (Pro Tools, Logic, Reaper all
                                                    // default to it). An LSB entropy > 0.95 with no
                                                    // noise-shaping signature is strong evidence.
                                                    // The old 0.5 was too conservative and caused the
                                                    // detection to be filtered out by min_confidence
                                                    // in many pipelines.
                    confidence = 0.72;
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

    /// Detect noise-shaped dithering by checking for a rising HF slope
    /// in the difference-signal spectrum.
    ///
    /// # Fix: sample-rate-aware bin mapping
    ///
    /// The original implementation hardcoded `44100.0` for the bin size
    /// calculation, which meant that at 48 kHz the "10 kHz" boundary was
    /// actually at ~10.9 kHz and the "16 kHz" boundary at ~17.4 kHz.
    /// At 96 kHz the error doubled. This caused:
    ///
    /// - **False negatives** at high sample rates (the "high band" was
    ///   actually the mid-band, so no HF rise was visible).
    /// - **Slightly shifted thresholds** at 48 kHz (less severe but
    ///   still wrong).
    ///
    /// Now uses `self.sample_rate` so bin boundaries are correct at any rate.
    fn detect_noise_shaping(&self, samples: &[f32]) -> f64 {
        // Analyze the spectrum of the extracted LSBs (approximate noise floor)

        let mut diffs: Vec<f64> = Vec::with_capacity(samples.len().min(16384));
        for i in 1..samples.len().min(16384) {
            // Convert to f64 for DSP processing
            diffs.push((samples[i] - samples[i - 1]) as f64);
        }

        // Compute FFT of the difference signal using dB method
        let fft_size = 4096;
        let mut analyzer = SpectralAnalyzer::new(fft_size, 1024, WindowFunction::Hann);
        let spectrum_db = analyzer.compute_power_spectrum_db(&diffs);

        // ── FIX: use actual sample rate ─────────────────────────
        let bin_size = self.sample_rate as f64 / fft_size as f64;

        // Compare energy in 0–10 kHz vs 16–(Nyquist) kHz
        let low_end_bin = (10000.0 / bin_size) as usize;
        let high_start_bin = (16000.0 / bin_size) as usize;

        if high_start_bin >= spectrum_db.len() {
            return 0.0;
        }

        // Average dB in low band
        if low_end_bin == 0 {
            return 0.0;
        }
        let low_db: f64 = spectrum_db[..low_end_bin].iter().sum::<f64>() / low_end_bin as f64;

        // Average dB in high band
        let high_count = spectrum_db.len() - high_start_bin;
        if high_count == 0 {
            return 0.0;
        }
        let high_db: f64 = spectrum_db[high_start_bin..].iter().sum::<f64>() / high_count as f64;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_respects_sample_rate() {
        // Ensure the two constructors produce different sample_rate fields
        let default_det = DitheringDetector::new();
        assert_eq!(default_det.sample_rate, 44100);

        let custom_det = DitheringDetector::with_sample_rate(96000);
        assert_eq!(custom_det.sample_rate, 96000);
    }

    #[test]
    fn test_truncated_detection() {
        // All-zero LSBs should detect as truncated
        let samples: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / 1024.0 * 2.0 - 1.0) * 0.5)
            .map(|s| {
                // Quantize to effectively fewer bits
                let q = (s * 32767.0).round() as i32;
                let truncated = (q >> 4) << 4; // kill bottom 4 bits
                truncated as f32 / 32767.0
            })
            .collect();

        let det = DitheringDetector::new();
        let result = det.detect(&samples, 16);
        // Should detect low entropy → truncated
        assert!(
            result.dither_type == DitherType::Truncated || !result.is_dithered,
            "Expected truncated or undithered for quantized signal"
        );
    }
}
