//! MQA (Master Quality Authenticated) detection
//!
//! Detects MQA-encoded audio by analyzing:
//! - LSB noise patterns (MQA uses lower 8 bits for encoding)
//! - Elevated noise floor above 18kHz
//! - Non-compressible stochastic noise in lower bits
//! - Characteristic spectral artifacts

use serde::{Deserialize, Serialize};
use rustfft::num_complex::Complex;


/// MQA detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqaDetectionResult {
    pub is_mqa_encoded: bool,
    pub confidence: f32,
    pub original_sample_rate: Option<u32>,
    pub mqa_type: Option<MqaType>,
    pub evidence: Vec<String>,
    
    // Detection metrics
    pub lsb_entropy: f32,
    pub noise_floor_elevation: f32,
    pub hf_noise_level: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MqaType {
    /// Standard MQA encoding (green light)
    Standard,
    /// MQA Studio (blue light) - studio authenticated
    Studio,
    /// Unknown MQA variant
    Unknown,
}

impl Default for MqaDetectionResult {
    fn default() -> Self {
        Self {
            is_mqa_encoded: false,
            confidence: 0.0,
            original_sample_rate: None,
            mqa_type: None,
            evidence: Vec::new(),
            lsb_entropy: 0.0,
            noise_floor_elevation: 0.0,
            hf_noise_level: 0.0,
        }
    }
}

/// MQA detector with configurable thresholds
pub struct MqaDetector {
    /// Minimum LSB entropy to consider MQA (0.0-1.0)
    pub lsb_entropy_threshold: f32,
    /// Minimum noise floor elevation in dB
    pub noise_floor_threshold: f32,
    /// Frequency above which to measure elevated noise
    pub hf_analysis_freq: f32,
    /// Number of samples to analyze
    pub analysis_window: usize,
}

impl Default for MqaDetector {
    fn default() -> Self {
        Self {
            lsb_entropy_threshold: 0.85,
            noise_floor_threshold: 15.0,
            hf_analysis_freq: 18000.0,
            analysis_window: 262144, // ~5.9s at 44.1kHz
        }
    }
}

impl MqaDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn detect(&self, samples: &[f32], sample_rate: u32, bit_depth: u32) -> MqaDetectionResult {
        let mut result = MqaDetectionResult::default();
        
        // MQA only makes sense for 24-bit files at 44.1/48kHz
        if bit_depth != 24 || (sample_rate != 44100 && sample_rate != 48000) {
            result.evidence.push("Not 24-bit 44.1/48kHz format (MQA requirement)".to_string());
            return result;
        }
        
        // Limit analysis to reasonable window
        let analysis_len = samples.len().min(self.analysis_window);
        let samples = &samples[..analysis_len];
        
        // 1. Analyze LSB entropy
        result.lsb_entropy = self.analyze_lsb_entropy(samples);
        
        // 2. Measure high-frequency noise elevation
        result.hf_noise_level = self.measure_hf_noise(samples, sample_rate);
        
        // 3. Check for elevated noise floor
        result.noise_floor_elevation = self.measure_noise_floor_elevation(samples, sample_rate);
        
        // 4. Detect original sample rate from folding patterns
        result.original_sample_rate = self.detect_original_rate(samples, sample_rate);
        
        // === DECISION LOGIC ===
        let mut confidence_factors = Vec::new();
        
        // High LSB entropy indicates MQA encoding in lower bits
        if result.lsb_entropy > self.lsb_entropy_threshold {
            let factor = (result.lsb_entropy - self.lsb_entropy_threshold) / (1.0 - self.lsb_entropy_threshold);
            confidence_factors.push(factor * 0.4);
            result.evidence.push(format!(
                "High LSB entropy ({:.2}) indicates MQA encoding in lower bits",
                result.lsb_entropy
            ));
        }
        
        // Elevated noise above 18kHz
        if result.noise_floor_elevation > self.noise_floor_threshold {
            let factor = (result.noise_floor_elevation - self.noise_floor_threshold) / 30.0;
            confidence_factors.push(factor.min(1.0) * 0.35);
            result.evidence.push(format!(
                "Elevated noise floor above 18kHz (+{:.1} dB)",
                result.noise_floor_elevation
            ));
        }
        
        // High-frequency noise characteristic of MQA
        if result.hf_noise_level > -60.0 {
            let factor = (result.hf_noise_level + 90.0) / 30.0;
            confidence_factors.push(factor.min(1.0) * 0.25);
            result.evidence.push(format!(
                "Characteristic HF noise pattern ({:.1} dBFS)",
                result.hf_noise_level
            ));
        }
        
        // Calculate overall confidence
        result.confidence = if confidence_factors.is_empty() {
            0.0
        } else {
            confidence_factors.iter().sum::<f32>()
        };
        
        result.is_mqa_encoded = result.confidence > 0.5;
        
        // Determine MQA type (simplified - would need metadata for accurate detection)
        if result.is_mqa_encoded {
            result.mqa_type = Some(MqaType::Unknown);
        }
        
