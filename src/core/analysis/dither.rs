// src/core/analysis/dither.rs
// Module for detecting dithering patterns in audio files
// This helps distinguish genuine 24-bit audio from upscaled 16-bit audio

use std::f64::consts::PI;

/// Common dithering algorithms used in audio production
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DitherType {
    /// No dithering detected (likely genuine bit depth or truncation)
    None,
    /// Rectangular/flat PDF dither (±0.5 LSB uniform noise)
    Rectangular,
    /// Triangular PDF dither (±1 LSB triangular noise) - most common
    Triangular,
    /// High-pass triangular dither (shaped to reduce audibility)
    HighPassTriangular,
    /// Noise shaping dither (POW-R, etc.)
    NoiseShaped,
    /// Unknown dithering pattern
    Unknown,
}

/// Results from dithering analysis
#[derive(Debug, Clone)]
pub struct DitherAnalysis {
    /// Detected dither type
    pub dither_type: DitherType,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,
    /// Effective bit depth detected
    pub effective_bits: u8,
    /// Container bit depth
    pub container_bits: u8,
    /// Is this likely upscaled from lower bit depth?
    pub is_upscaled: bool,
    /// LSB activity ratio (proportion of samples with LSB variation)
    pub lsb_activity: f32,
    /// Noise floor estimate in dBFS
    pub noise_floor_db: f32,
    /// Spectral characteristics of noise
    pub noise_spectral_tilt: f32,
}

/// Analyzer for detecting dithering patterns
pub struct DitherAnalyzer {
    /// FFT size for noise analysis
    fft_size: usize,
    /// Number of segments to analyze
    num_segments: usize,
}

impl Default for DitherAnalyzer {
    fn default() -> Self {
        Self {
            fft_size: 4096,
            num_segments: 32,
        }
    }
}

impl DitherAnalyzer {
    pub fn new(fft_size: usize, num_segments: usize) -> Self {
        Self { fft_size, num_segments }
    }

    /// Main analysis function - takes audio samples normalized to [-1.0, 1.0]
    pub fn analyze(&self, samples: &[f32], container_bits: u8) -> DitherAnalysis {
        // Step 1: Extract LSB patterns
        let lsb_stats = self.analyze_lsb_patterns(samples, container_bits);
        
        // Step 2: Analyze noise floor characteristics
        let noise_stats = self.analyze_noise_floor(samples);
        
        // Step 3: Detect effective bit depth
        let effective_bits = self.detect_effective_bits(samples, container_bits);
        
        // Step 4: Classify dither type based on noise PDF
        let (dither_type, confidence) = self.classify_dither_type(&lsb_stats, &noise_stats);
        
        // Step 5: Determine if upscaled
        let is_upscaled = effective_bits < container_bits && 
                          (dither_type != DitherType::None || lsb_stats.zero_lsb_ratio > 0.99);

        DitherAnalysis {
            dither_type,
            confidence,
            effective_bits,
            container_bits,
            is_upscaled,
            lsb_activity: lsb_stats.activity_ratio,
            noise_floor_db: noise_stats.floor_db,
            noise_spectral_tilt: noise_stats.spectral_tilt,
        }
    }

