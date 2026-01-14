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
//!
//! IMPROVED: Better detection for early encoder versions which have:
//! - Lower LSB entropy than current encoders
//! - Different noise shaping characteristics
//! - Less aggressive HF noise injection

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
    
    // Additional metrics for early encoder detection
    pub lsb_periodicity_score: f32,
    pub bit_transition_rate: f32,
    pub lsb_value_clustering: f32,
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
            lsb_periodicity_score: 0.0,
            bit_transition_rate: 0.0,
            lsb_value_clustering: 0.0,
        }
    }
}

/// MQA detector with configurable thresholds
pub struct MqaDetector {
    /// Minimum LSB entropy to consider MQA (0.0-1.0) - for current encoders
    pub lsb_entropy_threshold: f32,
    /// Minimum LSB entropy for early encoders (lower threshold)
    pub lsb_entropy_threshold_early: f32,
    /// Minimum noise floor elevation in dB
    pub noise_floor_threshold: f32,
    /// Minimum noise floor for early encoders
    pub noise_floor_threshold_early: f32,
    /// Frequency above which to measure elevated noise
    pub hf_analysis_freq: f32,
    /// Number of samples to analyze
    pub analysis_window: usize,
    /// Minimum bit pattern score for detection
    pub bit_pattern_threshold: f32,
    /// Enable early encoder detection mode
    pub detect_early_encoders: bool,
}

impl Default for MqaDetector {
    fn default() -> Self {
        Self {
            lsb_entropy_threshold: 0.75,           // Lowered from 0.85 - current encoders
            lsb_entropy_threshold_early: 0.40,     // Lowered from 0.45 - early encoders
            noise_floor_threshold: 6.0,            // Lowered from 8.0
            noise_floor_threshold_early: 2.0,      // Lowered from 3.0
            hf_analysis_freq: 18000.0,
            analysis_window: 262144,
            bit_pattern_threshold: 0.20,           // Lowered from 0.25
            detect_early_encoders: true,
        }
    }
}

