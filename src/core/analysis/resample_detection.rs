// src/core/analysis/resample_detection.rs
//
// Resampling detection module for AudioCheckr.
// Detects various resampling algorithms and quality settings:
//
// SWR (FFmpeg libswresample):
// - Default (linear interpolation)
// - Cubic interpolation
// - Blackman-Nuttall window
// - Kaiser window (beta 9, 12, 16)
// - Various filter sizes (16, 64, etc.)
//
// SoXR (high-quality resampler):
// - Default quality
// - HQ (precision=20)
// - VHQ (precision=28)
// - VHQ with Chebyshev passband (cheby=1)
// - Various cutoff frequencies (0.91, 0.95, etc.)
//
// Also detects original sample rate and upsampling vs downsampling

use rustfft::{FftPlanner, num_complex::Complex};
use std::f64::consts::PI;

/// Resampling engine type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResamplerEngine {
    /// FFmpeg's built-in libswresample
    SwrDefault,
    /// libswresample with cubic interpolation
    SwrCubic,
    /// libswresample with Blackman-Nuttall window
    SwrBlackmanNuttall,
    /// libswresample with Kaiser window
    SwrKaiser { beta: u8 },
    /// libswresample with custom filter size
    SwrFilterSize { size: u16 },
    /// SoXR default quality
    SoxrDefault,
    /// SoXR high quality (precision=20)
    SoxrHQ,
    /// SoXR very high quality (precision=28)
    SoxrVHQ,
    /// SoXR VHQ with Chebyshev passband
    SoxrVHQCheby,
    /// SoXR with custom cutoff
    SoxrCutoff { cutoff: f32 },
    /// Unknown or undetectable resampler
    Unknown,
}

impl std::fmt::Display for ResamplerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResamplerEngine::SwrDefault => write!(f, "SWR Default"),
            ResamplerEngine::SwrCubic => write!(f, "SWR Cubic"),
            ResamplerEngine::SwrBlackmanNuttall => write!(f, "SWR Blackman-Nuttall"),
            ResamplerEngine::SwrKaiser { beta } => write!(f, "SWR Kaiser (β={})", beta),
            ResamplerEngine::SwrFilterSize { size } => write!(f, "SWR Filter Size {}", size),
            ResamplerEngine::SoxrDefault => write!(f, "SoXR Default"),
            ResamplerEngine::SoxrHQ => write!(f, "SoXR HQ"),
            ResamplerEngine::SoxrVHQ => write!(f, "SoXR VHQ"),
            ResamplerEngine::SoxrVHQCheby => write!(f, "SoXR VHQ Chebyshev"),
            ResamplerEngine::SoxrCutoff { cutoff } => write!(f, "SoXR Cutoff {:.0}%", cutoff * 100.0),
            ResamplerEngine::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Quality tier of resampling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResampleQuality {
    /// Low quality (simple linear, small filters)
    Low,
    /// Standard quality (default settings)
    Standard,
    /// High quality (larger filters, better windowing)
    High,
    /// Very high quality (SoXR VHQ, large Kaiser beta)
    VeryHigh,
    /// Transparent quality (indistinguishable from original)
    Transparent,
}

impl std::fmt::Display for ResampleQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResampleQuality::Low => write!(f, "Low"),
            ResampleQuality::Standard => write!(f, "Standard"),
            ResampleQuality::High => write!(f, "High"),
            ResampleQuality::VeryHigh => write!(f, "Very High"),
            ResampleQuality::Transparent => write!(f, "Transparent"),
        }
    }
}

/// Direction of sample rate conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleDirection {
    /// Upsampling (increasing sample rate)
    Upsample,
    /// Downsampling (decreasing sample rate)
    Downsample,
    /// No resampling detected
    None,
}