    /// Analyze patterns in the least significant bits
    fn analyze_lsb_patterns(&self, samples: &[f32], container_bits: u8) -> LsbStats {
        let scale = (1u32 << (container_bits - 1)) as f32;
        
        // Analyze multiple LSB levels (8 LSBs for 24-bit)
        let analyze_bits = 8.min(container_bits);
        let mut bit_activity = vec![0.0f32; analyze_bits as usize];
        let mut histogram_8bit = vec![0u64; 256];  // For 8 LSBs
        
        let mut prev_sample_int = 0i32;
        let mut lsb_deltas = Vec::with_capacity(samples.len());
        let mut zero_lsb_count = 0u64;
        
        for (i, &sample) in samples.iter().enumerate() {
            let sample_int = (sample * scale) as i32;
            
            // Count activity in each bit position
            for bit in 0..analyze_bits {
                let mask = 1 << bit;
                if (sample_int & mask) != 0 {
                    bit_activity[bit as usize] += 1.0;
                }
            }
            
            // Build histograms
            let lsb_8 = (sample_int & 0xFF) as usize;
            
            histogram_8bit[lsb_8] += 1;
            
            // Check for zero LSBs (indicates possible truncation/upscaling)
            if lsb_8 == 0 {
                zero_lsb_count += 1;
            }
            
            // Track LSB delta patterns
            if i > 0 {
                let delta = ((sample_int & 0xFF) as i32) - ((prev_sample_int & 0xFF) as i32);
                lsb_deltas.push(delta);
            }
            prev_sample_int = sample_int;
        }
        
        let n = samples.len() as f32;
        
        // Normalize bit activity
        for activity in &mut bit_activity {
            *activity /= n;
        }
        
        // Calculate PDF shape metrics for dither classification
        let pdf_flatness = self.calculate_pdf_flatness(&histogram_8bit);
        let pdf_triangularity = self.calculate_pdf_triangularity(&histogram_8bit);
        
        // Calculate delta distribution metrics (for triangular dither detection)
        let delta_pdf_shape = self.analyze_delta_distribution(&lsb_deltas);
        
        LsbStats {
            bit_activity,
            zero_lsb_ratio: zero_lsb_count as f32 / n,
            activity_ratio: 1.0 - (zero_lsb_count as f32 / n),
            pdf_flatness,
            pdf_triangularity,
            delta_triangularity: delta_pdf_shape.triangularity,
            histogram_entropy: self.calculate_entropy(&histogram_8bit),
        }
    }

    /// Calculate how flat the PDF is (rectangular dither = flat)
    fn calculate_pdf_flatness(&self, histogram: &[u64]) -> f32 {
        let total: u64 = histogram.iter().sum();
        if total == 0 {
            return 0.0;
        }
        
        let expected = total as f64 / histogram.len() as f64;
        let variance: f64 = histogram.iter()
            .map(|&count| {
                let diff = count as f64 - expected;
                diff * diff
            })
            .sum::<f64>() / histogram.len() as f64;
        
        // Lower variance = flatter distribution
        let std_dev = variance.sqrt();
        let cv = std_dev / expected; // Coefficient of variation
        
        // Convert to flatness score (1.0 = perfectly flat)
        (1.0 / (1.0 + cv)) as f32
    }

    /// Calculate how triangular the PDF is
    fn calculate_pdf_triangularity(&self, histogram: &[u64]) -> f32 {
        let total: u64 = histogram.iter().sum();
        if total == 0 {
            return 0.0;
        }
        
        let n = histogram.len();
        let center = n / 2;
        
        // Generate ideal triangular PDF
        let ideal: Vec<f64> = (0..n)
            .map(|i| {
                let dist_from_center = (i as i32 - center as i32).abs() as f64;
                let max_dist = center as f64;
                (max_dist - dist_from_center) / max_dist
            })
            .collect();
        
        let ideal_sum: f64 = ideal.iter().sum();
        let ideal_normalized: Vec<f64> = ideal.iter().map(|&x| x / ideal_sum).collect();
        
        // Compare actual distribution to ideal triangular
        let actual_normalized: Vec<f64> = histogram.iter()
            .map(|&count| count as f64 / total as f64)
            .collect();
        
        // Calculate correlation
        let mean_actual = 1.0 / n as f64;
        let mean_ideal = 1.0 / n as f64;
        
        let mut cov = 0.0;
        let mut var_actual = 0.0;
        let mut var_ideal = 0.0;
        
        for i in 0..n {
            let diff_actual = actual_normalized[i] - mean_actual;
            let diff_ideal = ideal_normalized[i] - mean_ideal;
            cov += diff_actual * diff_ideal;
            var_actual += diff_actual * diff_actual;
            var_ideal += diff_ideal * diff_ideal;
        }
        
        if var_actual > 0.0 && var_ideal > 0.0 {
            (cov / (var_actual.sqrt() * var_ideal.sqrt())).max(0.0) as f32
        } else {
            0.0
        }
    }