        result
    }
    
    /// Analyze entropy in the least significant bits
    /// MQA stores encoded data in lower bits, creating high entropy
    fn analyze_lsb_entropy(&self, samples: &[f32]) -> f32 {
        // Convert f32 samples to simulated 24-bit integers
        let mut lsb_values = Vec::with_capacity(samples.len());
        
        for &sample in samples {
            // Simulate 24-bit quantization
            let int24 = (sample * 8388607.0) as i32;
            // Extract lower 8 bits (where MQA hides data)
            let lsb = (int24 & 0xFF) as u8;
            lsb_values.push(lsb);
        }
        
        // Calculate Shannon entropy
        let mut histogram = [0u32; 256];
        for &val in &lsb_values {
            histogram[val as usize] += 1;
        }
        
        let len = lsb_values.len() as f32;
        let mut entropy = 0.0f32;
        
        for &count in &histogram {
            if count > 0 {
                let prob = count as f32 / len;
                entropy -= prob * prob.log2();
            }
        }
        
        // Normalize to 0-1 range (max entropy for 8 bits is 8.0)
        entropy / 8.0
    }
    
    /// Measure high-frequency noise level above 18kHz
    fn measure_hf_noise(&self, samples: &[f32], sample_rate: u32) -> f32 {
        use rustfft::{FftPlanner, num_complex::Complex};
        
        let n = samples.len().min(8192).next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        
        // Prepare FFT input
        let mut buffer: Vec<Complex<f32>> = samples[..n]
            .iter()
            .map(|&s| Complex::new(s, 0.0))
            .collect();
        
        fft.process(&mut buffer);
        
        // Calculate frequency bin for 18kHz
        let freq_per_bin = sample_rate as f32 / n as f32;
        let hf_start_bin = (self.hf_analysis_freq / freq_per_bin) as usize;
        let nyquist_bin = n / 2;
        
        // Measure average power above 18kHz
        let mut hf_power = 0.0f32;
        let mut count = 0;
        
        for i in hf_start_bin..nyquist_bin {
            let magnitude = buffer[i].norm();
            hf_power += magnitude * magnitude;
            count += 1;
        }
        
        if count > 0 {
            hf_power /= count as f32;
            // Convert to dBFS
            20.0 * hf_power.sqrt().log10()
        } else {
            -100.0
        }
    }
    
    /// Measure noise floor elevation compared to typical lossless
    fn measure_noise_floor_elevation(&self, samples: &[f32], sample_rate: u32) -> f32 {
        use rustfft::{FftPlanner, num_complex::Complex};
        
        let n = samples.len().min(16384).next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        
        let mut buffer: Vec<Complex<f32>> = samples[..n]
            .iter()
            .map(|&s| Complex::new(s, 0.0))
            .collect();
        
        fft.process(&mut buffer);
        
        let freq_per_bin = sample_rate as f32 / n as f32;
        
        // Measure noise in 10-16kHz range (typical music content area)
        let low_start = (10000.0 / freq_per_bin) as usize;
        let low_end = (16000.0 / freq_per_bin) as usize;
        
        // Measure noise in 18-20kHz range (MQA encoded area)
        let high_start = (18000.0 / freq_per_bin) as usize;
        let high_end = (20000.0 / freq_per_bin) as usize;
        
        let low_noise = Self::avg_magnitude(&buffer, low_start, low_end);
        let high_noise = Self::avg_magnitude(&buffer, high_start, high_end);
        
        // Return elevation in dB
        if low_noise > 1e-10 {
            20.0 * (high_noise / low_noise).log10()
        } else {
            0.0
        }
    }
    
    /// Helper to calculate average magnitude in frequency range
    fn avg_magnitude(buffer: &[Complex<f32>], start: usize, end: usize) -> f32 {
        let mut sum = 0.0f32;
        let count = (end - start) as f32;
        
        for i in start..end {
            sum += buffer[i].norm();
        }
        
        sum / count
    }
    
    /// Attempt to detect original sample rate from MQA folding pattern
    fn detect_original_rate(&self, _samples: &[f32], base_rate: u32) -> Option<u32> {
        // MQA typically encodes 88.2/96kHz into 44.1/48kHz
        // or 176.4/192kHz into 44.1/48kHz
        // This would require more sophisticated analysis of the folding markers
        
        // For now, return common MQA original rates based on base rate
        match base_rate {
            44100 => Some(88200), // or 176400
            48000 => Some(96000), // or 192000
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lsb_entropy() {
        let detector = MqaDetector::default();
        
        // Create samples with random LSB (simulating MQA)
        let samples: Vec<f32> = (0..8192)
            .map(|i| {
                let base = (i as f32 / 8192.0) * 0.1;
                let lsb_noise = (i as f32 * 0.123).sin() * 0.00001;
                base + lsb_noise
            })
            .collect();
        
        let entropy = detector.analyze_lsb_entropy(&samples);
        assert!(entropy > 0.0 && entropy <= 1.0);
    }
    
    #[test]
    fn test_non_mqa_file() {
        let detector = MqaDetector::default();
        
        // Simple sine wave at 1kHz
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44100.0).sin() * 0.5)
            .collect();
        
        let result = detector.detect(&samples, 44100, 24);
        
        // Clean sine wave should not be detected as MQA
        assert!(!result.is_mqa_encoded || result.confidence < 0.5);
    }
}