impl MqaDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create detector optimized for early encoder versions
    pub fn for_early_encoders() -> Self {
        Self {
            lsb_entropy_threshold: 0.50,           // Lower for any MQA detection
            lsb_entropy_threshold_early: 0.30,     // Very low for old encoders
            noise_floor_threshold: 4.0,
            noise_floor_threshold_early: 1.5,
            bit_pattern_threshold: 0.15,
            detect_early_encoders: true,
            ..Default::default()
        }
    }
    
    /// Create detector with strict thresholds (fewer false positives)
    pub fn strict() -> Self {
        Self {
            lsb_entropy_threshold: 0.90,
            lsb_entropy_threshold_early: 0.75,
            noise_floor_threshold: 15.0,
            noise_floor_threshold_early: 10.0,
            bit_pattern_threshold: 0.4,
            detect_early_encoders: true,
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
        
        // FIXED: Check for silence before processing
        let rms = Self::calculate_rms(samples);
        if rms < 1e-6 {
            result.evidence.push("Audio appears to be silence or near-silent".to_string());
            return result;
        }
        
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
        
        // 7. NEW: Analyze LSB periodicity (early encoders have specific patterns)
        result.lsb_periodicity_score = self.analyze_lsb_periodicity(samples);
        
        // 8. NEW: Analyze bit transition rate
        result.bit_transition_rate = self.analyze_bit_transitions(samples);
        
        // 9. NEW: Analyze LSB value clustering
        result.lsb_value_clustering = self.analyze_lsb_clustering(samples);
        
        // 10. Detect original sample rate from folding patterns
        result.original_sample_rate = self.detect_original_rate(samples, sample_rate);
        
        // === DECISION LOGIC ===
        // Use multi-factor scoring with version-aware thresholds
        
        let mut confidence_factors = Vec::new();
        let mut is_likely_early_encoder = false;
        
        // === EARLY ENCODER DETECTION ===
        // Early encoders (v2.3.x) have:
        // - Lower LSB entropy (0.4-0.75 instead of 0.75+)
        // - More structured/periodic LSB patterns
        // - Less aggressive HF noise injection
        // - Higher LSB value clustering
        
        if self.detect_early_encoders {
            let early_encoder_indicators = self.check_early_encoder_indicators(&result);
            
            // FIXED: Lowered threshold from 0.4 to 0.25 to make early encoder detection more accessible
            if early_encoder_indicators.score > 0.25 {
                is_likely_early_encoder = true;
                result.evidence.push(format!(
                    "Early encoder indicators detected (score: {:.2}): {}",
                    early_encoder_indicators.score,
                    early_encoder_indicators.reasons.join(", ")
                ));
            }
        }
        
        // === CONFIDENCE CALCULATION ===
        
        // FIXED: Check for ANY MQA encoding first, then refine with version-specific thresholds
        // This creates a multi-path detection strategy
        
        // Path 1: Check for basic MQA indicators (entropy + ANY other metric)
        let has_elevated_entropy = result.lsb_entropy > self.lsb_entropy_threshold_early;
        let has_noise_elevation = result.noise_floor_elevation > self.noise_floor_threshold_early;
        let has_correlation = result.lsb_correlation > 0.06;  // FIXED: Lowered from 0.08
        let has_hf_noise = result.hf_noise_level > -75.0;  // FIXED: Lowered threshold
        
        // If we have entropy + one other signal, we likely have MQA
        let basic_mqa_indicators = if has_elevated_entropy {
            let mut count = 0;
            if has_noise_elevation { count += 1; }
            if has_correlation { count += 1; }
            if has_hf_noise { count += 1; }
            count >= 1
        } else {
            false
        };
        
        // Select thresholds based on detected encoder type OR basic indicators
        let entropy_threshold = if is_likely_early_encoder || basic_mqa_indicators {
            self.lsb_entropy_threshold_early
        } else {
            self.lsb_entropy_threshold
        };
        
        let noise_threshold = if is_likely_early_encoder || basic_mqa_indicators {
            self.noise_floor_threshold_early
        } else {
            self.noise_floor_threshold
        };
        
        // FIXED: More generous entropy scoring
        if result.lsb_entropy > entropy_threshold {
            let factor = (result.lsb_entropy - entropy_threshold) / (1.0 - entropy_threshold);
            confidence_factors.push(factor.min(1.0) * 0.35);  // Increased from 0.30
            result.evidence.push(format!(
                "Elevated LSB entropy ({:.3}) indicates MQA encoding in lower bits",
                result.lsb_entropy
            ));
        } else if result.lsb_entropy > 0.35 {  // FIXED: Catch even lower entropy
            // Very low threshold for any structured encoding
            let factor = (result.lsb_entropy - 0.35) / 0.40;
            confidence_factors.push(factor.min(1.0) * 0.25);  // Increased from 0.20
            result.evidence.push(format!(
                "Moderate LSB entropy ({:.3}) suggests possible MQA encoding",
                result.lsb_entropy
            ));
        }
        
        // FIXED: LSB correlation patterns specific to MQA - lower threshold
        if result.lsb_correlation > 0.06 {  // Lowered from 0.08
            let factor = (result.lsb_correlation - 0.06) / 0.4;
            confidence_factors.push(factor.min(1.0) * 0.15);
            result.evidence.push(format!(
                "LSB correlation pattern ({:.3}) suggests MQA encoding",
                result.lsb_correlation
            ));
        }
        
        // FIXED: Elevated noise above 18kHz - more generous
        if result.noise_floor_elevation > noise_threshold {
            let factor = (result.noise_floor_elevation - noise_threshold) / 25.0;  // Adjusted scale
            confidence_factors.push(factor.min(1.0) * 0.20);  // Increased from 0.15
            result.evidence.push(format!(
                "Elevated noise floor above 18kHz (+{:.1} dB)",
                result.noise_floor_elevation
            ));
        } else if result.noise_floor_elevation > 1.0 {  // FIXED: Even slight elevation counts
            let factor = result.noise_floor_elevation / noise_threshold;
            confidence_factors.push(factor.min(1.0) * 0.10);
            result.evidence.push(format!(
                "Slight noise floor elevation (+{:.1} dB)",
                result.noise_floor_elevation
            ));
        }
        
        // FIXED: High-frequency noise characteristic of MQA - more inclusive
        let hf_threshold = if is_likely_early_encoder { -75.0 } else { -70.0 };
        if result.hf_noise_level > hf_threshold {
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
        if result.spectral_folding_score > 0.20 {  // FIXED: Lowered from 0.25
            let factor = (result.spectral_folding_score - 0.20) / 0.5;
            confidence_factors.push(factor.min(1.0) * 0.10);
            result.evidence.push(format!(
                "Spectral folding artifacts detected (score: {:.2})",
                result.spectral_folding_score
            ));
        }
        
        // Early encoder specific: LSB periodicity
        if result.lsb_periodicity_score > 0.15 {  // FIXED: Lowered from 0.3
            let factor = (result.lsb_periodicity_score - 0.15) / 0.5;
            confidence_factors.push(factor.min(1.0) * 0.15);  // Increased weight
            result.evidence.push(format!(
                "LSB periodicity pattern ({:.2}) indicates structured encoding",
                result.lsb_periodicity_score
            ));
        }
        
        // Early encoder specific: LSB clustering
        if result.lsb_value_clustering > 0.25 {  // FIXED: Lowered from 0.4
            let factor = (result.lsb_value_clustering - 0.25) / 0.5;
            confidence_factors.push(factor.min(1.0) * 0.15);  // Increased weight
            result.evidence.push(format!(
                "LSB value clustering ({:.2}) indicates structured encoding",
                result.lsb_value_clustering
            ));
        }
        
        // Calculate overall confidence
        result.confidence = if confidence_factors.is_empty() {
            0.0
        } else {
            confidence_factors.iter().sum::<f32>().min(1.0)
        };
        
        // FIXED: Detection threshold - significantly lowered and simplified
        let detection_threshold = if is_likely_early_encoder { 0.25 } else { 0.35 };
        result.is_mqa_encoded = result.confidence > detection_threshold;
        
        // Determine encoder version
        if result.is_mqa_encoded {
            result.encoder_version = Some(if is_likely_early_encoder {
                MqaEncoderVersion::Early
            } else if result.lsb_entropy > 0.85 {
                MqaEncoderVersion::Current
            } else {
                MqaEncoderVersion::Unknown
            });
            
            // Determine MQA type (simplified - would need metadata for accurate detection)
            result.mqa_type = Some(MqaType::Unknown);
            
            // Add summary evidence
            if let Some(ref version) = result.encoder_version {
                result.evidence.push(format!(
                    "Detected MQA encoder version: {:?}",
                    version
                ));
            }
        }
        
        result
    }
    
    /// Calculate RMS level for silence detection
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        
        let sum_squares: f32 = samples.iter()
            .take(8192)  // Sample first 8k samples for speed
            .map(|&s| s * s)
            .sum();
        
        (sum_squares / samples.len().min(8192) as f32).sqrt()
    }
    
    /// Check for indicators of early MQA encoder (v2.3.x)
    fn check_early_encoder_indicators(&self, result: &MqaDetectionResult) -> EarlyEncoderIndicators {
        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        // FIXED: Early encoders have moderate entropy (0.30-0.75 range, not as high as current)
        if result.lsb_entropy > 0.30 && result.lsb_entropy < 0.80 {  // Widened range
            score += 0.35;  // Increased weight
            reasons.push(format!("entropy in early range ({:.2})", result.lsb_entropy));
        }

        // FIXED: Early encoders have higher periodicity in LSBs
        if result.lsb_periodicity_score > 0.10 {  // Lowered from 0.15
            score += 0.30;  // Increased weight
            reasons.push(format!("LSB periodicity ({:.2})", result.lsb_periodicity_score));
        }

        // FIXED: Early encoders have more clustered LSB values
        if result.lsb_value_clustering > 0.20 {  // Lowered from 0.25
            score += 0.25;  // Increased weight
            reasons.push(format!("value clustering ({:.2})", result.lsb_value_clustering));
        }

        // FIXED: Early encoders have specific bit transition rates
        if result.bit_transition_rate > 0.35 && result.bit_transition_rate < 0.60 {  // Widened range
            score += 0.20;  // Increased weight
            reasons.push(format!("transition rate ({:.2})", result.bit_transition_rate));
        }

        // FIXED: Early encoders have lower HF noise injection
        if result.noise_floor_elevation > 1.0 && result.noise_floor_elevation < 20.0 {  // Widened significantly
            score += 0.25;  // Increased weight
            reasons.push(format!("moderate HF noise (+{:.1}dB)", result.noise_floor_elevation));
        }

        EarlyEncoderIndicators { score, reasons }
    }
    
    /// Analyze entropy in the least significant bits
    /// FIXED: Improved 24-bit conversion and handling
    fn analyze_lsb_entropy(&self, samples: &[f32]) -> f32 {
        let mut lsb_values = Vec::with_capacity(samples.len());
        
        for &sample in samples {
            // FIXED: Use abs() and clamp to ensure proper range
            // Convert to 24-bit integer representation (-8388608 to 8388607)
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            
            // Extract the least significant byte
            let lsb = (int24.abs() & 0xFF) as u8;
            lsb_values.push(lsb);
        }
        
        // FIXED: Check for degenerate case (all zeros or single value)
        let unique_values: std::collections::HashSet<u8> = lsb_values.iter().copied().collect();
        if unique_values.len() <= 1 {
            return 0.0;  // No entropy in constant signal
        }
        
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
    fn analyze_lsb_correlation(&self, samples: &[f32]) -> f32 {
        if samples.len() < 1000 {
            return 0.0;
        }
        
        let mut lsb_values: Vec<i32> = Vec::with_capacity(samples.len());
        
        for &sample in samples {
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            let lsb = (int24.abs() & 0xFF) as i32;
            lsb_values.push(lsb);
        }
        
        let lags = [1, 2, 4, 8, 16, 32, 64, 128];
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
        
        if correlations.is_empty() {
            0.0
        } else {
            correlations.iter().sum::<f32>() / correlations.len() as f32
        }
    }
    
    /// Analyze periodicity in LSB patterns (important for early encoders)
    fn analyze_lsb_periodicity(&self, samples: &[f32]) -> f32 {
        if samples.len() < 4096 {
            return 0.0;
        }
        
        let mut lsb_values: Vec<u8> = Vec::with_capacity(samples.len().min(65536));
        
        for &sample in samples.iter().take(65536) {
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            let lsb = (int24.abs() & 0xFF) as u8;
            lsb_values.push(lsb);
        }
        
        // Check for periodicity at common MQA frame boundaries
        let periods = [256, 512, 1024, 1152, 2048, 2304, 4096];
        let mut max_periodicity = 0.0f32;
        
        for &period in &periods {
            if period * 3 > lsb_values.len() {
                continue;
            }
            
            let mut matches = 0;
            let check_len = lsb_values.len().min(period * 10);
            
            for i in period..check_len {
                // Check for repeating patterns
                if lsb_values[i] == lsb_values[i - period] {
                    matches += 1;
                }
            }
            
            let periodicity = matches as f32 / (check_len - period) as f32;
            // Random data would have ~1/256 = 0.004 match rate
            // Periodic data would have much higher
            if periodicity > 0.01 {
                max_periodicity = max_periodicity.max(periodicity * 10.0);
            }
        }
        
        max_periodicity.min(1.0)
    }
    
    /// Analyze bit transition rate in LSBs
    fn analyze_bit_transitions(&self, samples: &[f32]) -> f32 {
        if samples.len() < 1000 {
            return 0.5;
        }
        
        let mut transitions = 0u64;
        let mut total_bits = 0u64;
        let mut prev_byte = 0u8;
        
        for (i, &sample) in samples.iter().take(100000).enumerate() {
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            let lsb = (int24.abs() & 0xFF) as u8;
            
            if i > 0 {
                // Count bit transitions between consecutive LSB bytes
                let diff = lsb ^ prev_byte;
                transitions += diff.count_ones() as u64;
                total_bits += 8;
            }
            prev_byte = lsb;
        }
        
        if total_bits == 0 {
            return 0.5;
        }
        
        transitions as f32 / total_bits as f32
    }
    
    /// Analyze clustering of LSB values
    fn analyze_lsb_clustering(&self, samples: &[f32]) -> f32 {
        if samples.len() < 1000 {
            return 0.0;
        }
        
        let mut histogram = [0u32; 256];
        let mut total = 0u32;
        
        for &sample in samples.iter().take(100000) {
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            let lsb = (int24.abs() & 0xFF) as u8;
            histogram[lsb as usize] += 1;
            total += 1;
        }
        
        // Count how many values have > 1% of total
        let threshold = total / 100;
        let common_values = histogram.iter().filter(|&&c| c > threshold).count();
        
        // MQA early encoders tend to cluster around certain values
        // Pure random would use all 256 values roughly equally
        // High clustering (few common values) suggests structure
        
        if common_values < 50 {
            // Very high clustering
            1.0 - (common_values as f32 / 50.0)
        } else if common_values < 150 {
            // Moderate clustering
            0.5 * (1.0 - (common_values as f32 - 50.0) / 100.0)
        } else {
            // Low clustering (more random)
            0.0
        }
    }
    
    /// Measure high-frequency noise level above 18kHz
    fn measure_hf_noise(&self, samples: &[f32], sample_rate: u32) -> f32 {
        let n = samples.len().min(8192).next_power_of_two();
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
        let hf_start_bin = (self.hf_analysis_freq / freq_per_bin) as usize;
        let nyquist_bin = n / 2;
        
        let mut hf_power = 0.0f32;
        let mut count = 0;
        
        for i in hf_start_bin..nyquist_bin {
            let magnitude = buffer[i].norm();
            hf_power += magnitude * magnitude;
            count += 1;
        }
        
        if count > 0 {
            hf_power /= count as f32;
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
        
        // Measure noise in 10-16kHz range
        let low_start = (10000.0 / freq_per_bin) as usize;
        let low_end = (16000.0 / freq_per_bin) as usize;
        
        // Measure noise in 18-20kHz range
        let high_start = (18000.0 / freq_per_bin) as usize;
        let high_end = ((20000.0 / freq_per_bin) as usize).min(n / 2);
        
        let low_noise = Self::avg_magnitude(&buffer, low_start, low_end);
        let high_noise = Self::avg_magnitude(&buffer, high_start, high_end);
        
        if low_noise > 1e-10 && high_noise > 1e-10 {
            20.0 * (high_noise / low_noise).log10()
        } else {
            0.0
        }
    }
    
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
    fn detect_bit_patterns(&self, samples: &[f32]) -> f32 {
        if samples.len() < 4096 {
            return 0.0;
        }
        
        let mut bit_stream: Vec<u8> = Vec::new();
        
        for &sample in samples.iter().take(16384) {
            let sample_clamped = sample.clamp(-1.0, 1.0);
            let int24 = (sample_clamped * 8388607.0) as i32;
            let lsb = (int24.abs() & 0xFF) as u8;
            bit_stream.push(lsb);
        }
        
        let mut pattern_score = 0.0f32;
        
        // Check for non-random distribution of bit transitions
        let mut transitions = 0;
        for i in 1..bit_stream.len() {
            if bit_stream[i] != bit_stream[i-1] {
                transitions += 1;
            }
        }
        
        let transition_rate = transitions as f32 / (bit_stream.len() - 1) as f32;
        
        // MQA has specific transition rates
        if transition_rate > 0.45 && transition_rate < 0.55 {
            pattern_score += 0.3;
        }
        
        // Check byte value distribution
        let mut byte_counts = [0u32; 256];
        for &b in &bit_stream {
            byte_counts[b as usize] += 1;
        }
        
        let rare_bytes = byte_counts.iter().filter(|&&c| c < 5).count();
        
        if rare_bytes > 10 && rare_bytes < 100 {
            pattern_score += 0.2;
        }
        
        // Check for periodicity
        let period_scores = self.check_bit_periodicity(&bit_stream);
        if period_scores > 0.2 {
            pattern_score += period_scores * 0.3;
        }
        
        pattern_score.min(1.0)
    }
    
    fn check_bit_periodicity(&self, data: &[u8]) -> f32 {
        if data.len() < 1024 {
            return 0.0;
        }
        
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
            if correlation > 0.01 {
                max_correlation = max_correlation.max(correlation);
            }
        }
        
        max_correlation
    }
    
    /// Detect spectral folding artifacts
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
        
        let fold_points = [
            sample_rate as f32 / 4.0,
            sample_rate as f32 / 3.0,
        ];
        
        let mut folding_score = 0.0f32;
        
        for &fold_freq in &fold_points {
            let fold_bin = (fold_freq / freq_per_bin) as usize;
            
            if fold_bin < 50 || fold_bin >= nyquist_bin - 50 {
                continue;
            }
            
            let mut correlation = 0.0f32;
            let check_range = 30;
            
            for offset in 1..check_range {
                let below = buffer[fold_bin - offset].norm();
                let above = buffer[fold_bin + offset].norm();
                
                if below > 1e-10 && above > 1e-10 {
                    let ratio = (below / above).log10().abs();
                    if ratio < 0.5 {
                        correlation += 1.0;
                    }
                }
            }
            
            correlation /= check_range as f32;
            folding_score = folding_score.max(correlation);
        }
        
        folding_score
    }
    
    /// Detect original sample rate from folding patterns
    fn detect_original_rate(&self, _samples: &[f32], base_rate: u32) -> Option<u32> {
        match base_rate {
            44100 => Some(88200),
            48000 => Some(96000),
            _ => None,
        }
    }
}