    /// Analyze the distribution of LSB deltas
    fn analyze_delta_distribution(&self, deltas: &[i32]) -> DeltaStats {
        if deltas.is_empty() {
            return DeltaStats::default();
        }
        
        // Build histogram of deltas (expect range roughly -255 to 255 for 8-bit LSB)
        let mut histogram = vec![0u64; 512];
        let offset = 256i32;
        
        for &delta in deltas {
            let idx = (delta + offset).clamp(0, 511) as usize;
            histogram[idx] += 1;
        }
        
        // Triangular dither produces triangular delta distribution
        let triangularity = self.calculate_pdf_triangularity(&histogram);
        
        DeltaStats {
            triangularity,
        }
    }

    /// Calculate entropy of a distribution
    fn calculate_entropy(&self, histogram: &[u64]) -> f32 {
        let total: u64 = histogram.iter().sum();
        if total == 0 {
            return 0.0;
        }
        
        let mut entropy = 0.0f64;
        for &count in histogram {
            if count > 0 {
                let p = count as f64 / total as f64;
                entropy -= p * p.log2();
            }
        }
        
        entropy as f32
    }

    /// Analyze noise floor characteristics
    fn analyze_noise_floor(&self, samples: &[f32]) -> NoiseStats {
        if samples.len() < self.fft_size {
            return NoiseStats::default();
        }
        
        // Find quiet sections to analyze noise floor
        let segment_size = samples.len() / self.num_segments;
        let mut quiet_segments: Vec<(usize, f32)> = Vec::new();
        
        for i in 0..self.num_segments {
            let start = i * segment_size;
            let end = start + segment_size.min(samples.len() - start);
            let segment = &samples[start..end];
            
            // Calculate RMS
            let rms: f32 = (segment.iter().map(|&x| x * x).sum::<f32>() / segment.len() as f32).sqrt();
            quiet_segments.push((start, rms));
        }
        
        // Sort by RMS and take quietest segments
        quiet_segments.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Analyze FFT of quiet segments to get noise spectral shape
        let mut spectral_accumulator = vec![0.0f64; self.fft_size / 2];
        let mut analyzed_count = 0;
        
        for (start, rms) in quiet_segments.iter().take(self.num_segments / 4) {
            if *rms < 0.01 {  // Only analyze very quiet sections
                let segment_end = (*start + self.fft_size).min(samples.len());
                if segment_end - *start >= self.fft_size {
                    let spectrum = self.compute_magnitude_spectrum(&samples[*start..*start + self.fft_size]);
                    for (i, &mag) in spectrum.iter().enumerate() {
                        spectral_accumulator[i] += mag as f64;
                    }
                    analyzed_count += 1;
                }
            }
        }
        
        if analyzed_count == 0 {
            return NoiseStats::default();
        }
        
        // Average spectrum
        for val in &mut spectral_accumulator {
            *val /= analyzed_count as f64;
        }
        
        // Calculate spectral tilt (positive = high-frequency emphasis)
        let spectral_tilt = self.calculate_spectral_tilt(&spectral_accumulator);
        
        // Estimate noise floor in dB
        let avg_magnitude: f64 = spectral_accumulator.iter().sum::<f64>() / spectral_accumulator.len() as f64;
        let floor_db = if avg_magnitude > 0.0 {
            (20.0 * avg_magnitude.log10()) as f32
        } else {
            -120.0
        };
        
        NoiseStats {
            floor_db,
            spectral_tilt: spectral_tilt as f32,
            is_shaped: spectral_tilt > 2.0,  // Significant high-frequency boost indicates shaping
        }
    }

    /// Simple FFT magnitude computation (you'll want to use rustfft in production)
    fn compute_magnitude_spectrum(&self, samples: &[f32]) -> Vec<f32> {
        // Simplified DFT for demonstration - replace with rustfft in production
        let n = samples.len();
        let mut spectrum = vec![0.0f32; n / 2];
        
        for k in 0..n/2 {
            let mut real = 0.0f64;
            let mut imag = 0.0f64;
            
            for (i, &sample) in samples.iter().enumerate() {
                let angle = -2.0 * PI * k as f64 * i as f64 / n as f64;
                real += sample as f64 * angle.cos();
                imag += sample as f64 * angle.sin();
            }
            
            spectrum[k] = ((real * real + imag * imag).sqrt() / n as f64) as f32;
        }
        
        spectrum
    }

