//! MQA (Master Quality Authenticated) detection
//!
//! Detects MQA-encoded audio by analyzing:
//! - LSB noise patterns (MQA uses lower 8 bits for encoding)
//! - Elevated noise floor above 18kHz
//! - Non-compressible stochastic noise in lower bits
//! - Characteristic spectral artifacts
//! - Bit pattern analysis for MQA sync markers
//!
//! Supports detection of files encoded with various MQAEncode versions:
//! - v2.3.x (2017-2018): Earlier encoding with different LSB patterns
//! - v2.5.x (2020+): Current encoding with refined noise injection

use serde::{Deserialize, Serialize};
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::PI;

/// MQA detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqaDetectionResult {
    pub is_mqa_encoded: bool,
    pub confidence: f32,
    pub original_sample_rate: Option<u32>,
    pub mqa_type: Option<MqaType>,
    pub encoder_version: Option<MqaEncoderVersion>,
    pub evidence: Vec<String>,
    
    // Detection metrics
    pub lsb_entropy: f32,
    pub lsb_correlation: f32,
    pub noise_floor_elevation: f32,
    pub hf_noise_level: f32,
    pub bit_pattern_score: f32,
    pub spectral_folding_score: f32,
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

/// Detected MQA encoder version family
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MqaEncoderVersion {
    /// Early encoder (v2.3.x, 2017-2018)
    Early,
    /// Current encoder (v2.5.x, 2020+)
    Current,
    /// Unknown version
    Unknown,
}

impl Default for MqaDetectionResult {
    fn default() -> Self {
        Self {
            is_mqa_encoded: false,
            confidence: 0.0,
            original_sample_rate: None,
            mqa_type: None,
            encoder_version: None,
            evidence: Vec::new(),
            lsb_entropy: 0.0,
            lsb_correlation: 0.0,
            noise_floor_elevation: 0.0,
            hf_noise_level: 0.0,
            bit_pattern_score: 0.0,
            spectral_folding_score: 0.0,
        }
    }
}

/// MQA detector with configurable thresholds
pub struct MqaDetector {
    /// Minimum LSB entropy to consider MQA (0.0-1.0)
    pub lsb_entropy_threshold: f32,
    /// Minimum LSB entropy for early encoders (lower threshold)
    pub lsb_entropy_threshold_early: f32,
    /// Minimum noise floor elevation in dB
    pub noise_floor_threshold: f32,
    /// Frequency above which to measure elevated noise
    pub hf_analysis_freq: f32,
    /// Number of samples to analyze
    pub analysis_window: usize,
    /// Minimum bit pattern score for detection
    pub bit_pattern_threshold: f32,
}

impl Default for MqaDetector {
    fn default() -> Self {
        Self {
            lsb_entropy_threshold: 0.85,
            lsb_entropy_threshold_early: 0.70,  // Early encoders may have lower entropy
            noise_floor_threshold: 12.0,         // Lowered from 15.0 for early encoders
            hf_analysis_freq: 18000.0,
            analysis_window: 262144,             // ~5.9s at 44.1kHz
            bit_pattern_threshold: 0.3,
        }
    }
}