/// Complete resampling detection result
#[derive(Debug, Clone)]
pub struct ResampleDetectionResult {
    /// Whether resampling was detected
    pub is_resampled: bool,
    /// Confidence in detection (0.0-1.0)
    pub confidence: f32,
    /// Current sample rate
    pub current_sample_rate: u32,
    /// Estimated original sample rate (before resampling)
    pub original_sample_rate: Option<u32>,
    /// Resampling direction
    pub direction: ResampleDirection,
    /// Detected resampler engine
    pub engine: ResamplerEngine,
    /// Confidence in engine detection (0.0-1.0)
    pub engine_confidence: f32,
    /// Estimated quality tier
    pub quality: ResampleQuality,
    /// Anti-aliasing filter cutoff ratio (0.0-1.0 of Nyquist)
    pub filter_cutoff_ratio: f32,
    /// Filter transition band width in Hz
    pub transition_band_hz: f32,
    /// Stopband attenuation in dB
    pub stopband_attenuation_db: f32,
    /// Passband ripple in dB
    pub passband_ripple_db: f32,
    /// Whether spectral null at original Nyquist was found
    pub has_nyquist_null: bool,
    /// Frequency of detected spectral null (Hz)
    pub null_frequency_hz: Option<f32>,
    /// Detailed evidence
    pub evidence: Vec<String>,
}

impl Default for ResampleDetectionResult {
    fn default() -> Self {
        Self {
            is_resampled: false,
            confidence: 0.0,
            current_sample_rate: 44100,
            original_sample_rate: None,
            direction: ResampleDirection::None,
            engine: ResamplerEngine::Unknown,
            engine_confidence: 0.0,
            quality: ResampleQuality::Standard,
            filter_cutoff_ratio: 1.0,
            transition_band_hz: 0.0,
            stopband_attenuation_db: 0.0,
            passband_ripple_db: 0.0,
            has_nyquist_null: false,
            null_frequency_hz: None,
            evidence: Vec::new(),
        }
    }
}

/// Main resampling detector
pub struct ResampleDetector {
    fft_size: usize,
    num_frames: usize,
}

impl Default for ResampleDetector {
    fn default() -> Self {
        Self {
            fft_size: 16384,  // High resolution for null detection
            num_frames: 100,
        }
    }
}

