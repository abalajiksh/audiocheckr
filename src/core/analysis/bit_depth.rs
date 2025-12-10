// src/core/analysis/bit_depth.rs
//
// Advanced bit depth detection using multiple analysis methods.
// Detects fake 24-bit files that are actually 16-bit with zero-padded LSBs.
//
// Key insight: Genuine 24-bit audio has random distribution in lower 8 bits.
// Fake 24-bit (upscaled 16-bit) has nearly ALL samples with lower 8 bits as 0x00 or 0x80.
// We need VERY high thresholds to avoid false positives on real 24-bit audio.

use std::collections::HashMap;

/// Comprehensive bit depth analysis result
#[derive(Debug, Clone)]
pub struct BitDepthAnalysis {
    /// Claimed bit depth from file metadata
    pub claimed_bit_depth: u32,
    /// Detected actual bit depth
    pub actual_bit_depth: u32,
    /// Confidence in detection (0.0 to 1.0)
    pub confidence: f32,
    /// Individual method results for transparency
    pub method_results: BitDepthMethodResults,
    /// Whether a mismatch was detected
    pub is_mismatch: bool,
    /// Detailed evidence for the detection
    pub evidence: Vec<String>,
}

/// Results from individual detection methods
#[derive(Debug, Clone)]
pub struct BitDepthMethodResults {
    /// LSB precision analysis result
    pub lsb_method: u32,
    /// Histogram analysis result
    pub histogram_method: u32,
    /// Quantization noise analysis result
    pub noise_method: u32,
    /// Value clustering analysis result
    pub clustering_method: u32,
}

/// Analyze bit depth using multiple methods
pub fn analyze_bit_depth(audio: &crate::core::AudioData) -> BitDepthAnalysis {
    let samples = &audio.samples;
    
    if samples.is_empty() {
        return BitDepthAnalysis {
            claimed_bit_depth: audio.claimed_bit_depth,
            actual_bit_depth: audio.claimed_bit_depth,
            confidence: 0.0,
            method_results: BitDepthMethodResults {
                lsb_method: audio.claimed_bit_depth,
                histogram_method: audio.claimed_bit_depth,
                noise_method: audio.claimed_bit_depth,
                clustering_method: audio.claimed_bit_depth,
            },
            is_mismatch: false,
            evidence: vec!["No samples to analyze".to_string()],
        };
    }

    // Run all detection methods
    let lsb_result = analyze_lsb_precision(samples);
    let histogram_result = analyze_histogram(samples);
    let noise_result = analyze_quantization_noise(samples);
    let clustering_result = analyze_value_clustering(samples);
    
    // Collect evidence
    let mut evidence = Vec::new();
    evidence.push(format!("LSB analysis: {} bit (confidence: {:.1}%)", 
        lsb_result.0, lsb_result.1 * 100.0));
    evidence.push(format!("Histogram analysis: {} bit (confidence: {:.1}%)", 
        histogram_result.0, histogram_result.1 * 100.0));
    evidence.push(format!("Noise floor analysis: {} bit (confidence: {:.1}%)", 
        noise_result.0, noise_result.1 * 100.0));
    evidence.push(format!("Clustering analysis: {} bit (confidence: {:.1}%)", 
        clustering_result.0, clustering_result.1 * 100.0));
    
    // Conservative voting: require strong agreement to flag as 16-bit
    // We want to AVOID false positives on genuine 24-bit audio
    let (actual_bit_depth, confidence) = vote_bit_depth_conservative(
        &[lsb_result, histogram_result, noise_result, clustering_result],
        audio.claimed_bit_depth,
    );
    
    // Very conservative mismatch detection:
    // - Require high confidence from multiple methods
    // - Only flag if claimed 24-bit but detected as 16-bit
    let is_mismatch = actual_bit_depth == 16 
        && audio.claimed_bit_depth >= 24
        && confidence >= 0.85;  // Increased threshold
    
    if is_mismatch {
        evidence.push(format!(
            "MISMATCH: File claims {} bit but analysis indicates {} bit with {:.0}% confidence",
            audio.claimed_bit_depth, actual_bit_depth, confidence * 100.0
        ));
    }

    BitDepthAnalysis {
        claimed_bit_depth: audio.claimed_bit_depth,
        actual_bit_depth,
        confidence,
        method_results: BitDepthMethodResults {
            lsb_method: lsb_result.0,
            histogram_method: histogram_result.0,
            noise_method: noise_result.0,
            clustering_method: clustering_result.0,
        },
        is_mismatch,
        evidence,
    }
}