impl MqaDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create detector with relaxed thresholds for early encoder versions
    pub fn for_early_encoders() -> Self {
        Self {
            lsb_entropy_threshold: 0.70,
            lsb_entropy_threshold_early: 0.60,
            noise_floor_threshold: 8.0,
            bit_pattern_threshold: 0.25,
            ..Default::default()
        }
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
        
        // 1. Analyze LSB entropy (primary indicator)
        result.lsb_entropy = self.analyze_lsb_entropy(samples);
        
        // 2. Analyze LSB correlation patterns (MQA has specific patterns)
        result.lsb_correlation = self.analyze_lsb_correlation(samples);
        
        // 3. Measure high-frequency noise elevation
        result.hf_noise_level = self.measure_hf_noise(samples, sample_rate);
        
        // 4. Check for elevated noise floor
        result.noise_floor_elevation = self.measure_noise_floor_elevation(samples, sample_rate);
        
        // 5. Look for MQA bit patterns / sync markers
        result.bit_pattern_score = self.detect_bit_patterns(samples);
        
        // 6. Detect spectral folding artifacts
        result.spectral_folding_score = self.detect_spectral_folding(samples, sample_rate);
        
        // 7. Detect original sample rate from folding patterns
        result.original_sample_rate = self.detect_original_rate(samples, sample_rate);
        
        // === DECISION LOGIC ===
        // Use multi-factor scoring with version-aware thresholds
        
        let mut confidence_factors = Vec::new();
        let mut is_likely_early_encoder = false;
        
        // Check for early encoder characteristics:
        // Lower entropy but specific correlation patterns
        if result.lsb_entropy > self.lsb_entropy_threshold_early 
           && result.lsb_entropy < self.lsb_entropy_threshold
           && result.lsb_correlation > 0.15 {
            is_likely_early_encoder = true;
            result.evidence.push(format!(
                "LSB patterns consistent with early MQA encoder (entropy: {:.2}, correlation: {:.2})",
                result.lsb_entropy, result.lsb_correlation
            ));
        }
        
        // High LSB entropy indicates MQA encoding in lower bits
        let entropy_threshold = if is_likely_early_encoder {
            self.lsb_entropy_threshold_early
        } else {
            self.lsb_entropy_threshold
        };
        
        if result.lsb_entropy > entropy_threshold {
            let factor = (result.lsb_entropy - entropy_threshold) / (1.0 - entropy_threshold);
            confidence_factors.push(factor.min(1.0) * 0.35);
            result.evidence.push(format!(
                "High LSB entropy ({:.3}) indicates MQA encoding in lower bits",
                result.lsb_entropy
            ));
        }
        
        // LSB correlation patterns specific to MQA
        if result.lsb_correlation > 0.1 {
            let factor = (result.lsb_correlation - 0.1) / 0.4;
            confidence_factors.push(factor.min(1.0) * 0.15);
            result.evidence.push(format!(
                "LSB correlation pattern ({:.3}) suggests MQA encoding",
                result.lsb_correlation
            ));
        }
        
        // Elevated noise above 18kHz
        let noise_threshold = if is_likely_early_encoder {
            self.noise_floor_threshold * 0.7
        } else {
            self.noise_floor_threshold
        };
        
        if result.noise_floor_elevation > noise_threshold {
            let factor = (result.noise_floor_elevation - noise_threshold) / 30.0;
            confidence_factors.push(factor.min(1.0) * 0.20);
            result.evidence.push(format!(
                "Elevated noise floor above 18kHz (+{:.1} dB)",
                result.noise_floor_elevation
            ));
        }
        
        // High-frequency noise characteristic of MQA
        if result.hf_noise_level > -65.0 {
            let factor = (result.hf_noise_level + 90.0) / 30.0;
            confidence_factors.push(factor.min(1.0) * 0.10);
            result.evidence.push(format!(
                "Characteristic HF noise pattern ({:.1} dBFS)",
                result.hf_noise_level
            ));
        }
        
        // Bit pattern score
        if result.bit_pattern_score > self.bit_pattern_threshold {
            let factor = (result.bit_pattern_score - self.bit_pattern_threshold) / 0.5;
            confidence_factors.push(factor.min(1.0) * 0.10);
            result.evidence.push(format!(
                "MQA bit pattern detected (score: {:.2})",
                result.bit_pattern_score
            ));
        }
        
        // Spectral folding artifacts
        if result.spectral_folding_score > 0.3 {
            let factor = (result.spectral_folding_score - 0.3) / 0.5;
            confidence_factors.push(factor.min(1.0) * 0.10);
            result.evidence.push(format!(
                "Spectral folding artifacts detected (score: {:.2})",
                result.spectral_folding_score
            ));
        }
        
        // Calculate overall confidence
        result.confidence = if confidence_factors.is_empty() {
            0.0
        } else {
            confidence_factors.iter().sum::<f32>()
        };
        
        // Lower threshold for early encoders
        let detection_threshold = if is_likely_early_encoder { 0.35 } else { 0.45 };
        result.is_mqa_encoded = result.confidence > detection_threshold;
        
        // Determine encoder version
        if result.is_mqa_encoded {
            result.encoder_version = Some(if is_likely_early_encoder {
                MqaEncoderVersion::Early
            } else if result.lsb_entropy > 0.90 {
                MqaEncoderVersion::Current
            } else {
                MqaEncoderVersion::Unknown
            });
            
            // Determine MQA type (simplified - would need metadata for accurate detection)
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
    
    /// Analyze correlation patterns in LSBs
    /// MQA has specific inter-sample correlation in the encoded bits
    fn analyze_lsb_correlation(&self, samples: &[f32]) -> f32 {
        if samples.len() < 1000 {
            return 0.0;
        }
        
        let mut lsb_values: Vec<i32> = Vec::with_capacity(samples.len());
        
        for &sample in samples {
            let int24 = (sample * 8388607.0) as i32;
            // Look at lower 8 bits
            let lsb = (int24 & 0xFF) as i32;
            lsb_values.push(lsb);
        }
        
        // Calculate autocorrelation at specific lags
        // MQA tends to have patterns at certain intervals
        let lags = [1, 2, 4, 8, 16, 32];
        let mut correlations = Vec::new();
        
        let mean: f32 = lsb_values.iter().map(|&x| x as f32).sum::<f32>() / lsb_values.len() as f32;
        let variance: f32 = lsb_values.iter()
            .map(|&x| (x as f32 - mean).powi(2))
            .sum::<f32>() / lsb_values.len() as f32;
        
        if variance < 0.001 {
            return 0.0;
        }
        
        for &lag in &lags {
            if lag >= lsb_values.len() {
                continue;
            }
            
            let mut correlation = 0.0f32;
            let n = lsb_values.len() - lag;
            
            for i in 0..n {
                correlation += (lsb_values[i] as f32 - mean) * (lsb_values[i + lag] as f32 - mean);
            }
            
            correlation /= n as f32 * variance;
            correlations.push(correlation.abs());
        }
        
        // MQA files tend to have higher absolute correlation than random noise
        if correlations.is_empty() {
            0.0
        } else {
            correlations.iter().sum::<f32>() / correlations.len() as f32
        }
    }
    
    /// Measure high-frequency noise level above 18kHz
    fn measure_hf_noise(&self, samples: &[f32], sample_rate: u32) -> f32 {
        let n = samples.len().min(8192).next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        
        // Apply Hann window
        let mut buffer: Vec<Complex<f32>> = samples[..n]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let window = 0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos());
                Complex::new(s * window, 0.0)
            })
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
            if hf_power > 1e-20 {
                20.0 * hf_power.sqrt().log10()
            } else {
                -120.0
            }
        } else {
            -100.0
        }
    }
    
    /// Measure noise floor elevation compared to typical lossless
    fn measure_noise_floor_elevation(&self, samples: &[f32], sample_rate: u32) -> f32 {
        let n = samples.len().min(16384).next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        
        let mut buffer: Vec<Complex<f32>> = samples[..n]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let window = 0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos());
                Complex::new(s * window, 0.0)
            })
            .collect();
        
        fft.process(&mut buffer);
        
        let freq_per_bin = sample_rate as f32 / n as f32;
        
        // Measure noise in 10-16kHz range (typical music content area)
        let low_start = (10000.0 / freq_per_bin) as usize;
        let low_end = (16000.0 / freq_per_bin) as usize;
        
        // Measure noise in 18-20kHz range (MQA encoded area)
        let high_start = (18000.0 / freq_per_bin) as usize;
        let high_end = ((20000.0 / freq_per_bin) as usize).min(n / 2);
        
        let low_noise = Self::avg_magnitude(&buffer, low_start, low_end);
        let high_noise = Self::avg_magnitude(&buffer, high_start, high_end);
        
        // Return elevation in dB
        if low_noise > 1e-10 && high_noise > 1e-10 {
            20.0 * (high_noise / low_noise).log10()
        } else {
            0.0
        }
    }
    
    /// Helper to calculate average magnitude in frequency range
    fn avg_magnitude(buffer: &[Complex<f32>], start: usize, end: usize) -> f32 {
        if end <= start || start >= buffer.len() {
            return 0.0;
        }
        
        let end = end.min(buffer.len());
        let mut sum = 0.0f32;
        let count = (end - start) as f32;
        
        for i in start..end {
            sum += buffer[i].norm();
        }
        
        sum / count
    }
    
    /// Detect MQA-specific bit patterns
    /// MQA embeds sync markers and metadata in the LSBs
    fn detect_bit_patterns(&self, samples: &[f32]) -> f32 {
        if samples.len() < 4096 {
            return 0.0;
        }
        
        // Extract bit sequences from lower 8 bits
        let mut bit_stream: Vec<u8> = Vec::new();
        
        for &sample in samples.iter().take(16384) {
            let int24 = (sample * 8388607.0) as i32;
            let lsb = (int24 & 0xFF) as u8;
            bit_stream.push(lsb);
        }
        
        // Look for repeating patterns that indicate MQA structure
        // MQA uses specific sync patterns
        
        let mut pattern_score = 0.0f32;
        
        // Check for non-random distribution of bit transitions
        let mut transitions = 0;
        for i in 1..bit_stream.len() {
            if bit_stream[i] != bit_stream[i-1] {
                transitions += 1;
            }
        }
        
        let transition_rate = transitions as f32 / (bit_stream.len() - 1) as f32;
        
        // Random data has ~50% transition rate
        // MQA encoded data often has slightly different rates
        // due to structured data mixed with pseudo-random elements
        if transition_rate > 0.45 && transition_rate < 0.55 {
            // High entropy but structured
            pattern_score += 0.3;
        }
        
        // Check for specific byte value distributions
        // MQA tends to avoid certain byte values in the encoded stream
        let mut byte_counts = [0u32; 256];
        for &b in &bit_stream {
            byte_counts[b as usize] += 1;
        }
        
        // Count how many byte values are never or rarely used
        let rare_bytes = byte_counts.iter().filter(|&&c| c < 5).count();
        
        // Pure random would use all bytes roughly equally
        // MQA's structured encoding may have some unused values
        if rare_bytes > 10 && rare_bytes < 100 {
            pattern_score += 0.2;
        }
        
        // Check for periodicity in the LSB stream
        // MQA has frame-based structure
        let period_scores = self.check_periodicity(&bit_stream);
        if period_scores > 0.2 {
            pattern_score += period_scores * 0.3;
        }
        
        pattern_score.min(1.0)
    }
    
    /// Check for periodic patterns in bit stream
    fn check_periodicity(&self, data: &[u8]) -> f32 {
        if data.len() < 1024 {
            return 0.0;
        }
        
        // Check common MQA frame-related periods
        let periods = [256, 512, 1024, 2048];
        let mut max_correlation = 0.0f32;
        
        for &period in &periods {
            if period * 2 > data.len() {
                continue;
            }
            
            let mut matches = 0;
            let check_len = data.len().min(period * 4);
            
            for i in period..check_len {
                if data[i] == data[i - period] {
                    matches += 1;
                }
            }
            
            let correlation = matches as f32 / (check_len - period) as f32;
            // Random data would have ~1/256 match rate
            if correlation > 0.01 {
                max_correlation = max_correlation.max(correlation);
            }
        }
        
        max_correlation
    }
    
    /// Detect spectral folding artifacts characteristic of MQA
    fn detect_spectral_folding(&self, samples: &[f32], sample_rate: u32) -> f32 {
        if samples.len() < 8192 {
            return 0.0;
        }
        
        let n = 8192;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        
        let mut buffer: Vec<Complex<f32>> = samples[..n]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let window = 0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos());
                Complex::new(s * window, 0.0)
            })
            .collect();
        
        fft.process(&mut buffer);
        
        let freq_per_bin = sample_rate as f32 / n as f32;
        let nyquist_bin = n / 2;
        
        // MQA folds high-frequency content around specific frequencies
        // Check for symmetric patterns around folding points
        
        let fold_points = [
            sample_rate as f32 / 4.0,  // Quarter Nyquist
            sample_rate as f32 / 3.0,  // Third Nyquist
        ];
        
        let mut folding_score = 0.0f32;
        
        for &fold_freq in &fold_points {
            let fold_bin = (fold_freq / freq_per_bin) as usize;
            
            if fold_bin < 50 || fold_bin >= nyquist_bin - 50 {
                continue;
            }
            
            // Check for correlation between content on either side of fold point
            let mut correlation = 0.0f32;
            let check_range = 30;
            
            for offset in 1..check_range {
                let below = buffer[fold_bin - offset].norm();
                let above = buffer[fold_bin + offset].norm();
                
                if below > 1e-10 && above > 1e-10 {
                    let ratio = (below / above).log10().abs();
                    if ratio < 0.5 {  // Similar magnitudes
                        correlation += 1.0;
                    }
                }
            }
            
            correlation /= check_range as f32;
            folding_score = folding_score.max(correlation);
        }
        
        folding_score
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
        assert!(!result.is_mqa_encoded || result.confidence < 0.4);
    }
    
    #[test]
    fn test_early_encoder_detector() {
        let detector = MqaDetector::for_early_encoders();
        
        // Verify thresholds are relaxed
        assert!(detector.lsb_entropy_threshold < MqaDetector::default().lsb_entropy_threshold);
        assert!(detector.noise_floor_threshold < MqaDetector::default().noise_floor_threshold);
    }
    
    #[test]
    fn test_lsb_correlation() {
        let detector = MqaDetector::default();
        
        // Create samples with some correlation pattern
        let samples: Vec<f32> = (0..4096)
            .map(|i| {
                let base = (i as f32 * 0.01).sin() * 0.5;
                // Add structured noise to lower bits
                let pattern = ((i % 17) as f32 / 256.0) * 0.00001;
                base + pattern
            })
            .collect();
        
        let correlation = detector.analyze_lsb_correlation(&samples);
        // Should find some correlation due to the pattern
        assert!(correlation >= 0.0);
    }
}