impl ResampleDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_fft_size(mut self, size: usize) -> Self {
        self.fft_size = size;
        self
    }
    
    /// Analyze audio for resampling artifacts
    pub fn analyze(&self, samples: &[f32], sample_rate: u32) -> ResampleDetectionResult {
        let mut result = ResampleDetectionResult {
            current_sample_rate: sample_rate,
            ..Default::default()
        };
        
        if samples.len() < self.fft_size * 2 {
            result.evidence.push("Insufficient samples for resampling analysis".to_string());
            return result;
        }
        
        // Step 1: Compute high-resolution spectrum
        let spectrum = self.compute_averaged_spectrum(samples, sample_rate);
        
        // Step 2: Look for spectral nulls indicating original Nyquist
        let null_detection = self.detect_spectral_null(&spectrum, sample_rate);
        
        if let Some(null) = &null_detection {
            result.has_nyquist_null = true;
            result.null_frequency_hz = Some(null.frequency_hz);
            result.original_sample_rate = Some(null.implied_sample_rate);
            result.is_resampled = true;
            
            // Determine direction
            if null.implied_sample_rate < sample_rate {
                result.direction = ResampleDirection::Upsample;
                result.evidence.push(format!(
                    "Spectral null at {:.0} Hz suggests upsampling from {} Hz",
                    null.frequency_hz, null.implied_sample_rate
                ));
            } else {
                result.direction = ResampleDirection::Downsample;
                result.evidence.push(format!(
                    "Spectral characteristics suggest downsampling from {} Hz",
                    null.implied_sample_rate
                ));
            }
            
            result.confidence = null.confidence;
        }
        
        // Step 3: Analyze filter characteristics
        let filter_analysis = self.analyze_filter_characteristics(&spectrum, sample_rate);
        result.filter_cutoff_ratio = filter_analysis.cutoff_ratio;
        result.transition_band_hz = filter_analysis.transition_band_hz;
        result.stopband_attenuation_db = filter_analysis.stopband_attenuation_db;
        result.passband_ripple_db = filter_analysis.passband_ripple_db;
        
        result.evidence.push(format!(
            "Anti-aliasing filter cutoff: {:.1}% of Nyquist, transition band: {:.0} Hz",
            filter_analysis.cutoff_ratio * 100.0,
            filter_analysis.transition_band_hz
        ));
        
        // Step 4: If resampled, classify the resampler
        if result.is_resampled {
            let (engine, engine_conf, quality) = self.classify_resampler(&filter_analysis, &null_detection);
            result.engine = engine;
            result.engine_confidence = engine_conf;
            result.quality = quality;
            
            result.evidence.push(format!(
                "Resampler engine: {} ({:.0}% confidence), Quality: {}",
                result.engine, engine_conf * 100.0, result.quality
            ));
        }
        
        // Step 5: Check for upsampling without null (44.1 → 48k same family)
        if !result.is_resampled {
            let potential_upsampling = self.detect_potential_upsampling(&spectrum, sample_rate);
            if potential_upsampling.is_some() {
                let up = potential_upsampling.unwrap();
                result.is_resampled = true;
                result.original_sample_rate = Some(up.original_rate);
                result.direction = ResampleDirection::Upsample;
                result.confidence = up.confidence;
                result.evidence.push(format!(
                    "Possible upsampling from {} Hz detected (low confidence)",
                    up.original_rate
                ));
            }
        }
        
        result
    }
    
    /// Compute averaged magnitude spectrum
    fn compute_averaged_spectrum(&self, samples: &[f32], sample_rate: u32) -> Vec<f32> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        let hop_size = self.fft_size / 2;
        let num_frames = ((samples.len() - self.fft_size) / hop_size + 1).min(self.num_frames);
        
        let window: Vec<f32> = (0..self.fft_size)
            .map(|i| {
                // Blackman-Harris window for high dynamic range
                let x = i as f32 / self.fft_size as f32;
                0.35875 - 0.48829 * (2.0 * PI as f32 * x).cos()
                    + 0.14128 * (4.0 * PI as f32 * x).cos()
                    - 0.01168 * (6.0 * PI as f32 * x).cos()
            })
            .collect();
        
        let mut spectrum_accum = vec![0.0f64; self.fft_size / 2];
        let mut buffer = vec![Complex::new(0.0f32, 0.0); self.fft_size];
        
        for frame in 0..num_frames {
            let start = frame * hop_size;
            if start + self.fft_size > samples.len() {
                break;
            }
            
            for i in 0..self.fft_size {
                buffer[i] = Complex::new(samples[start + i] * window[i], 0.0);
            }
            
            fft.process(&mut buffer);
            
            for (i, c) in buffer.iter().take(self.fft_size / 2).enumerate() {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                spectrum_accum[i] += mag as f64;
            }
        }
        
        // Average and convert to dB
        spectrum_accum.iter()
            .map(|&s| {
                let avg = s / num_frames as f64;
                if avg > 1e-12 { (20.0 * avg.log10()) as f32 } else { -120.0 }
            })
            .collect()
    }
    
    /// Detect spectral null indicating original Nyquist frequency
    fn detect_spectral_null(&self, spectrum: &[f32], sample_rate: u32) -> Option<SpectralNull> {
        let nyquist = sample_rate as f32 / 2.0;
        let bin_hz = nyquist / spectrum.len() as f32;
        
        // Common original sample rates to check
        let candidate_rates: Vec<u32> = vec![
            8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000
        ];
        
        let mut best_null: Option<SpectralNull> = None;
        
        for &orig_rate in &candidate_rates {
            // Only check rates below current (for upsampling detection)
            if orig_rate >= sample_rate {
                continue;
            }
            
            let orig_nyquist = orig_rate as f32 / 2.0;
            
            // Skip if original Nyquist would be above current Nyquist
            if orig_nyquist >= nyquist {
                continue;
            }
            
            // Find the bin closest to original Nyquist
            let null_bin = (orig_nyquist / bin_hz) as usize;
            if null_bin >= spectrum.len() - 10 {
                continue;
            }
            
            // Analyze the null region
            let null_analysis = self.analyze_null_region(spectrum, null_bin, bin_hz);
            
            if null_analysis.is_null && null_analysis.confidence > 0.5 {
                let null_freq = (null_bin as f32 + 0.5) * bin_hz;
                
                if best_null.is_none() || null_analysis.confidence > best_null.as_ref().unwrap().confidence {
                    best_null = Some(SpectralNull {
                        frequency_hz: null_freq,
                        implied_sample_rate: orig_rate,
                        depth_db: null_analysis.depth_db,
                        confidence: null_analysis.confidence,
                    });
                }
            }
        }
        
        best_null
    }
    
    /// Analyze a region around a potential spectral null
    fn analyze_null_region(&self, spectrum: &[f32], center_bin: usize, bin_hz: f32) -> NullAnalysis {
        // Look at a region around the potential null
        let region_size = (500.0 / bin_hz) as usize; // ±500 Hz region
        let start = center_bin.saturating_sub(region_size);
        let end = (center_bin + region_size).min(spectrum.len());
        
        if end <= start + 10 {
            return NullAnalysis::default();
        }
        
        // Find average level before and after the null
        let before_region = &spectrum[start..center_bin.saturating_sub(5)];
        let after_start = (center_bin + 5).min(end);
        let after_region = if after_start < end { &spectrum[after_start..end] } else { &[] as &[f32] };
        
        if before_region.is_empty() {
            return NullAnalysis::default();
        }
        
        let before_avg: f32 = before_region.iter().sum::<f32>() / before_region.len() as f32;
        let after_avg = if !after_region.is_empty() {
            after_region.iter().sum::<f32>() / after_region.len() as f32
        } else {
            -60.0
        };
        
        // Find minimum in the null region
        let null_region = &spectrum[center_bin.saturating_sub(3)..(center_bin + 3).min(spectrum.len())];
        let null_min = null_region.iter().cloned().fold(f32::MAX, f32::min);
        
        // A null should be significantly below surrounding content
        let depth_db = before_avg - null_min;
        
        // Strong null characteristics:
        // 1. Significant depth (> 20 dB)
        // 2. Content above the null (upsampled content)
        // 3. Sharp transition
        
        let is_null = depth_db > 15.0 && after_avg < before_avg - 10.0;
        
        let confidence = if is_null {
            let depth_factor = (depth_db / 40.0).min(1.0);
            let transition_factor = ((before_avg - after_avg) / 30.0).min(1.0);
            depth_factor * 0.6 + transition_factor * 0.4
        } else {
            0.0
        };
        
        NullAnalysis {
            is_null,
            depth_db,
            confidence,
        }
    }
    
    /// Analyze anti-aliasing filter characteristics
    fn analyze_filter_characteristics(&self, spectrum: &[f32], sample_rate: u32) -> FilterAnalysis {
        let nyquist = sample_rate as f32 / 2.0;
        let bin_hz = nyquist / spectrum.len() as f32;
        
        // Find passband level (average from 1-5 kHz, typical music content)
        let pb_start = (1000.0 / bin_hz) as usize;
        let pb_end = (5000.0 / bin_hz) as usize;
        let passband_level: f32 = if pb_end > pb_start {
            spectrum[pb_start..pb_end].iter().sum::<f32>() / (pb_end - pb_start) as f32
        } else {
            -20.0
        };
        
        // Find stopband level (average near Nyquist)
        let sb_start = (spectrum.len() * 95 / 100).max(1);
        let stopband_level: f32 = spectrum[sb_start..].iter().sum::<f32>() 
            / (spectrum.len() - sb_start) as f32;
        
        // Find cutoff frequency (-3 dB point)
        let cutoff_threshold = passband_level - 3.0;
        let mut cutoff_bin = spectrum.len() - 1;
        
        // Search from high frequencies downward
        for i in (spectrum.len() / 2..spectrum.len()).rev() {
            if spectrum[i] > cutoff_threshold {
                cutoff_bin = i;
                break;
            }
        }
        
        let cutoff_hz = (cutoff_bin as f32 + 0.5) * bin_hz;
        let cutoff_ratio = cutoff_hz / nyquist;
        
        // Find transition band width (-3 dB to -60 dB)
        let stop_threshold = passband_level - 60.0;
        let mut stop_bin = spectrum.len() - 1;
        
        for i in cutoff_bin..spectrum.len() {
            if spectrum[i] < stop_threshold {
                stop_bin = i;
                break;
            }
        }
        
        let transition_band_hz = (stop_bin - cutoff_bin) as f32 * bin_hz;
        
        // Measure passband ripple
        let passband_max = spectrum[pb_start..pb_end].iter().cloned().fold(f32::MIN, f32::max);
        let passband_min = spectrum[pb_start..pb_end].iter().cloned().fold(f32::MAX, f32::min);
        let passband_ripple_db = passband_max - passband_min;
        
        FilterAnalysis {
            cutoff_ratio,
            transition_band_hz,
            stopband_attenuation_db: passband_level - stopband_level,
            passband_ripple_db,
            cutoff_hz,
        }
    }
    
    /// Classify the resampler based on filter characteristics
    fn classify_resampler(
        &self,
        filter: &FilterAnalysis,
        null: &Option<SpectralNull>,
    ) -> (ResamplerEngine, f32, ResampleQuality) {
        let mut best_engine = ResamplerEngine::Unknown;
        let mut best_confidence = 0.0f32;
        
        // Classify based on filter characteristics
        
        // SWR Default: moderate filter, ~85% cutoff, >60 dB stopband
        if filter.cutoff_ratio > 0.80 && filter.cutoff_ratio < 0.90 
           && filter.stopband_attenuation_db > 50.0 && filter.stopband_attenuation_db < 80.0 {
            best_engine = ResamplerEngine::SwrDefault;
            best_confidence = 0.6;
        }
        
        // SWR Cubic: slightly lower cutoff, some aliasing
        if filter.cutoff_ratio > 0.78 && filter.cutoff_ratio < 0.88
           && filter.stopband_attenuation_db < 60.0 {
            if best_confidence < 0.55 {
                best_engine = ResamplerEngine::SwrCubic;
                best_confidence = 0.55;
            }
        }
        
        // SWR Blackman-Nuttall: high stopband attenuation, sharp cutoff
        if filter.stopband_attenuation_db > 80.0 && filter.transition_band_hz < 2000.0 {
            if best_confidence < 0.7 {
                best_engine = ResamplerEngine::SwrBlackmanNuttall;
                best_confidence = 0.7;
            }
        }
        
        // SWR Kaiser: depends on beta
        if filter.stopband_attenuation_db > 70.0 && filter.stopband_attenuation_db < 100.0 {
            // Beta 9: ~60 dB stopband
            if filter.stopband_attenuation_db > 55.0 && filter.stopband_attenuation_db < 70.0 {
                if best_confidence < 0.5 {
                    best_engine = ResamplerEngine::SwrKaiser { beta: 9 };
                    best_confidence = 0.5;
                }
            }
            // Beta 12: ~80 dB stopband  
            if filter.stopband_attenuation_db > 70.0 && filter.stopband_attenuation_db < 90.0 {
                if best_confidence < 0.6 {
                    best_engine = ResamplerEngine::SwrKaiser { beta: 12 };
                    best_confidence = 0.6;
                }
            }
            // Beta 16: ~100+ dB stopband
            if filter.stopband_attenuation_db > 90.0 {
                if best_confidence < 0.65 {
                    best_engine = ResamplerEngine::SwrKaiser { beta: 16 };
                    best_confidence = 0.65;
                }
            }
        }
        
        // SoXR: very high quality characteristics
        if filter.stopband_attenuation_db > 100.0 {
            // Check for SoXR cutoff variants
            if filter.cutoff_ratio > 0.89 && filter.cutoff_ratio < 0.93 {
                best_engine = ResamplerEngine::SoxrCutoff { cutoff: 0.91 };
                best_confidence = 0.7;
            } else if filter.cutoff_ratio > 0.93 && filter.cutoff_ratio < 0.97 {
                best_engine = ResamplerEngine::SoxrCutoff { cutoff: 0.95 };
                best_confidence = 0.7;
            } else if filter.passband_ripple_db < 0.1 {
                // Very low ripple suggests Chebyshev passband
                best_engine = ResamplerEngine::SoxrVHQCheby;
                best_confidence = 0.75;
            } else if filter.stopband_attenuation_db > 130.0 {
                best_engine = ResamplerEngine::SoxrVHQ;
                best_confidence = 0.7;
            } else if filter.stopband_attenuation_db > 110.0 {
                best_engine = ResamplerEngine::SoxrHQ;
                best_confidence = 0.65;
            } else {
                best_engine = ResamplerEngine::SoxrDefault;
                best_confidence = 0.6;
            }
        }
        
        // Determine quality tier
        let quality = if filter.stopband_attenuation_db > 130.0 && filter.passband_ripple_db < 0.1 {
            ResampleQuality::Transparent
        } else if filter.stopband_attenuation_db > 100.0 {
            ResampleQuality::VeryHigh
        } else if filter.stopband_attenuation_db > 70.0 {
            ResampleQuality::High
        } else if filter.stopband_attenuation_db > 50.0 {
            ResampleQuality::Standard
        } else {
            ResampleQuality::Low
        };
        
        (best_engine, best_confidence, quality)
    }
    
    /// Detect potential upsampling without obvious spectral null
    fn detect_potential_upsampling(&self, spectrum: &[f32], sample_rate: u32) -> Option<PotentialUpsampling> {
        let nyquist = sample_rate as f32 / 2.0;
        let bin_hz = nyquist / spectrum.len() as f32;
        
        // Look for energy roll-off patterns suggesting upsampling from common rates
        let candidates = vec![
            (44100, 88200),   // 44.1k → 88.2k
            (44100, 96000),   // 44.1k → 96k
            (44100, 176400),  // 44.1k → 176.4k
            (44100, 192000),  // 44.1k → 192k
            (48000, 96000),   // 48k → 96k
            (48000, 192000),  // 48k → 192k
            (88200, 176400),  // 88.2k → 176.4k
            (96000, 192000),  // 96k → 192k
        ];
        
        for (orig, target) in candidates {
            if target != sample_rate {
                continue;
            }
            
            let orig_nyquist = orig as f32 / 2.0;
            let check_bin = (orig_nyquist * 0.95 / bin_hz) as usize;
            
            if check_bin >= spectrum.len() {
                continue;
            }
            
            // Check if energy drops off around original Nyquist
            let before_avg: f32 = if check_bin > 50 {
                spectrum[check_bin - 50..check_bin - 10].iter().sum::<f32>() / 40.0
            } else {
                continue;
            };
            
            let after_avg: f32 = if check_bin + 50 < spectrum.len() {
                spectrum[check_bin + 10..check_bin + 50].iter().sum::<f32>() / 40.0
            } else {
                spectrum[check_bin + 10..].iter().sum::<f32>() 
                    / (spectrum.len() - check_bin - 10).max(1) as f32
            };
            
            // Significant drop suggests upsampling
            let drop = before_avg - after_avg;
            if drop > 15.0 {
                return Some(PotentialUpsampling {
                    original_rate: orig,
                    confidence: (drop / 40.0).min(0.6),
                });
            }
        }
        
        None
    }
}