/// Analyze LSB (Least Significant Bit) precision
/// 
/// Genuine 24-bit audio: Random distribution of trailing zeros (0-7 mostly)
/// Fake 24-bit (16-bit upscaled): Nearly ALL samples have exactly 8 trailing zeros
fn analyze_lsb_precision(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(200000);
    
    let mut trailing_zero_counts: HashMap<u32, u32> = HashMap::new();
    let mut total_samples = 0u32;
    
    for &sample in samples.iter().take(test_samples) {
        // Skip near-silence (these naturally have many trailing zeros)
        if sample.abs() < 1e-5 {
            continue;
        }
        
        // Scale to 24-bit range
        let scaled = (sample * 8388607.0).round() as i32;
        
        if scaled != 0 {
            let trailing = scaled.trailing_zeros().min(24);
            *trailing_zero_counts.entry(trailing).or_insert(0) += 1;
            total_samples += 1;
        }
    }
    
    if total_samples < 1000 {
        // Not enough samples - assume claimed bit depth is correct
        return (24, 0.3);
    }
    
    // Count samples with EXACTLY 8 trailing zeros (the signature of 16-bit upscaled)
    let samples_with_exactly_8 = *trailing_zero_counts.get(&8).unwrap_or(&0);
    let ratio_exactly_8 = samples_with_exactly_8 as f32 / total_samples as f32;
    
    // Count samples with 8 or more trailing zeros
    let samples_with_8plus_zeros: u32 = trailing_zero_counts.iter()
        .filter(|(&zeros, _)| zeros >= 8)
        .map(|(_, &count)| count)
        .sum();
    let ratio_8plus = samples_with_8plus_zeros as f32 / total_samples as f32;
    
    // Count samples with low trailing zeros (0-3) - indicates genuine 24-bit activity
    let samples_with_low_zeros: u32 = trailing_zero_counts.iter()
        .filter(|(&zeros, _)| zeros <= 3)
        .map(|(_, &count)| count)
        .sum();
    let ratio_low_zeros = samples_with_low_zeros as f32 / total_samples as f32;
    
    // VERY CONSERVATIVE thresholds to avoid false positives:
    // - True 16-bit upscaled will have >95% samples with exactly 8 trailing zeros
    // - True 24-bit will have significant activity in lower bits
    
    if ratio_exactly_8 > 0.95 && ratio_low_zeros < 0.02 {
        // Almost certainly 16-bit upscaled - very little activity in lower 8 bits
        (16, 0.95)
    } else if ratio_8plus > 0.90 && ratio_low_zeros < 0.05 {
        // Very likely 16-bit upscaled
        (16, 0.85)
    } else if ratio_low_zeros > 0.30 {
        // Significant activity in lower bits - genuine 24-bit
        (24, 0.90)
    } else if ratio_low_zeros > 0.15 {
        // Some activity in lower bits - likely genuine 24-bit
        (24, 0.75)
    } else if ratio_8plus > 0.70 {
        // Suspicious but not conclusive - could be quiet recording
        (16, 0.55)
    } else {
        // Default to 24-bit to avoid false positives
        (24, 0.60)
    }
}

fn analyze_histogram(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(200000);
    
    let mut values_16bit: HashMap<i32, u32> = HashMap::new();
    let mut values_24bit: HashMap<i32, u32> = HashMap::new();
    
    for &sample in samples.iter().take(test_samples) {
        // Skip near-silence
        if sample.abs() < 1e-5 {
            continue;
        }
        
        let q16 = (sample * 32767.0).round() as i32;
        let q24 = (sample * 8388607.0).round() as i32;
        
        *values_16bit.entry(q16).or_insert(0) += 1;
        *values_24bit.entry(q24).or_insert(0) += 1;
    }
    
    if values_16bit.is_empty() {
        return (24, 0.3);
    }
    
    let unique_16 = values_16bit.len();
    let unique_24 = values_24bit.len();
    
    // Ratio of unique 24-bit values to unique 16-bit values
    // For genuine 24-bit: ratio should be >> 1 (many more unique values)
    // For 16-bit upscaled: ratio should be ~1 (same unique values, just scaled)
    let ratio = unique_24 as f32 / unique_16.max(1) as f32;
    
    // Check how many 24-bit values fall exactly on 256-boundaries (multiples of 256)
    // This is the signature of upscaled 16-bit
    let multiples_of_256: usize = values_24bit.keys()
        .filter(|&&v| v != 0 && v.abs() % 256 == 0)
        .count();
    let boundary_ratio = multiples_of_256 as f32 / unique_24.max(1) as f32;
    
    // VERY CONSERVATIVE: Only flag as 16-bit if evidence is overwhelming
    if ratio < 1.2 && boundary_ratio > 0.85 {
        // Almost certainly 16-bit: very few unique values, most on boundaries
        (16, 0.95)
    } else if ratio < 1.5 && boundary_ratio > 0.70 {
        // Likely 16-bit upscaled
        (16, 0.80)
    } else if ratio > 100.0 {
        // Definitely 24-bit: massive increase in unique values
        (24, 0.95)
    } else if ratio > 20.0 {
        // Very likely genuine 24-bit
        (24, 0.85)
    } else if ratio > 5.0 {
        // Probably genuine 24-bit
        (24, 0.70)
    } else {
        // Inconclusive - default to 24-bit to avoid false positives
        (24, 0.50)
    }
}

