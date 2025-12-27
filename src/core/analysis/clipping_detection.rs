//! Clipping detection analysis
//!
//! Detects digital clipping, inter-sample peaks, and related issues.

use crate::core::analysis::{Detection, DetectionMethod, DefectType, Severity, TemporalDistribution};

/// Clipping detection analyzer
pub struct ClippingDetector {
    /// Threshold for considering a sample as clipped (relative to max)
    clip_threshold: f64,
    /// Minimum consecutive clipped samples to report
    min_consecutive: usize,
    /// Enable inter-sample peak detection
    detect_intersample: bool,
}

impl Default for ClippingDetector {
    fn default() -> Self {
        Self {
            clip_threshold: 0.99,
            min_consecutive: 3,
            detect_intersample: true,
        }
    }
}

impl ClippingDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.clip_threshold = threshold;
        self
    }

    pub fn with_min_consecutive(mut self, min: usize) -> Self {
        self.min_consecutive = min;
        self
    }

    pub fn with_intersample_detection(mut self, enable: bool) -> Self {
        self.detect_intersample = enable;
        self
    }

    /// Analyze samples for clipping
    pub fn analyze(&self, samples: &[f32], sample_rate: u32) -> Option<Detection> {
        let mut clipped_regions: Vec<(usize, usize)> = Vec::new();
        let mut current_start: Option<usize> = None;
        let mut consecutive_count = 0;
        let mut max_peak: f64 = 0.0;
        let mut total_clipped: u64 = 0;

        for (i, &sample) in samples.iter().enumerate() {
            let abs_sample = sample.abs() as f64;
            max_peak = max_peak.max(abs_sample);

            if abs_sample >= self.clip_threshold {
                if current_start.is_none() {
                    current_start = Some(i);
                }
                consecutive_count += 1;
                total_clipped += 1;
            } else {
                if consecutive_count >= self.min_consecutive {
                    if let Some(start) = current_start {
                        clipped_regions.push((start, i));
                    }
                }
                current_start = None;
                consecutive_count = 0;
            }
        }

        // Handle trailing clipped region
        if consecutive_count >= self.min_consecutive {
            if let Some(start) = current_start {
                clipped_regions.push((start, samples.len()));
            }
        }

        // Also check for inter-sample peaks if enabled
        let intersample_clips = if self.detect_intersample {
            self.detect_intersample_peaks(samples)
        } else {
            0
        };

        total_clipped += intersample_clips as u64;

        if total_clipped == 0 {
            return None;
        }

        // Calculate peak level in dB
        let peak_db = if max_peak > 0.0 {
            20.0 * max_peak.log10()
        } else {
            -f64::INFINITY
        };

        // Calculate confidence based on severity
        let clip_ratio = total_clipped as f64 / samples.len() as f64;
        let confidence = (clip_ratio * 1000.0).min(1.0);

        // Determine severity
        let severity = if clip_ratio > 0.01 {
            Severity::Critical
        } else if clip_ratio > 0.001 {
            Severity::High
        } else if clip_ratio > 0.0001 {
            Severity::Medium
        } else {
            Severity::Low
        };

        // Build temporal distribution
        let temporal = if !clipped_regions.is_empty() {
            let duration = samples.len() as f64 / sample_rate as f64;
            let distribution = self.build_distribution(&clipped_regions, samples.len(), 100);
            
            let start_time = clipped_regions.first().unwrap().0 as f64 / sample_rate as f64;
            let end_time = clipped_regions.last().unwrap().1 as f64 / sample_rate as f64;
            let peak_idx = distribution
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);
            let peak_time = (peak_idx as f64 / 100.0) * duration;

            Some(TemporalDistribution {
                start_time,
                end_time,
                peak_time,
                distribution,
            })
        } else {
            None
        };

        Some(Detection {
            defect_type: DefectType::Clipping {
                peak_level: peak_db,
                clipped_samples: total_clipped,
            },
            confidence,
            severity,
            method: DetectionMethod::ClippingAnalysis,
            evidence: Some(format!(
                "{} clipped samples ({:.4}% of total), {} regions",
                total_clipped,
                clip_ratio * 100.0,
                clipped_regions.len()
            )),
            temporal,
        })
    }

    /// Detect inter-sample peaks using simple interpolation
    fn detect_intersample_peaks(&self, samples: &[f32]) -> usize {
        let mut count = 0;
        
        for window in samples.windows(3) {
            let (a, b, c) = (window[0] as f64, window[1] as f64, window[2] as f64);
            
            // Simple parabolic interpolation to find peak between samples
            let peak_offset = (a - c) / (2.0 * (a - 2.0 * b + c));
            
            if peak_offset.abs() < 1.0 && !peak_offset.is_nan() {
                let interpolated_peak = b - 0.25 * (a - c) * peak_offset;
                
                if interpolated_peak.abs() > 1.0 && b.abs() < self.clip_threshold {
                    count += 1;
                }
            }
        }
        
        count
    }

    /// Build temporal distribution histogram
    fn build_distribution(&self, regions: &[(usize, usize)], total_samples: usize, bins: usize) -> Vec<f64> {
        let mut distribution = vec![0.0; bins];
        let samples_per_bin = total_samples as f64 / bins as f64;

        for &(start, end) in regions {
            let start_bin = (start as f64 / samples_per_bin).floor() as usize;
            let end_bin = (end as f64 / samples_per_bin).ceil() as usize;

            for bin in start_bin..end_bin.min(bins) {
                distribution[bin] += 1.0;
            }
        }

        // Normalize
        let max_val = distribution.iter().cloned().fold(0.0_f64, f64::max);
        if max_val > 0.0 {
            for v in &mut distribution {
                *v /= max_val;
            }
        }

        distribution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_clipping() {
        let detector = ClippingDetector::new();
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 2000.0).sin() * 0.5).collect();
        
        let result = detector.analyze(&samples, 44100);
        assert!(result.is_none());
    }

    #[test]
    fn test_clipping_detection() {
        let detector = ClippingDetector::new();
        let mut samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 100.0).sin()).collect();
        
        // Add some clipped samples
        for i in 100..110 {
            samples[i] = 1.0;
        }
        
        let result = detector.analyze(&samples, 44100);
        assert!(result.is_some());
        
        let detection = result.unwrap();
        if let DefectType::Clipping { clipped_samples, .. } = detection.defect_type {
            assert!(clipped_samples >= 10);
        } else {
            panic!("Expected Clipping defect type");
        }
    }
}