/// Detected spectral null information
#[derive(Debug, Clone)]
struct SpectralNull {
    frequency_hz: f32,
    implied_sample_rate: u32,
    depth_db: f32,
    confidence: f32,
}

/// Null region analysis
#[derive(Debug, Clone, Default)]
struct NullAnalysis {
    is_null: bool,
    depth_db: f32,
    confidence: f32,
}

/// Filter characteristic analysis
#[derive(Debug, Clone)]
struct FilterAnalysis {
    cutoff_ratio: f32,
    transition_band_hz: f32,
    stopband_attenuation_db: f32,
    passband_ripple_db: f32,
    cutoff_hz: f32,
}

/// Potential upsampling detection
#[derive(Debug, Clone)]
struct PotentialUpsampling {
    original_rate: u32,
    confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resampler_display() {
        assert_eq!(format!("{}", ResamplerEngine::SwrDefault), "SWR Default");
        assert_eq!(format!("{}", ResamplerEngine::SoxrVHQ), "SoXR VHQ");
        assert_eq!(format!("{}", ResamplerEngine::SwrKaiser { beta: 12 }), "SWR Kaiser (β=12)");
    }
    
    #[test]
    fn test_quality_ordering() {
        assert!(ResampleQuality::Low < ResampleQuality::Standard);
        assert!(ResampleQuality::Standard < ResampleQuality::High);
        assert!(ResampleQuality::VeryHigh < ResampleQuality::Transparent);
    }
}
