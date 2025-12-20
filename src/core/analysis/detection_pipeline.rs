// src/core/analysis/detection_pipeline.rs
//
// Improved Detection Pipeline for AudioCheckr
//
// This module implements a sample-rate-aware, ordered detection pipeline
// that correctly handles DSP artifacts (dithering, resampling) without
// false positives from lossy codec detectors.
//
// Detection Order (critical for accuracy):
// 1. Sample rate analysis (determines what detectors can apply)
// 2. Bit depth analysis (detect actual vs claimed)
// 3. Dithering detection (if bit depth mismatch)
// 4. Resampling detection (spectral null analysis)
// 5. Upsampling detection (content analysis)
// 6. Lossy codec detection (ONLY if sample rate allows)
//
// Key Insight: Files at >48kHz CANNOT be direct MP3/AAC transcodes
// because those codecs don't support sample rates above 48kHz.

use crate::core::analysis::dither_detection::{DitherDetector, DitherDetectionResult, DitherAlgorithm};
use crate::core::analysis::resample_detection::{ResampleDetector, ResampleDetectionResult, ResampleDirection};

/// Sample rate constraints for lossy codecs
pub struct CodecConstraints {
    /// Maximum supported sample rate for MP3
    pub mp3_max_sample_rate: u32,
    /// Maximum supported sample rate for AAC (common usage)
    pub aac_max_sample_rate: u32,
    /// Maximum supported sample rate for Vorbis
    pub vorbis_max_sample_rate: u32,
    /// Maximum supported sample rate for Opus
    pub opus_max_sample_rate: u32,
}

impl Default for CodecConstraints {
    fn default() -> Self {
        Self {
            mp3_max_sample_rate: 48000,      // MP3 max is 48kHz
            aac_max_sample_rate: 96000,      // AAC can do 96kHz but rare
            vorbis_max_sample_rate: 192000,  // Vorbis supports high rates
            opus_max_sample_rate: 48000,     // Opus internally processes at 48kHz
        }
    }
}

/// Detection context with sample rate awareness
#[derive(Debug, Clone)]
pub struct DetectionContext {
    /// Current sample rate of the file
    pub sample_rate: u32,
    /// Container-claimed bit depth
    pub container_bit_depth: u8,
    /// Actual detected bit depth
    pub actual_bit_depth: u8,
    /// Is MP3 detection applicable for this sample rate?
    pub mp3_detection_applicable: bool,
    /// Is AAC detection applicable for this sample rate?
    pub aac_detection_applicable: bool,
    /// Is Vorbis detection applicable for this sample rate?
    pub vorbis_detection_applicable: bool,
    /// Is Opus detection applicable for this sample rate?
    pub opus_detection_applicable: bool,
    /// Detected dithering (if any)
    pub dithering: Option<DitherDetectionResult>,
    /// Detected resampling (if any)
    pub resampling: Option<ResampleDetectionResult>,
    /// Should suppress lossy detection due to DSP artifacts?
    pub suppress_lossy_detection: bool,
    /// Reasons for detection decisions
    pub evidence: Vec<String>,
}

impl DetectionContext {
    pub fn new(sample_rate: u32, container_bit_depth: u8) -> Self {
        let constraints = CodecConstraints::default();
        
        let mp3_applicable = sample_rate <= constraints.mp3_max_sample_rate;
        let aac_applicable = sample_rate <= constraints.aac_max_sample_rate;
        let vorbis_applicable = sample_rate <= constraints.vorbis_max_sample_rate;
        let opus_applicable = sample_rate <= constraints.opus_max_sample_rate;
        
        let mut evidence = Vec::new();
        
        if !mp3_applicable {
            evidence.push(format!(
                "Sample rate {} Hz > MP3 max {} Hz: skipping MP3 detection",
                sample_rate, constraints.mp3_max_sample_rate
            ));
        }
        
        if !aac_applicable {
            evidence.push(format!(
                "Sample rate {} Hz > AAC max {} Hz: skipping AAC detection",
                sample_rate, constraints.aac_max_sample_rate
            ));
        }
        
        Self {
            sample_rate,
            container_bit_depth,
            actual_bit_depth: container_bit_depth,
            mp3_detection_applicable: mp3_applicable,
            aac_detection_applicable: aac_applicable,
            vorbis_detection_applicable: vorbis_applicable,
            opus_detection_applicable: opus_applicable,
            dithering: None,
            resampling: None,
            suppress_lossy_detection: false,
            evidence,
        }
    }
    
