//! Detection Pipeline Extensions for ENF and Clipping Detection
//!
//! This module provides integration of ENF (Electrical Network Frequency) analysis
//! and comprehensive clipping detection into the AudioCheckr detection pipeline.
//!
//! Add this to your existing detection_pipeline.rs or use as a separate module.

use crate::core::analysis::enf_detection::{
    EnfDetector, EnfDetectionResult, EnfBaseFrequency, EnfRegion, EnfAnomalyType,
};
use crate::core::analysis::clipping_detection::{
    ClippingDetector, ClippingAnalysisResult, ClippingType,
    // NOTE: ClippingSeverity was removed - it doesn't exist in the actual module
    // Use ClippingCause, LikelyCause, TemporalDistribution instead
    TemporalDistribution,
};

/// Extended detection options including ENF and clipping analysis
#[derive(Debug, Clone)]
pub struct ExtendedDetectionOptions {
    /// Enable ENF (Electrical Network Frequency) analysis
    pub enable_enf: bool,
    /// Use sensitive ENF detection for noisy recordings
    pub enf_sensitive_mode: bool,
    /// Expected ENF frequency (None = auto-detect)
    pub expected_enf_frequency: Option<EnfBaseFrequency>,
    
    /// Enable clipping detection
    pub enable_clipping: bool,
    /// Use strict clipping thresholds (broadcast standards)
    pub clipping_strict_mode: bool,
    /// Enable inter-sample peak analysis (computationally intensive)
    pub enable_inter_sample_peaks: bool,
    /// Enable loudness war detection
    pub enable_loudness_analysis: bool,
}

impl Default for ExtendedDetectionOptions {
    fn default() -> Self {
        Self {
            enable_enf: false,  // Off by default (specialized use case)
            enf_sensitive_mode: false,
            expected_enf_frequency: None,
            enable_clipping: true,  // On by default (common issue)
            clipping_strict_mode: false,
            enable_inter_sample_peaks: true,
            enable_loudness_analysis: true,
        }
    }
}

/// Combined results from ENF and clipping analysis
#[derive(Debug, Clone)]
pub struct ExtendedAnalysisResult {
    /// ENF detection results (if enabled)
    pub enf_result: Option<EnfDetectionResult>,
    /// Clipping analysis results (if enabled)
    pub clipping_result: Option<ClippingAnalysisResult>,
    /// Combined quality assessment
    pub quality_assessment: QualityAssessment,
    /// Authenticity assessment (based on ENF)
    pub authenticity_assessment: Option<AuthenticityAssessment>,
}