fn analyze_quantization_noise(samples: &[f32]) -> (u32, f32) {
    let section_size = 16384;
    let num_sections = (samples.len() / section_size).min(20);
    
    if num_sections == 0 {
        return (24, 0.3);
    }
    
    // Find quiet sections to analyze noise floor
    let mut quiet_sections: Vec<(usize, f32)> = Vec::new();
    
    for i in 0..num_sections {
        let start = i * section_size;
        let end = (start + section_size).min(samples.len());
        let section = &samples[start..end];
        
        let rms = (section.iter().map(|s| s * s).sum::<f32>() / section.len() as f32).sqrt();
        
        // Look for quiet sections that aren't complete silence
        if rms > 1e-7 && rms < 0.01 {
            quiet_sections.push((start, rms));
        }
    }
    
    if quiet_sections.is_empty() {
        // No quiet sections - can't analyze noise floor reliably
        return (24, 0.40);
    }
    
    quiet_sections.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    
    let mut lsb_noise_sum = 0.0f32;
    let mut count = 0;
    
    for (start, _) in quiet_sections.iter().take(5) {
        let end = (*start + section_size).min(samples.len());
        let section = &samples[*start..end];
        
        // Analyze sample-to-sample differences in quiet sections
        let diffs: Vec<f32> = section.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-10 && d < 0.001)
            .collect();
        
        if diffs.len() > 100 {
            let mut sorted_diffs = diffs.clone();
            sorted_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // Use 10th percentile as estimate of LSB step
            let noise_step = sorted_diffs[sorted_diffs.len() / 10];
            lsb_noise_sum += noise_step;
            count += 1;
        }
    }
    
    if count == 0 {
        return (24, 0.40);
    }
    
    let avg_noise_step = lsb_noise_sum / count as f32;
    
    // Expected LSB step sizes
    let step_16bit = 1.0 / 32768.0;   // ~3.05e-5
    let step_24bit = 1.0 / 8388608.0; // ~1.19e-7
    
    // CONSERVATIVE thresholds
    if avg_noise_step > step_16bit * 0.7 {
        // Noise floor matches 16-bit quantization
        (16, 0.80)
    } else if avg_noise_step < step_24bit * 50.0 {
        // Very fine noise floor - genuine 24-bit
        (24, 0.85)
    } else if avg_noise_step < step_16bit * 0.2 {
        // Noise floor finer than 16-bit would allow
        (24, 0.70)
    } else {
        // Inconclusive
        (24, 0.45)
    }
}