    /// Update context with dithering detection results
    pub fn set_dithering(&mut self, result: DitherDetectionResult) {
        if result.is_bit_reduced && result.algorithm != DitherAlgorithm::None {
            self.evidence.push(format!(
                "Dithering detected: {} ({}→{} bit)",
                result.algorithm, result.container_bit_depth, result.effective_bit_depth
            ));
            
            // If dithering is detected, we should be more cautious about
            // lossy codec detection - noise shaping can look like spectral artifacts
            if result.algorithm_confidence > 0.6 {
                self.evidence.push(
                    "High-confidence dithering: reducing lossy detection sensitivity".to_string()
                );
            }
        }
        
        self.actual_bit_depth = result.effective_bit_depth;
        self.dithering = Some(result);
    }
    
    /// Update context with resampling detection results
    pub fn set_resampling(&mut self, result: ResampleDetectionResult) {
        if result.is_resampled {
            self.evidence.push(format!(
                "Resampling detected: {} Hz → {} Hz ({})",
                result.original_sample_rate.unwrap_or(0),
                result.current_sample_rate,
                match result.direction {
                    ResampleDirection::Upsample => "upsampled",
                    ResampleDirection::Downsample => "downsampled",
                    ResampleDirection::None => "unknown direction",
                }
            ));
            
            // If high-quality resampling is detected, spectral rolloff is expected
            // and should not be confused with lossy codec artifacts
            if result.confidence > 0.6 {
                self.suppress_lossy_detection = true;
                self.evidence.push(
                    "Suppressing lossy detection: resampling filter rolloff expected".to_string()
                );
            }
        }
        
        self.resampling = Some(result);
    }
    
    /// Check if lossy codec detection should run
    pub fn should_run_lossy_detection(&self) -> bool {
        // Don't run if sample rate prevents it
        if !self.mp3_detection_applicable && 
           !self.aac_detection_applicable && 
           !self.vorbis_detection_applicable {
            return false;
        }
        
        // Don't run if DSP artifacts would cause false positives
        if self.suppress_lossy_detection {
            return false;
        }
        
        true
    }
    
    /// Get modified confidence for lossy detection based on context
    pub fn adjust_lossy_confidence(&self, raw_confidence: f32, codec: &str) -> f32 {
        let mut confidence = raw_confidence;
        
        // Reduce confidence if dithering was detected
        if let Some(ref dither) = self.dithering {
            if dither.is_bit_reduced && dither.algorithm_confidence > 0.5 {
                // Noise shaping can create spectral characteristics similar to lossy
                confidence *= 0.7;
            }
        }
        
        // Reduce confidence if resampling was detected
        if let Some(ref resample) = self.resampling {
            if resample.is_resampled && resample.confidence > 0.5 {
                // Anti-aliasing filters create rolloff similar to lossy
                confidence *= 0.6;
            }
        }
        
        // Apply codec-specific adjustments
        match codec {
            "mp3" => {
                if !self.mp3_detection_applicable {
                    confidence = 0.0;
                }
            }
            "aac" => {
                if !self.aac_detection_applicable {
                    confidence = 0.0;
                }
            }
            "vorbis" => {
                if !self.vorbis_detection_applicable {
                    confidence = 0.0;
                }
            }
            "opus" => {
                if !self.opus_detection_applicable {
                    confidence = 0.0;
                }
            }
            _ => {}
        }
        
        confidence
    }
}

