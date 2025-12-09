//! Detection result types with profile-aware confidence scoring

use crate::config::{DetectorType, ProfileConfig};

/// Severity level for a detection finding
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational - may be expected behavior
    Info,
    /// Low confidence - possible issue
    Low,
    /// Medium confidence - likely issue
    Medium,
    /// High confidence - definite issue
    High,
}

impl Severity {
    pub fn from_confidence(confidence: f32) -> Self {
        match confidence {
            c if c >= 0.85 => Severity::High,
            c if c >= 0.65 => Severity::Medium,
            c if c >= 0.40 => Severity::Low,
            _ => Severity::Info,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Severity::Info => "ℹ",
            Severity::Low => "⚠",
            Severity::Medium => "⚠",
            Severity::High => "✗",
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            Severity::Info => "\x1b[36m",  // cyan
            Severity::Low => "\x1b[33m",   // yellow
            Severity::Medium => "\x1b[33m", // yellow
            Severity::High => "\x1b[31m",  // red
        }
    }
}

/// Raw detection result from a detector (before profile adjustment)
#[derive(Debug, Clone)]
pub struct RawDetection {
    pub detector: DetectorType,
    pub confidence: f32,
    pub description: String,
    pub details: Option<String>,
}

impl RawDetection {
    pub fn new(detector: DetectorType, confidence: f32, description: impl Into<String>) -> Self {
        Self {
            detector,
            confidence: confidence.clamp(0.0, 1.0),
            description: description.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Profile-adjusted detection finding
#[derive(Debug, Clone)]
pub struct Finding {
    pub detector: DetectorType,
    pub raw_confidence: f32,
    pub adjusted_confidence: f32,
    pub severity: Severity,
    pub description: String,
    pub details: Option<String>,
    /// Whether this finding was suppressed by profile (for verbose output)
    pub suppressed: bool,
}

impl Finding {
    /// Create a finding from a raw detection and profile
    pub fn from_raw(raw: RawDetection, profile: &ProfileConfig) -> Self {
        let adjusted = profile.adjust_confidence(raw.detector, raw.confidence);

        match adjusted {
            Some(conf) => Self {
                detector: raw.detector,
                raw_confidence: raw.confidence,
                adjusted_confidence: conf,
                severity: Severity::from_confidence(conf),
                description: raw.description,
                details: raw.details,
                suppressed: false,
            },
            None => Self {
                detector: raw.detector,
                raw_confidence: raw.confidence,
                adjusted_confidence: 0.0,
                severity: Severity::Info,
                description: raw.description,
                details: raw.details,
                suppressed: true,
            },
        }
    }
}

/// Overall analysis result for a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisVerdict {
    /// File appears to be genuine lossless
    Genuine,
    /// File appears to be transcoded/fake lossless
    Transcoded,
    /// Uncertain - some flags but not definitive
    Suspicious,
    /// Unable to determine
    Unknown,
}

impl AnalysisVerdict {
    pub fn symbol(&self) -> &'static str {
        match self {
            AnalysisVerdict::Genuine => "✓",
            AnalysisVerdict::Transcoded => "✗",
            AnalysisVerdict::Suspicious => "?",
            AnalysisVerdict::Unknown => "—",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AnalysisVerdict::Genuine => "Appears genuine",
            AnalysisVerdict::Transcoded => "Likely transcoded",
            AnalysisVerdict::Suspicious => "Suspicious",
            AnalysisVerdict::Unknown => "Unable to determine",
        }
    }
}

/// Complete analysis result for a file
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub file_path: String,
    pub profile_name: String,
    pub verdict: AnalysisVerdict,
    pub findings: Vec<Finding>,
    pub overall_confidence: f32,
}

impl AnalysisResult {
    pub fn new(file_path: impl Into<String>, profile: &ProfileConfig) -> Self {
        Self {
            file_path: file_path.into(),
            profile_name: profile.name.clone(),
            verdict: AnalysisVerdict::Unknown,
            findings: Vec::new(),
            overall_confidence: 0.0,
        }
    }

    /// Add findings from raw detections
    pub fn add_detections(&mut self, raws: Vec<RawDetection>, profile: &ProfileConfig) {
        for raw in raws {
            self.findings.push(Finding::from_raw(raw, profile));
        }
        self.compute_verdict();
    }

    /// Compute overall verdict from findings
    fn compute_verdict(&mut self) {
        let active_findings: Vec<_> = self.findings.iter().filter(|f| !f.suppressed).collect();

        if active_findings.is_empty() {
            self.verdict = AnalysisVerdict::Unknown;
            self.overall_confidence = 0.0;
            return;
        }

        // Weighted average of confidence scores
        let total_confidence: f32 = active_findings.iter().map(|f| f.adjusted_confidence).sum();
        let avg_confidence = total_confidence / active_findings.len() as f32;

        // Count high-severity findings
        let high_count = active_findings
            .iter()
            .filter(|f| f.severity == Severity::High)
            .count();

        self.overall_confidence = avg_confidence;
        self.verdict = match (high_count, avg_confidence) {
            (h, _) if h >= 2 => AnalysisVerdict::Transcoded,
            (1, c) if c >= 0.7 => AnalysisVerdict::Transcoded,
            (_, c) if c >= 0.5 => AnalysisVerdict::Suspicious,
            (_, c) if c < 0.3 => AnalysisVerdict::Genuine,
            _ => AnalysisVerdict::Suspicious,
        };
    }

    /// Get only active (non-suppressed) findings
    pub fn active_findings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| !f.suppressed)
    }

    /// Get suppressed findings (for verbose output)
    pub fn suppressed_findings(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.suppressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProfilePreset;

    #[test]
    fn test_severity_from_confidence() {
        assert_eq!(Severity::from_confidence(0.9), Severity::High);
        assert_eq!(Severity::from_confidence(0.7), Severity::Medium);
        assert_eq!(Severity::from_confidence(0.5), Severity::Low);
        assert_eq!(Severity::from_confidence(0.2), Severity::Info);
    }

    #[test]
    fn test_finding_suppression() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Noise);

        // This should be suppressed due to noise profile's spectral cutoff modifier
        let raw = RawDetection::new(
            DetectorType::SpectralCutoff,
            0.6,
            "Cutoff at 16kHz",
        );

        let finding = Finding::from_raw(raw, &profile);
        // With 0.3x multiplier and 0.7 threshold, 0.6 * 0.3 = 0.18 < 0.7
        assert!(finding.suppressed);
    }

    #[test]
    fn test_analysis_verdict() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Standard);
        let mut result = AnalysisResult::new("test.flac", &profile);

        let detections = vec![
            RawDetection::new(DetectorType::SpectralCutoff, 0.9, "Sharp cutoff at 16kHz"),
            RawDetection::new(DetectorType::PreEcho, 0.85, "MP3 pre-echo detected"),
        ];

        result.add_detections(detections, &profile);

        // Two high-confidence findings should result in Transcoded verdict
        assert_eq!(result.verdict, AnalysisVerdict::Transcoded);
    }
}