fn analyze_value_clustering(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(200000);
    let mut lsb_distribution: HashMap<u8, u32> = HashMap::new();
    let mut total_non_silent = 0u32;
    
    for &sample in samples.iter().take(test_samples) {
        // Skip silence
        if sample.abs() < 1e-5 {
            continue;
        }
        
        let q24 = (sample * 8388607.0).round() as i32;
        // Get lower 8 bits
        let lsb_8 = (q24.abs() & 0xFF) as u8;
        
        *lsb_distribution.entry(lsb_8).or_insert(0) += 1;
        total_non_silent += 1;
    }
    
    if lsb_distribution.is_empty() || total_non_silent < 1000 {
        return (24, 0.3);
    }
    
    let unique_lsb_values = lsb_distribution.len();
    
    // For 16-bit upscaled: almost all samples have LSB = 0x00 or 0x80
    let count_00 = *lsb_distribution.get(&0x00).unwrap_or(&0);
    let count_80 = *lsb_distribution.get(&0x80).unwrap_or(&0);
    let concentrated_ratio = (count_00 + count_80) as f32 / total_non_silent as f32;
    
    // Calculate entropy of LSB distribution
    let entropy = calculate_entropy(&lsb_distribution);
    let max_entropy = 8.0; // Maximum for 256 values
    let normalized_entropy = entropy / max_entropy;
    
    // VERY CONSERVATIVE thresholds
    if concentrated_ratio > 0.95 && unique_lsb_values < 5 {
        // Almost all samples at 0x00 or 0x80 - definitely 16-bit upscaled
        (16, 0.95)
    } else if concentrated_ratio > 0.85 && unique_lsb_values < 20 {
        // Very likely 16-bit upscaled
        (16, 0.85)
    } else if normalized_entropy > 0.90 && unique_lsb_values > 200 {
        // High entropy, many unique values - definitely genuine 24-bit
        (24, 0.95)
    } else if normalized_entropy > 0.80 && unique_lsb_values > 150 {
        // Good entropy - likely genuine 24-bit
        (24, 0.80)
    } else if unique_lsb_values > 100 {
        // Moderate variety - probably genuine 24-bit
        (24, 0.65)
    } else if concentrated_ratio > 0.70 {
        // Suspicious clustering but not definitive
        (16, 0.55)
    } else {
        // Default to 24-bit to avoid false positives
        (24, 0.50)
    }
}

fn calculate_entropy(distribution: &HashMap<u8, u32>) -> f32 {
    let total: u32 = distribution.values().sum();
    if total == 0 {
        return 0.0;
    }
    
    distribution.values()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f32 / total as f32;
            -p * p.log2()
        })
        .sum()
}

/// Conservative voting system that requires strong evidence to flag as 16-bit
fn vote_bit_depth_conservative(
    results: &[(u32, f32)],
    claimed_bit_depth: u32,
) -> (u32, f32) {
    // Count weighted votes for each bit depth
    let mut vote_16 = 0.0f32;
    let mut vote_24 = 0.0f32;
    let mut high_confidence_16_count = 0;
    
    for &(bit_depth, confidence) in results {
        if bit_depth <= 16 {
            vote_16 += confidence;
            if confidence >= 0.80 {
                high_confidence_16_count += 1;
            }
        } else {
            vote_24 += confidence;
        }
    }
    
    let total = vote_16 + vote_24;
    if total < 0.1 {
        // No clear signal - trust claimed bit depth
        return (claimed_bit_depth, 0.3);
    }
    
    // CONSERVATIVE: Require MULTIPLE high-confidence 16-bit votes to flag
    // This prevents false positives from a single noisy detector
    if vote_16 > vote_24 && high_confidence_16_count >= 3 {
        // Strong consensus for 16-bit
        (16, vote_16 / total)
    } else if vote_16 > vote_24 * 1.5 && high_confidence_16_count >= 2 {
        // Good evidence for 16-bit
        (16, (vote_16 / total) * 0.9)  // Slightly reduce confidence
    } else if vote_24 > vote_16 {
        // Evidence favors 24-bit
        (24, vote_24 / total)
    } else {
        // Ambiguous - default to claimed bit depth to avoid false positives
        (claimed_bit_depth, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_calculation() {
        // Uniform distribution should have high entropy
        let mut uniform: HashMap<u8, u32> = HashMap::new();
        for i in 0..=255 {
            uniform.insert(i, 100);
        }
        let entropy = calculate_entropy(&uniform);
        assert!(entropy > 7.9);

        // Single value should have zero entropy
        let mut single: HashMap<u8, u32> = HashMap::new();
        single.insert(0, 1000);
        let entropy = calculate_entropy(&single);
        assert!(entropy < 0.001);
    }
    
    #[test]
    fn test_lsb_analysis_genuine_24bit() {
        // Simulate genuine 24-bit audio with random LSBs
        use std::f32::consts::PI;
        let samples: Vec<f32> = (0..10000)
            .map(|i| {
                let base = (i as f32 * 0.01 * PI).sin() * 0.5;
                // Add fine detail in lower bits
                let detail = (i as f32 * 0.1234).sin() * 0.0001;
                base + detail
            })
            .collect();
        
        let (bit_depth, confidence) = analyze_lsb_precision(&samples);
        // Should detect as 24-bit (or at least not confident 16-bit)
        assert!(bit_depth == 24 || confidence < 0.7, 
            "Should not confidently detect synthetic 24-bit as 16-bit");
    }
}