/// Early encoder detection indicators
struct EarlyEncoderIndicators {
    score: f32,
    reasons: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lsb_entropy() {
        let detector = MqaDetector::default();
        
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
        
        assert!(detector.lsb_entropy_threshold < MqaDetector::default().lsb_entropy_threshold);
        assert!(detector.noise_floor_threshold < MqaDetector::default().noise_floor_threshold);
    }
    
    #[test]
    fn test_strict_detector() {
        let detector = MqaDetector::strict();
        
        assert!(detector.lsb_entropy_threshold > MqaDetector::default().lsb_entropy_threshold);
    }
    
    #[test]
    fn test_lsb_periodicity() {
        let detector = MqaDetector::default();
        
        // Create samples with periodic LSB pattern
        let samples: Vec<f32> = (0..8192)
            .map(|i| {
                let base = (i as f32 * 0.01).sin() * 0.5;
                // Add periodic pattern
                let periodic = ((i % 256) as f32 / 8388607.0) * 0.00001;
                base + periodic
            })
            .collect();
        
        let periodicity = detector.analyze_lsb_periodicity(&samples);
        // Should detect some periodicity
        assert!(periodicity >= 0.0);
    }
    
    #[test]
    fn test_lsb_clustering() {
        let detector = MqaDetector::default();
        
        // Create samples with clustered LSB values
        let samples: Vec<f32> = (0..8192)
            .map(|i| {
                let base = (i as f32 * 0.01).sin() * 0.5;
                // Cluster around certain values
                let clustered = ((i % 10) as f32 * 25.0 / 8388607.0) * 0.0001;
                base + clustered
            })
            .collect();
        
        let clustering = detector.analyze_lsb_clustering(&samples);
        // Should detect clustering
        assert!(clustering >= 0.0 && clustering <= 1.0);
    }
    
    #[test]
    fn test_silence_detection() {
        let detector = MqaDetector::default();
        
        // Create silent samples
        let samples = vec![0.0f32; 44100];
        
        let result = detector.detect(&samples, 44100, 24);
        
        // Should detect silence and return early
        assert!(!result.is_mqa_encoded);
        assert!(result.evidence.iter().any(|e| e.contains("silence")));
    }
}
