// src/detection/result.rs
//
// Profile-aware detection result types

use serde::{Deserialize, Serialize};
use crate::config::{DetectorType, ProfileConfig};

/// Severity level for findings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Raw detection output from a detector (before profile adjustment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDetection {
    /// Which detector produced this
    pub detector: DetectorType,
    /// Raw confidence before profile adjustment
    pub raw_confidence: f32,
    /// Detection severity
    pub severity: Severity,
    /// Short description
    pub summary: String,
    /// Detailed evidence
    pub evidence: Vec<String>,
    /// Machine-readable data
    pub data: serde_json::Value,
}

/// Profile-adjusted finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Which detector produced this
    pub detector: DetectorType,
    /// Adjusted confidence after profile
    pub confidence: f32,
    /// Original raw confidence
    pub raw_confidence: f32,
    /// Detection severity
    pub severity: Severity,
    /// Short description
    pub summary: String,
    /// Detailed evidence
    pub evidence: Vec<String>,
    /// Whether this was suppressed by profile
    pub suppressed: bool,
    /// Machine-readable data
    pub data: serde_json::Value,
}

impl Finding {
    pub fn from_raw(raw: &RawDetection, profile: &ProfileConfig) -> Self {
        let modifier = profile.get_modifier(raw.detector);
        let adjusted_confidence = profile.adjust_confidence(raw.detector, raw.raw_confidence);
        
        Self {
            detector: raw.detector,
            confidence: adjusted_confidence,
            raw_confidence: raw.raw_confidence,
            severity: raw.severity,
            summary: raw.summary.clone(),
            evidence: raw.evidence.clone(),
            suppressed: modifier.suppress_from_verdict || modifier.multiplier == 0.0,
            data: raw.data.clone(),
        }
    }
}

/// Overall verdict for the file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisVerdict {
    /// High confidence the file is genuine lossless
    Lossless,
    /// Likely lossless but some uncertainty
    ProbablyLossless,
    /// Uncertain - manual review recommended
    Uncertain,
    /// Likely transcoded or has issues
    ProbablyLossy,
    /// High confidence the file is transcoded or fake
    Lossy,
}

/// Complete analysis result with profile-aware findings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Profile used for analysis
    pub profile_name: String,
    /// Overall verdict
    pub verdict: AnalysisVerdict,
    /// Quality score (0.0-1.0)
    pub quality_score: f32,
    /// All findings (including suppressed)
    pub findings: Vec<Finding>,
    /// Active findings (not suppressed, above threshold)
    pub active_findings: Vec<Finding>,
    /// Suppressed findings (for verbose/debug mode)
    pub suppressed_findings: Vec<Finding>,
}

impl AnalysisResult {
    /// Create new result from raw detections and profile
    pub fn from_detections(
        raw_detections: Vec<RawDetection>,
        profile: &ProfileConfig,
    ) -> Self {
        let findings: Vec<Finding> = raw_detections
            .iter()
            .map(|r| Finding::from_raw(r, profile))
            .collect();
        
        // Separate active and suppressed
        let active_findings: Vec<Finding> = findings
            .iter()
            .filter(|f| !f.suppressed && f.confidence >= profile.min_confidence)
            .cloned()
            .collect();
        
        let suppressed_findings: Vec<Finding> = findings
            .iter()
            .filter(|f| f.suppressed || f.confidence < profile.min_confidence)
            .cloned()
            .collect();
        
        // Calculate quality score
        let quality_score = Self::calculate_quality_score(&active_findings);
        
        // Determine verdict
        let verdict = Self::determine_verdict(&active_findings, quality_score);
        
        AnalysisResult {
            profile_name: profile.name.clone(),
            verdict,
            quality_score,
            findings,
            active_findings,
            suppressed_findings,
        }
    }
    
    /// Add detections to existing result
    pub fn add_detections(&mut self, raw_detections: Vec<RawDetection>, profile: &ProfileConfig) {
        for raw in raw_detections {
            let finding = Finding::from_raw(&raw, profile);
            
            if !finding.suppressed && finding.confidence >= profile.min_confidence {
                self.active_findings.push(finding.clone());
            } else {
                self.suppressed_findings.push(finding.clone());
            }
            
            self.findings.push(finding);
        }
        
        // Recalculate score and verdict
        self.quality_score = Self::calculate_quality_score(&self.active_findings);
        self.verdict = Self::determine_verdict(&self.active_findings, self.quality_score);
    }
    
    fn calculate_quality_score(findings: &[Finding]) -> f32 {
        if findings.is_empty() {
            return 1.0;
        }
        
        let mut score = 1.0f32;
        
        for finding in findings {
            let penalty = match finding.severity {
                Severity::Critical => 0.4,
                Severity::Error => 0.25,
                Severity::Warning => 0.15,
                Severity::Info => 0.05,
            };
            score -= penalty * finding.confidence;
        }
        
        score.clamp(0.0, 1.0)
    }
    
    fn determine_verdict(findings: &[Finding], quality_score: f32) -> AnalysisVerdict {
        let critical_count = findings.iter()
            .filter(|f| f.severity == Severity::Critical && f.confidence > 0.7)
            .count();
        
        let error_count = findings.iter()
            .filter(|f| f.severity == Severity::Error && f.confidence > 0.6)
            .count();
        
        if critical_count > 0 {
            AnalysisVerdict::Lossy
        } else if error_count >= 2 || quality_score < 0.4 {
            AnalysisVerdict::ProbablyLossy
        } else if error_count == 1 || quality_score < 0.6 {
            AnalysisVerdict::Uncertain
        } else if quality_score < 0.85 {
            AnalysisVerdict::ProbablyLossless
        } else {
            AnalysisVerdict::Lossless
        }
    }
    
    /// Get findings that affect the verdict
    pub fn verdict_findings(&self) -> &[Finding] {
        &self.active_findings
    }
    
    /// Check if file passed analysis
    pub fn is_clean(&self) -> bool {
        matches!(
            self.verdict,
            AnalysisVerdict::Lossless | AnalysisVerdict::ProbablyLossless
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verdict_clean() {
        let result = AnalysisResult {
            profile_name: "Test".to_string(),
            verdict: AnalysisVerdict::Lossless,
            quality_score: 1.0,
            findings: vec![],
            active_findings: vec![],
            suppressed_findings: vec![],
        };
        
        assert!(result.is_clean());
    }
    
    #[test]
    fn test_quality_score_penalty() {
        let findings = vec![
            Finding {
                detector: DetectorType::SpectralCutoff,
                confidence: 0.8,
                raw_confidence: 0.8,
                severity: Severity::Error,
                summary: "Test".to_string(),
                evidence: vec![],
                suppressed: false,
                data: serde_json::Value::Null,
            },
        ];
        
        let score = AnalysisResult::calculate_quality_score(&findings);
        assert!(score < 1.0);
        assert!(score > 0.5);
    }
}
