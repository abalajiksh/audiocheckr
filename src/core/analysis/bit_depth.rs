// src/core/analysis/bit_depth.rs
//
// Advanced bit depth detection using multiple analysis methods.
// Detects fake 24-bit files that are actually 16-bit with zero-padded LSBs.

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
    
    // Collect results
    let results = vec![
        lsb_result.0,
        histogram_result.0,
        noise_result.0,
        clustering_result.0,
    ];
    
    // Use voting with weighted confidence
    let (actual_bit_depth, confidence) = vote_bit_depth(&results, &[
        lsb_result.1,
        histogram_result.1,
        noise_result.1,
        clustering_result.1,
    ]);
    
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
    
    let is_mismatch = actual_bit_depth < audio.claimed_bit_depth 
        && (audio.claimed_bit_depth - actual_bit_depth) >= 8
        && confidence > 0.7;
    
    if is_mismatch {
        evidence.push(format!(
            "MISMATCH: File claims {} bit but analysis indicates {} bit",
            audio.claimed_bit_depth, actual_bit_depth
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
fn analyze_lsb_precision(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(100000);
    
    let mut trailing_zero_counts: HashMap<u32, u32> = HashMap::new();
    let mut total_samples = 0u32;
    
    for &sample in samples.iter().take(test_samples) {
        if sample.abs() < 1e-6 {
            continue;
        }
        
        let scaled = (sample * 8388607.0).round() as i32;
        
        if scaled != 0 {
            let trailing = scaled.trailing_zeros().min(24);
            *trailing_zero_counts.entry(trailing).or_insert(0) += 1;
            total_samples += 1;
        }
    }
    
    if total_samples < 1000 {
        return (16, 0.3);
    }
    
    let samples_with_8plus_zeros: u32 = trailing_zero_counts.iter()
        .filter(|(&zeros, _)| zeros >= 8)
        .map(|(_, &count)| count)
        .sum();
    
    let ratio_8plus = samples_with_8plus_zeros as f32 / total_samples as f32;
    let samples_with_exactly_8 = *trailing_zero_counts.get(&8).unwrap_or(&0);
    let ratio_exactly_8 = samples_with_exactly_8 as f32 / total_samples as f32;
    let median_zeros = calculate_median_trailing_zeros(&trailing_zero_counts, total_samples);
    
    if ratio_8plus > 0.85 || median_zeros >= 8 {
        let confidence = (ratio_8plus * 0.7 + 0.3).min(0.95);
        (16, confidence)
    } else if ratio_8plus > 0.5 || median_zeros >= 6 {
        (16, 0.6)
    } else if ratio_exactly_8 < 0.1 && median_zeros < 4 {
        let confidence = (1.0 - ratio_8plus) * 0.8;
        (24, confidence)
    } else {
        (24, 0.5)
    }
}

fn calculate_median_trailing_zeros(counts: &HashMap<u32, u32>, total: u32) -> u32 {
    let mut cumulative = 0u32;
    let target = total / 2;
    
    for zeros in 0..=24 {
        cumulative += *counts.get(&zeros).unwrap_or(&0);
        if cumulative >= target {
            return zeros;
        }
    }
    
    0
}

fn analyze_histogram(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(200000);
    
    let mut values_16bit: HashMap<i32, u32> = HashMap::new();
    let mut values_24bit: HashMap<i32, u32> = HashMap::new();
    
    for &sample in samples.iter().take(test_samples) {
        let q16 = (sample * 32767.0).round() as i32;
        let q24 = (sample * 8388607.0).round() as i32;
        
        *values_16bit.entry(q16).or_insert(0) += 1;
        *values_24bit.entry(q24).or_insert(0) += 1;
    }
    
    let unique_16 = values_16bit.len();
    let unique_24 = values_24bit.len();
    let ratio = unique_24 as f32 / unique_16.max(1) as f32;
    
    let multiples_of_256: usize = values_24bit.keys()
        .filter(|&&v| v % 256 == 0 || v % 256 == 255 || v % 256 == 1)
        .count();
    let clustering_ratio = multiples_of_256 as f32 / unique_24 as f32;
    
    if ratio < 1.5 {
        (16, 0.95)
    } else if ratio < 3.0 && clustering_ratio > 0.7 {
        (16, 0.8)
    } else if ratio > 50.0 && clustering_ratio < 0.3 {
        (24, 0.9)
    } else if ratio > 10.0 {
        (24, 0.7)
    } else {
        let confidence = if ratio > 5.0 { 0.6 } else { 0.5 };
        if clustering_ratio > 0.5 {
            (16, confidence)
        } else {
            (24, confidence)
        }
    }
}

fn analyze_quantization_noise(samples: &[f32]) -> (u32, f32) {
    let section_size = 16384;
    let num_sections = (samples.len() / section_size).min(20);
    
    if num_sections == 0 {
        return (16, 0.3);
    }
    
    let mut quiet_sections: Vec<(usize, f32)> = Vec::new();
    
    for i in 0..num_sections {
        let start = i * section_size;
        let end = (start + section_size).min(samples.len());
        let section = &samples[start..end];
        
        let rms = (section.iter().map(|s| s * s).sum::<f32>() / section.len() as f32).sqrt();
        
        if rms > 1e-8 && rms < 0.01 {
            quiet_sections.push((start, rms));
        }
    }
    
    if quiet_sections.is_empty() {
        return analyze_overall_noise(samples);
    }
    
    quiet_sections.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    
    let mut lsb_noise_sum = 0.0f32;
    let mut count = 0;
    
    for (start, _) in quiet_sections.iter().take(5) {
        let end = (*start + section_size).min(samples.len());
        let section = &samples[*start..end];
        
        let diffs: Vec<f32> = section.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-10 && d < 0.001)
            .collect();
        
        if diffs.len() > 100 {
            let mut sorted_diffs = diffs.clone();
            sorted_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let noise_step = sorted_diffs[sorted_diffs.len() / 10];
            lsb_noise_sum += noise_step;
            count += 1;
        }
    }
    
    if count == 0 {
        return (24, 0.5);
    }
    
    let avg_noise_step = lsb_noise_sum / count as f32;
    let step_16bit = 1.0 / 32768.0;
    let step_24bit = 1.0 / 8388608.0;
    
    if avg_noise_step > step_16bit * 0.5 {
        (16, 0.85)
    } else if avg_noise_step < step_24bit * 10.0 {
        (24, 0.8)
    } else if avg_noise_step < step_16bit * 0.1 {
        (24, 0.7)
    } else {
        (16, 0.6)
    }
}

fn analyze_overall_noise(samples: &[f32]) -> (u32, f32) {
    let hp_samples: Vec<f32> = samples.windows(2)
        .map(|w| w[1] - w[0])
        .collect();
    
    let hp_rms = (hp_samples.iter().map(|s| s * s).sum::<f32>() / hp_samples.len() as f32).sqrt();
    
    if hp_rms < 1e-5 {
        (24, 0.5)
    } else if hp_rms > 1e-4 {
        (16, 0.5)
    } else {
        (16, 0.4)
    }
}

fn analyze_value_clustering(samples: &[f32]) -> (u32, f32) {
    let test_samples = samples.len().min(100000);
    let mut lsb_distribution: HashMap<u8, u32> = HashMap::new();
    
    for &sample in samples.iter().take(test_samples) {
        if sample.abs() < 1e-6 {
            continue;
        }
        
        let q24 = (sample * 8388607.0).round() as i32;
        let lsb_8 = (q24.abs() & 0xFF) as u8;
        
        *lsb_distribution.entry(lsb_8).or_insert(0) += 1;
    }
    
    if lsb_distribution.is_empty() {
        return (16, 0.3);
    }
    
    let unique_lsb_values = lsb_distribution.len();
    let count_00 = *lsb_distribution.get(&0x00).unwrap_or(&0);
    let count_80 = *lsb_distribution.get(&0x80).unwrap_or(&0);
    let total: u32 = lsb_distribution.values().sum();
    let concentrated_ratio = (count_00 + count_80) as f32 / total as f32;
    let entropy = calculate_entropy(&lsb_distribution);
    let max_entropy = 8.0;
    let normalized_entropy = entropy / max_entropy;
    
    if unique_lsb_values < 10 || concentrated_ratio > 0.8 {
        (16, 0.9)
    } else if normalized_entropy > 0.95 && unique_lsb_values > 200 {
        (24, 0.85)
    } else if normalized_entropy < 0.5 || unique_lsb_values < 50 {
        (16, 0.75)
    } else {
        if normalized_entropy > 0.8 {
            (24, 0.6)
        } else {
            (16, 0.55)
        }
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

fn vote_bit_depth(results: &[u32], confidences: &[f32]) -> (u32, f32) {
    let mut vote_16 = 0.0f32;
    let mut vote_24 = 0.0f32;
    
    for (i, &result) in results.iter().enumerate() {
        let weight = confidences[i];
        if result <= 16 {
            vote_16 += weight;
        } else {
            vote_24 += weight;
        }
    }
    
    let total = vote_16 + vote_24;
    if total < 0.1 {
        return (16, 0.3);
    }
    
    if vote_16 > vote_24 {
        (16, vote_16 / total)
    } else {
        (24, vote_24 / total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_calculation() {
        let mut uniform: HashMap<u8, u32> = HashMap::new();
        for i in 0..=255 {
            uniform.insert(i, 100);
        }
        let entropy = calculate_entropy(&uniform);
        assert!(entropy > 7.9);

        let mut single: HashMap<u8, u32> = HashMap::new();
        single.insert(0, 1000);
        let entropy = calculate_entropy(&single);
        assert!(entropy < 0.001);
    }
}