/// Overall quality assessment based on clipping and loudness analysis
#[derive(Debug, Clone)]
pub struct QualityAssessment {
    /// Overall quality score (0.0 = poor, 1.0 = excellent)
    pub score: f32,
    /// Quality grade
    pub grade: QualityGrade,
    /// Issues detected
    pub issues: Vec<QualityIssue>,
    /// Recommendations for improvement
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityGrade {
    /// Excellent quality, no issues
    Excellent,
    /// Good quality, minor issues
    Good,
    /// Acceptable quality, some issues
    Acceptable,
    /// Poor quality, significant issues
    Poor,
    /// Severely degraded, major issues
    Severe,
}

impl std::fmt::Display for QualityGrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualityGrade::Excellent => write!(f, "Excellent"),
            QualityGrade::Good => write!(f, "Good"),
            QualityGrade::Acceptable => write!(f, "Acceptable"),
            QualityGrade::Poor => write!(f, "Poor"),
            QualityGrade::Severe => write!(f, "Severe"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityIssue {
    pub issue_type: QualityIssueType,
    pub severity: f32,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityIssueType {
    DigitalClipping,
    InterSamplePeaks,
    LoudnessWarVictim,
    LowDynamicRange,
    HighCompressionSeverity,
    SoftClipping,
    LimiterArtifacts,
}

/// Authenticity assessment based on ENF analysis
#[derive(Debug, Clone)]
pub struct AuthenticityAssessment {
    /// Confidence in authenticity (0.0 = definitely edited, 1.0 = definitely authentic)
    pub confidence: f32,
    /// Assessment result
    pub result: AuthenticityResult,
    /// Detected anomalies that may indicate editing
    pub anomalies: Vec<AuthenticityAnomaly>,
    /// Estimated recording region (based on ENF frequency)
    pub estimated_region: Option<EnfRegion>,
    /// Evidence supporting the assessment
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticityResult {
    /// Recording appears authentic with high confidence
    Authentic,
    /// Recording appears authentic but with some uncertainty
    LikelyAuthentic,
    /// Cannot determine authenticity (weak/no ENF signal)
    Inconclusive,
    /// Recording shows signs of editing
    PotentiallyEdited,
    /// Recording appears synthetic or digitally generated
    LikelySynthetic,
}

impl std::fmt::Display for AuthenticityResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthenticityResult::Authentic => write!(f, "Authentic"),
            AuthenticityResult::LikelyAuthentic => write!(f, "Likely Authentic"),
            AuthenticityResult::Inconclusive => write!(f, "Inconclusive"),
            AuthenticityResult::PotentiallyEdited => write!(f, "Potentially Edited"),
            AuthenticityResult::LikelySynthetic => write!(f, "Likely Synthetic"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthenticityAnomaly {
    pub timestamp_secs: f32,
    pub anomaly_type: String,
    pub severity: f32,
    pub description: String,
}

/// Extended detection pipeline with ENF and clipping analysis
pub struct ExtendedDetectionPipeline {
    enf_detector: EnfDetector,
    clipping_detector: ClippingDetector,
    options: ExtendedDetectionOptions,
}

impl ExtendedDetectionPipeline {
    pub fn new() -> Self {
        Self {
            enf_detector: EnfDetector::new(),
            clipping_detector: ClippingDetector::new(),
            options: ExtendedDetectionOptions::default(),
        }
    }

    pub fn with_options(options: ExtendedDetectionOptions) -> Self {
        let mut pipeline = Self::new();
        pipeline.options = options.clone();
        
        if options.enf_sensitive_mode {
            pipeline.enf_detector = EnfDetector::new().sensitive();
        }
        
        if options.clipping_strict_mode {
            pipeline.clipping_detector = ClippingDetector::new().strict();
        }
        
        pipeline
    }

    /// Analyze mono audio samples
    pub fn analyze_mono(&self, samples: &[f32], sample_rate: u32) -> ExtendedAnalysisResult {
        let enf_result = if self.options.enable_enf {
            Some(self.enf_detector.analyze(samples, sample_rate))
        } else {
            None
        };

        let clipping_result = if self.options.enable_clipping {
            Some(self.clipping_detector.analyze_mono(samples, sample_rate))
        } else {
            None
        };

        let quality_assessment = self.assess_quality(&clipping_result);
        let authenticity_assessment = self.assess_authenticity(&enf_result);

        ExtendedAnalysisResult {
            enf_result,
            clipping_result,
            quality_assessment,
            authenticity_assessment,
        }
    }

    /// Analyze stereo audio samples
    pub fn analyze_stereo(
        &self,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> ExtendedAnalysisResult {
        // For ENF, mix to mono (ENF is typically captured in both channels)
        let mono: Vec<f32> = left.iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        let enf_result = if self.options.enable_enf {
            Some(self.enf_detector.analyze(&mono, sample_rate))
        } else {
            None
        };

        let clipping_result = if self.options.enable_clipping {
            Some(self.clipping_detector.analyze_stereo(left, right, sample_rate))
        } else {
            None
        };

        let quality_assessment = self.assess_quality(&clipping_result);
        let authenticity_assessment = self.assess_authenticity(&enf_result);

        ExtendedAnalysisResult {
            enf_result,
            clipping_result,
            quality_assessment,
            authenticity_assessment,
        }
    }

    /// Assess audio quality based on clipping analysis
    fn assess_quality(&self, clipping_result: &Option<ClippingAnalysisResult>) -> QualityAssessment {
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();
        let mut score = 1.0f32;

        if let Some(ref result) = clipping_result {
            // Digital clipping penalty
            if result.has_clipping {
                let clipping_penalty = result.severity * 0.4;
                score -= clipping_penalty;
                
                issues.push(QualityIssue {
                    issue_type: QualityIssueType::DigitalClipping,
                    severity: result.severity,
                    description: format!(
                        "{} clipped samples ({:.2}%)",
                        result.statistics.samples_at_digital_max,
                        result.statistics.clipping_percentage
                    ),
                });

                if result.restoration_assessment.restorable {
                    recommendations.push(format!(
                        "Consider restoration using {} method (estimated {:.0}% recovery)",
                        result.restoration_assessment.recommended_method
                            .map(|m| format!("{:?}", m))
                            .unwrap_or_else(|| "unknown".to_string()),
                        result.restoration_assessment.recoverable_percentage
                    ));
                } else {
                    recommendations.push(
                        "Clipping is too severe for effective restoration. \
                         Consider re-recording or using original source.".to_string()
                    );
                }
            }

            // Inter-sample peak penalty
            if result.inter_sample_analysis.inter_sample_overs > 0 {
                let isp_penalty = 0.1 * (result.inter_sample_analysis.inter_sample_overs as f32 / 100.0).min(0.3);
                score -= isp_penalty;
                
                issues.push(QualityIssue {
                    issue_type: QualityIssueType::InterSamplePeaks,
                    severity: isp_penalty / 0.3,
                    description: format!(
                        "{} inter-sample overs detected (true peak: {:.2} dBTP)",
                        result.inter_sample_analysis.inter_sample_overs,
                        result.inter_sample_analysis.true_peak_db
                    ),
                });

                recommendations.push(format!(
                    "Apply {:.1} dB gain reduction to eliminate inter-sample peaks",
                    -result.inter_sample_analysis.inter_sample_headroom_db
                ));
            }

            // Loudness war detection
            if result.loudness_analysis.loudness_war_victim {
                score -= 0.25;
                
                issues.push(QualityIssue {
                    issue_type: QualityIssueType::LoudnessWarVictim,
                    severity: result.loudness_analysis.compression_severity,
                    description: format!(
                        "Loudness war victim: DR {:.1} dB, PLR {:.1} dB",
                        result.loudness_analysis.dynamic_range_db,
                        result.loudness_analysis.plr_db
                    ),
                });

                recommendations.push(
                    "This recording has been over-compressed. \
                     Consider finding an earlier/alternative master with better dynamics.".to_string()
                );
            } else if result.loudness_analysis.dynamic_range_db < 8.0 {
                let dr_penalty = (8.0 - result.loudness_analysis.dynamic_range_db) * 0.03;
                score -= dr_penalty;
                
                issues.push(QualityIssue {
                    issue_type: QualityIssueType::LowDynamicRange,
                    severity: dr_penalty / 0.24,
                    description: format!(
                        "Low dynamic range: {:.1} dB",
                        result.loudness_analysis.dynamic_range_db
                    ),
                });
            }

            // Check for soft clipping / limiter artifacts
            for event in &result.clipping_events {
                match event.clip_type {
                    ClippingType::SoftAnalog => {
                        if !issues.iter().any(|i| i.issue_type == QualityIssueType::SoftClipping) {
                            issues.push(QualityIssue {
                                issue_type: QualityIssueType::SoftClipping,
                                severity: 0.3,
                                description: "Soft/analog-style clipping detected".to_string(),
                            });
                            score -= 0.1;
                        }
                    }
                    ClippingType::Limiter => {
                        if !issues.iter().any(|i| i.issue_type == QualityIssueType::LimiterArtifacts) {
                            issues.push(QualityIssue {
                                issue_type: QualityIssueType::LimiterArtifacts,
                                severity: 0.2,
                                description: "Heavy limiter artifacts detected".to_string(),
                            });
                            score -= 0.05;
                        }
                    }
                    _ => {}
                }
            }
        }

        score = score.clamp(0.0, 1.0);

        let grade = if score >= 0.9 {
            QualityGrade::Excellent
        } else if score >= 0.75 {
            QualityGrade::Good
        } else if score >= 0.5 {
            QualityGrade::Acceptable
        } else if score >= 0.25 {
            QualityGrade::Poor
        } else {
            QualityGrade::Severe
        };

        if issues.is_empty() {
            recommendations.push("No quality issues detected. Audio meets professional standards.".to_string());
        }

        QualityAssessment {
            score,
            grade,
            issues,
            recommendations,
        }
    }

    /// Assess authenticity based on ENF analysis
    fn assess_authenticity(&self, enf_result: &Option<EnfDetectionResult>) -> Option<AuthenticityAssessment> {
        let enf = enf_result.as_ref()?;
        
        let mut evidence = Vec::new();
        let mut anomalies = Vec::new();

        // No ENF detected
        if !enf.enf_detected {
            evidence.push("No ENF signal detected in recording".to_string());
            
            return Some(AuthenticityAssessment {
                confidence: 0.5,
                result: AuthenticityResult::Inconclusive,
                anomalies: vec![],
                estimated_region: None,
                evidence: vec![
                    "No ENF signal detected. This could indicate:".to_string(),
                    "- Recording made in an electrically shielded environment".to_string(),
                    "- Synthetic/digitally generated audio".to_string(),
                    "- Recording location far from power grid".to_string(),
                    "- Heavy noise reduction that removed ENF".to_string(),
                ],
            });
        }

        // ENF detected - analyze for authenticity
        evidence.push(format!(
            "ENF detected at {} Hz with {:.1}% confidence",
            match enf.base_frequency {
                Some(EnfBaseFrequency::Hz50) => "50",
                Some(EnfBaseFrequency::Hz60) => "60",
                None => "unknown",
            },
            enf.confidence * 100.0
        ));

        if let Some(ref region) = enf.estimated_region {
            evidence.push(format!("Estimated recording region: {:?}", region));
        }

        evidence.push(format!(
            "ENF stability score: {:.2} (higher = more stable)",
            enf.stability_score
        ));

        // Convert ENF anomalies to authenticity anomalies
        for anomaly in &enf.anomalies {
            let severity = match anomaly.anomaly_type {
                EnfAnomalyType::FrequencyJump => 0.8,
                EnfAnomalyType::PhaseDiscontinuity => 0.9,
                EnfAnomalyType::SignalDropout => 0.6,
                EnfAnomalyType::DriftRateChange => 0.4,
                EnfAnomalyType::HarmonicAnomaly => 0.3,
            };

            anomalies.push(AuthenticityAnomaly {
                timestamp_secs: anomaly.start_time_secs,
                anomaly_type: format!("{:?}", anomaly.anomaly_type),
                severity,
                description: anomaly.description.clone(),
            });
        }

        // Calculate authenticity score
        let mut authenticity_score = enf.confidence * enf.stability_score;
        
        // Penalize for anomalies
        for anomaly in &anomalies {
            authenticity_score -= anomaly.severity * 0.15;
        }
        authenticity_score = authenticity_score.clamp(0.0, 1.0);

        let result = if anomalies.is_empty() && authenticity_score > 0.8 {
            AuthenticityResult::Authentic
        } else if anomalies.is_empty() && authenticity_score > 0.5 {
            AuthenticityResult::LikelyAuthentic
        } else if anomalies.len() <= 2 && authenticity_score > 0.4 {
            AuthenticityResult::LikelyAuthentic
        } else if anomalies.iter().any(|a| a.severity > 0.7) {
            AuthenticityResult::PotentiallyEdited
        } else if authenticity_score < 0.3 {
            AuthenticityResult::LikelySynthetic
        } else {
            AuthenticityResult::Inconclusive
        };

        if !anomalies.is_empty() {
            evidence.push(format!(
                "{} potential edit point(s) detected via ENF analysis",
                anomalies.len()
            ));
        }

        Some(AuthenticityAssessment {
            confidence: authenticity_score,
            result,
            anomalies,
            estimated_region: enf.estimated_region.clone(),
            evidence,
        })
    }
}

impl Default for ExtendedDetectionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for quick mono analysis with default options
pub fn analyze_audio_quality(samples: &[f32], sample_rate: u32) -> ExtendedAnalysisResult {
    let pipeline = ExtendedDetectionPipeline::new();
    pipeline.analyze_mono(samples, sample_rate)
}

/// Convenience function for quick stereo analysis with default options
pub fn analyze_stereo_quality(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> ExtendedAnalysisResult {
    let pipeline = ExtendedDetectionPipeline::new();
    pipeline.analyze_stereo(left, right, sample_rate)
}

/// Analyze audio for authenticity (ENF-based)
pub fn analyze_authenticity(samples: &[f32], sample_rate: u32) -> Option<AuthenticityAssessment> {
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enable_clipping: false,
        ..Default::default()
    };
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    pipeline.analyze_mono(samples, sample_rate).authenticity_assessment
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_sine_wave(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * frequency * t).sin()
            })
            .collect()
    }

    fn generate_clipped_sine(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
        generate_sine_wave(frequency, sample_rate, duration_secs)
            .into_iter()
            .map(|s| {
                let amplified = s * 1.5;
                amplified.clamp(-1.0, 1.0)
            })
            .collect()
    }

    #[test]
    fn test_quality_assessment_clean_audio() {
        let samples = generate_sine_wave(440.0, 44100, 1.0);
        let result = analyze_audio_quality(&samples, 44100);
        
        assert!(result.quality_assessment.score > 0.9);
        assert_eq!(result.quality_assessment.grade, QualityGrade::Excellent);
        assert!(result.quality_assessment.issues.is_empty());
    }

    #[test]
    fn test_quality_assessment_clipped_audio() {
        let samples = generate_clipped_sine(440.0, 44100, 1.0);
        let result = analyze_audio_quality(&samples, 44100);
        
        assert!(result.quality_assessment.score < 0.9);
        assert!(result.quality_assessment.issues.iter()
            .any(|i| i.issue_type == QualityIssueType::DigitalClipping));
    }

    #[test]
    fn test_extended_pipeline_default_options() {
        let options = ExtendedDetectionOptions::default();
        
        assert!(!options.enable_enf);  // Off by default
        assert!(options.enable_clipping);  // On by default
        assert!(options.enable_inter_sample_peaks);
        assert!(options.enable_loudness_analysis);
    }

    #[test]
    fn test_extended_pipeline_enf_enabled() {
        let options = ExtendedDetectionOptions {
            enable_enf: true,
            ..Default::default()
        };
        
        let samples = generate_sine_wave(440.0, 44100, 2.0);
        let pipeline = ExtendedDetectionPipeline::with_options(options);
        let result = pipeline.analyze_mono(&samples, 44100);
        
        assert!(result.enf_result.is_some());
        assert!(result.authenticity_assessment.is_some());
    }

    #[test]
    fn test_stereo_analysis() {
        let left = generate_sine_wave(440.0, 44100, 1.0);
        let right = generate_sine_wave(440.0, 44100, 1.0);
        
        let result = analyze_stereo_quality(&left, &right, 44100);
        
        assert!(result.clipping_result.is_some());
        assert!(result.quality_assessment.score > 0.9);
    }

    #[test]
    fn test_authenticity_analysis_no_enf() {
        // Pure sine wave has no ENF
        let samples = generate_sine_wave(440.0, 44100, 2.0);
        let assessment = analyze_authenticity(&samples, 44100);
        
        assert!(assessment.is_some());
        let auth = assessment.unwrap();
        // Clean sine has no ENF signal
        assert_eq!(auth.result, AuthenticityResult::Inconclusive);
    }
}