/// Discrimination features between DSP artifacts and lossy codec artifacts
pub struct ArtifactDiscrimination {
    /// Spectral rolloff frequency in Hz
    pub rolloff_hz: f32,
    /// Rolloff steepness in dB/octave
    pub rolloff_steepness: f32,
    /// Is there a sharp (brick-wall) cutoff?
    pub has_brick_wall: bool,
    /// Frequency of brick-wall cutoff if present
    pub brick_wall_hz: Option<f32>,
    /// Does the rolloff match known lossy codec patterns?
    pub matches_lossy_pattern: bool,
    /// Does the rolloff match anti-aliasing filter patterns?
    pub matches_aa_filter: bool,
    /// Spectral energy above expected Nyquist (for upsampling detection)
    pub energy_above_half_nyquist: f32,
}

impl ArtifactDiscrimination {
    /// Analyze spectral characteristics to distinguish artifact types
    pub fn analyze(
        spectrum: &[f32],
        sample_rate: u32,
        bin_hz: f32,
    ) -> Self {
        let nyquist = sample_rate as f32 / 2.0;
        
        // Find rolloff characteristics
        let (rolloff_hz, rolloff_steepness) = find_rolloff(spectrum, bin_hz);
        
        // Check for brick-wall cutoff
        let (has_brick_wall, brick_wall_hz) = detect_brick_wall(spectrum, bin_hz, nyquist);
        
        // Distinguish lossy codec rolloff from anti-aliasing
        let matches_lossy = is_lossy_codec_rolloff(
            rolloff_hz, rolloff_steepness, sample_rate
        );
        
        let matches_aa = is_antialiasing_rolloff(
            rolloff_hz, rolloff_steepness, has_brick_wall, sample_rate
        );
        
        // Check energy distribution above half-Nyquist
        let half_nyquist_bin = (nyquist / 2.0 / bin_hz) as usize;
        let energy_above = if half_nyquist_bin < spectrum.len() {
            spectrum[half_nyquist_bin..].iter()
                .map(|&x| 10.0f32.powf(x / 20.0))
                .sum::<f32>() / (spectrum.len() - half_nyquist_bin) as f32
        } else {
            0.0
        };
        
        Self {
            rolloff_hz,
            rolloff_steepness,
            has_brick_wall,
            brick_wall_hz,
            matches_lossy_pattern: matches_lossy,
            matches_aa_filter: matches_aa,
            energy_above_half_nyquist: energy_above,
        }
    }
}

/// Find spectral rolloff point and steepness
fn find_rolloff(spectrum: &[f32], bin_hz: f32) -> (f32, f32) {
    if spectrum.is_empty() {
        return (0.0, 0.0);
    }
    
    // Find reference level (average of first quarter)
    let ref_end = spectrum.len() / 4;
    let ref_level: f32 = spectrum[1..ref_end].iter().sum::<f32>() / (ref_end - 1) as f32;
    
    // Find -3dB point
    let threshold = ref_level - 3.0;
    let mut rolloff_bin = spectrum.len() - 1;
    
    for i in (spectrum.len() / 2..spectrum.len()).rev() {
        if spectrum[i] > threshold {
            rolloff_bin = i;
            break;
        }
    }
    
    let rolloff_hz = (rolloff_bin as f32 + 0.5) * bin_hz;
    
    // Calculate steepness (dB/octave) from -3dB to -20dB point
    let steep_threshold = ref_level - 20.0;
    let mut steep_bin = spectrum.len() - 1;
    
    for i in rolloff_bin..spectrum.len() {
        if spectrum[i] < steep_threshold {
            steep_bin = i;
            break;
        }
    }
    
    let octaves = ((steep_bin as f32 * bin_hz) / (rolloff_bin as f32 * bin_hz).max(1.0)).log2();
    let steepness = if octaves > 0.01 { 17.0 / octaves } else { 0.0 }; // 17dB drop over octaves
    
    (rolloff_hz, steepness)
}