    /// Calculate spectral tilt in dB/octave
    fn calculate_spectral_tilt(&self, spectrum: &[f64]) -> f64 {
        if spectrum.len() < 2 {
            return 0.0;
        }
        
        // Use linear regression on log-log plot
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;
        let mut count = 0.0;
        
        for (i, &mag) in spectrum.iter().enumerate().skip(1) {  // Skip DC
            if mag > 1e-10 {
                let x = (i as f64).ln();
                let y = mag.ln();
                sum_x += x;
                sum_y += y;
                sum_xy += x * y;
                sum_xx += x * x;
                count += 1.0;
            }
        }
        
        if count < 2.0 {
            return 0.0;
        }
        
        // Slope of linear regression
        let slope = (count * sum_xy - sum_x * sum_y) / (count * sum_xx - sum_x * sum_x);
        
        // Convert to dB/octave
        slope * 20.0 / 2.0_f64.ln()
    }

    /// Detect effective bit depth of the audio
    fn detect_effective_bits(&self, samples: &[f32], container_bits: u8) -> u8 {
        let scale = (1u64 << (container_bits - 1)) as f64;
        
        // For each potential bit depth, check if lower bits are active
        for test_bits in (8..=container_bits).rev() {
            let mask = (1u64 << (container_bits - test_bits)) - 1;
            
            let mut active_count = 0u64;
            let mut total_count = 0u64;
            
            for &sample in samples {
                let sample_int = ((sample as f64) * scale) as i64;
                let lower_bits = (sample_int.unsigned_abs()) & mask;
                
                if lower_bits != 0 {
                    active_count += 1;
                }
                total_count += 1;
            }
            
            let activity_ratio = active_count as f64 / total_count as f64;
            
            // If less than 1% of samples have activity in lower bits,
            // effective depth is at most test_bits
            if activity_ratio < 0.01 {
                return test_bits;
            }
        }
        
        container_bits
    }

    /// Classify the type of dithering applied
    fn classify_dither_type(&self, lsb: &LsbStats, noise: &NoiseStats) -> (DitherType, f32) {
        // High zero-LSB ratio = likely truncation or no dither
        if lsb.zero_lsb_ratio > 0.99 {
            return (DitherType::None, 0.95);
        }
        
        // Very flat PDF with high LSB activity = rectangular dither
        if lsb.pdf_flatness > 0.85 && lsb.activity_ratio > 0.9 {
            return (DitherType::Rectangular, lsb.pdf_flatness);
        }
        
        // Triangular PDF = triangular dither
        if lsb.pdf_triangularity > 0.7 {
            // Check if it's high-pass shaped
            if noise.spectral_tilt > 3.0 {
                return (DitherType::HighPassTriangular, lsb.pdf_triangularity * 0.9);
            }
            return (DitherType::Triangular, lsb.pdf_triangularity);
        }
        
        // Strong noise shaping = POW-R or similar
        if noise.is_shaped && noise.spectral_tilt > 6.0 {
            return (DitherType::NoiseShaped, 0.7);
        }
        
        // LSB activity but no clear pattern
        if lsb.activity_ratio > 0.5 {
            return (DitherType::Unknown, 0.4);
        }
        
        (DitherType::None, 0.5)
    }
}

/// Statistics from LSB analysis
#[derive(Debug, Default)]
struct LsbStats {
    bit_activity: Vec<f32>,
    zero_lsb_ratio: f32,
    activity_ratio: f32,
    pdf_flatness: f32,
    pdf_triangularity: f32,
    delta_triangularity: f32,
    histogram_entropy: f32,
}

/// Statistics from delta analysis
#[derive(Debug, Default)]
struct DeltaStats {
    triangularity: f32,
}

/// Statistics from noise analysis
#[derive(Debug)]
struct NoiseStats {
    floor_db: f32,
    spectral_tilt: f32,
    is_shaped: bool,
}

impl Default for NoiseStats {
    fn default() -> Self {
        Self {
            floor_db: -120.0,
            spectral_tilt: 0.0,
            is_shaped: false,
        }
    }
}

// ============================================================================
// Integration with existing AudioCheckr bit depth analysis
// ============================================================================

/// Enhanced bit depth result combining existing analysis with dithering detection
#[derive(Debug, Clone)]
pub struct EnhancedBitDepthResult {
    /// Container bit depth (what the file claims)
    pub container_bits: u8,
    /// Effective bit depth (actual resolution)
    pub effective_bits: u8,
    /// Dithering analysis results
    pub dither: DitherAnalysis,
    /// Is this likely an upscaled file?
    pub is_upscaled: bool,
    /// Confidence in the upscaling detection
    pub upscale_confidence: f32,
    /// Detailed reason for classification
    pub reason: String,
}

/// Analyze bit depth with dithering detection
pub fn analyze_bit_depth_enhanced(
    samples: &[f32],
    container_bits: u8,
) -> EnhancedBitDepthResult {
    let analyzer = DitherAnalyzer::default();
    let dither = analyzer.analyze(samples, container_bits);
    
    // Determine if upscaled based on multiple factors
    let (is_upscaled, confidence, reason) = determine_upscaling(
        &dither,
        container_bits,
    );
    
    EnhancedBitDepthResult {
        container_bits,
        effective_bits: dither.effective_bits,
        dither,
        is_upscaled,
        upscale_confidence: confidence,
        reason,
    }
}

fn determine_upscaling(
    dither: &DitherAnalysis,
    container_bits: u8,
) -> (bool, f32, String) {
    // Case 1: Clear bit depth mismatch with no dithering
    if dither.effective_bits < container_bits && dither.dither_type == DitherType::None {
        return (
            true,
            0.95,
            format!(
                "Effective bit depth ({}) < container ({}) with no dithering",
                dither.effective_bits, container_bits
            ),
        );
    }
    
    // Case 2: Bit depth mismatch with dithering detected
    if dither.effective_bits < container_bits && dither.dither_type != DitherType::None {
        let confidence = 0.8 * dither.confidence;
        return (
            true,
            confidence,
            format!(
                "Effective bit depth ({}) < container ({}) with {:?} dithering (confidence: {:.1}%)",
                dither.effective_bits, container_bits, dither.dither_type, confidence * 100.0
            ),
        );
    }
    
    // Case 3: LSBs mostly zero but some activity (possible bad upscaling)
    if dither.lsb_activity < 0.05 && container_bits == 24 {
        return (
            true,
            0.85,
            "Very low LSB activity in 24-bit container suggests 16-bit source".to_string(),
        );
    }
    
    // Case 4: Genuine bit depth
    (
        false,
        0.9,
        format!("Consistent {}-bit depth with natural quantization noise", container_bits),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rectangular_dither_detection() {
        // Generate samples with rectangular dither pattern
        let mut samples: Vec<f32> = Vec::new();
        let scale = (1 << 23) as f32; // 24-bit scale
        
        for i in 0..10000 {
            // Base 16-bit signal upscaled to 24-bit
            let base = ((i as f32 * 0.1).sin() * 32767.0) as i32;
            let upscaled = base << 8;
            
            // Add rectangular dither (±128 uniform)
            let dither = ((i * 1234567) % 256) as i32 - 128;
            let sample_int = upscaled + dither;
            
            samples.push(sample_int as f32 / scale);
        }
        
        let analyzer = DitherAnalyzer::default();
        let result = analyzer.analyze(&samples, 24);
        
        assert_eq!(result.effective_bits, 16);
        assert!(result.is_upscaled);
    }
    
    #[test]
    fn test_genuine_24bit_detection() {
        // Generate genuine 24-bit samples
        let mut samples: Vec<f32> = Vec::new();
        let scale = (1 << 23) as f32;
        
        for i in 0..10000 {
            // Full 24-bit signal
            let sample = ((i as f32 * 0.1).sin() * 8388607.0) as i32;
            samples.push(sample as f32 / scale);
        }
        
        let analyzer = DitherAnalyzer::default();
        let result = analyzer.analyze(&samples, 24);
        
        assert_eq!(result.effective_bits, 24);
        assert!(!result.is_upscaled);
    }
}