/// Detect brick-wall (sharp) cutoff
fn detect_brick_wall(spectrum: &[f32], bin_hz: f32, nyquist: f32) -> (bool, Option<f32>) {
    // Look for sudden drop > 40dB within a narrow frequency band
    let window = 10; // ~10 bins window
    
    for i in (spectrum.len() / 2)..(spectrum.len() - window) {
        let before = spectrum[i];
        let after = spectrum[i + window];
        
        // Sharp drop of > 40dB in 10 bins indicates brick-wall
        if before - after > 40.0 && before > -60.0 {
            let cutoff_hz = (i as f32 + window as f32 / 2.0) * bin_hz;
            if cutoff_hz < nyquist * 0.98 { // Must be below Nyquist
                return (true, Some(cutoff_hz));
            }
        }
    }
    
    (false, None)
}

/// Check if rolloff matches lossy codec patterns
fn is_lossy_codec_rolloff(rolloff_hz: f32, steepness: f32, sample_rate: u32) -> bool {
    // MP3 characteristics at various bitrates:
    // - 128kbps: ~16kHz cutoff, moderate steepness
    // - 192kbps: ~18kHz cutoff
    // - 256kbps: ~19kHz cutoff
    // - 320kbps: ~20kHz cutoff (sometimes transparent)
    
    // Only consider if sample rate allows MP3
    if sample_rate > 48000 {
        return false;
    }
    
    // MP3 typically has cutoff between 15-20kHz with moderate steepness
    let mp3_range = rolloff_hz >= 14000.0 && rolloff_hz <= 20500.0;
    let mp3_steepness = steepness >= 50.0 && steepness <= 200.0;
    
    mp3_range && mp3_steepness
}

/// Check if rolloff matches anti-aliasing filter patterns
fn is_antialiasing_rolloff(
    rolloff_hz: f32,
    steepness: f32,
    has_brick_wall: bool,
    sample_rate: u32,
) -> bool {
    let nyquist = sample_rate as f32 / 2.0;
    
    // Anti-aliasing filters typically:
    // - Cut off near (but slightly below) the original Nyquist
    // - Have very steep rolloff (often > 200 dB/octave)
    // - May have brick-wall characteristics
    
    // Common original sample rates for downsampling
    let common_originals = [176400, 192000, 96000, 88200];
    
    for &orig_rate in &common_originals {
        if orig_rate as f32 > sample_rate as f32 {
            let orig_nyquist = orig_rate as f32 / 2.0;
            // Check if rolloff is near original Nyquist
            let near_orig_nyquist = (rolloff_hz - orig_nyquist).abs() < 2000.0;
            
            if near_orig_nyquist {
                return true;
            }
        }
    }
    
    // High sample rate files with rolloff near Nyquist likely have AA filter
    if sample_rate >= 88200 && rolloff_hz > nyquist * 0.85 {
        return true;
    }
    
    // Very steep rolloff (> 200 dB/oct) suggests digital filter, not lossy codec
    if steepness > 200.0 {
        return true;
    }
    
    // Brick-wall cutoff suggests resampling, not lossy compression
    if has_brick_wall {
        return true;
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_context_sample_rate_constraints() {
        // 44.1kHz - all codecs applicable
        let ctx = DetectionContext::new(44100, 24);
        assert!(ctx.mp3_detection_applicable);
        assert!(ctx.aac_detection_applicable);
        
        // 96kHz - MP3 not applicable
        let ctx = DetectionContext::new(96000, 24);
        assert!(!ctx.mp3_detection_applicable);
        assert!(ctx.aac_detection_applicable);
        
        // 176.4kHz - only Vorbis applicable
        let ctx = DetectionContext::new(176400, 24);
        assert!(!ctx.mp3_detection_applicable);
        assert!(!ctx.aac_detection_applicable);
        assert!(ctx.vorbis_detection_applicable);
    }
    
    #[test]
    fn test_lossy_confidence_adjustment() {
        let ctx = DetectionContext::new(176400, 24);
        
        // MP3 confidence should be zeroed at high sample rate
        let adjusted = ctx.adjust_lossy_confidence(0.9, "mp3");
        assert_eq!(adjusted, 0.0);
        
        // Vorbis should pass through
        let adjusted = ctx.adjust_lossy_confidence(0.9, "vorbis");
        assert!(adjusted > 0.0);
    }
}
