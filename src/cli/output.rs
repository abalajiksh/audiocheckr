//! Output formatting for CLI results

use crate::detection::{AnalysisResult, AnalysisVerdict, Finding, Severity};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

/// Format analysis result for terminal output
pub fn format_result(result: &AnalysisResult, verbose: bool, show_suppressed: bool) -> String {
    let mut output = String::new();

    // Header with verdict
    let verdict_color = match result.verdict {
        AnalysisVerdict::Genuine => "\x1b[32m",    // green
        AnalysisVerdict::Transcoded => "\x1b[31m", // red
        AnalysisVerdict::Suspicious => "\x1b[33m", // yellow
        AnalysisVerdict::Unknown => "\x1b[90m",    // gray
    };

    output.push_str(&format!(
        "{}{} {}{} {}{}{}\n",
        verdict_color,
        result.verdict.symbol(),
        BOLD,
        result.file_path,
        RESET,
        DIM,
        format!(" [{}]", result.profile_name),
    ));

    output.push_str(&format!(
        "  {} (confidence: {:.0}%)\n",
        result.verdict.description(),
        result.overall_confidence * 100.0
    ));

    // Active findings
    let active: Vec<_> = result.active_findings().collect();
    if !active.is_empty() {
        output.push_str("\n  Findings:\n");
        for finding in active {
            output.push_str(&format_finding(finding, verbose));
        }
    }

    // Suppressed findings (if requested)
    if show_suppressed {
        let suppressed: Vec<_> = result.suppressed_findings().collect();
        if !suppressed.is_empty() {
            output.push_str(&format!("\n  {}Suppressed by profile:{}\n", DIM, RESET));
            for finding in suppressed {
                output.push_str(&format_suppressed_finding(finding));
            }
        }
    }

    output
}

fn format_finding(finding: &Finding, verbose: bool) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "    {}{} {}{} ",
        finding.severity.color_code(),
        finding.severity.symbol(),
        RESET,
        finding.description
    ));

    output.push_str(&format!(
        "{}({:.0}%){}\n",
        DIM,
        finding.adjusted_confidence * 100.0,
        RESET
    ));

    if verbose {
        output.push_str(&format!(
            "      {}Detector: {} | Raw: {:.0}% | Adjusted: {:.0}%{}\n",
            DIM,
            finding.detector.name(),
            finding.raw_confidence * 100.0,
            finding.adjusted_confidence * 100.0,
            RESET
        ));

        if let Some(details) = &finding.details {
            output.push_str(&format!("      {}{}{}\n", DIM, details, RESET));
        }
    }

    output
}

fn format_suppressed_finding(finding: &Finding) -> String {
    format!(
        "    {}- {} ({}: {:.0}% → suppressed){}\n",
        DIM,
        finding.description,
        finding.detector.name(),
        finding.raw_confidence * 100.0,
        RESET
    )
}

/// Format analysis result as JSON
pub fn format_json(result: &AnalysisResult) -> String {
    let findings_json: Vec<String> = result
        .findings
        .iter()
        .map(|f| {
            format!(
                r#"    {{
      "detector": "{}",
      "description": "{}",
      "raw_confidence": {:.3},
      "adjusted_confidence": {:.3},
      "severity": "{}",
      "suppressed": {}
    }}"#,
                f.detector.name(),
                f.description.replace('"', "\\\""),
                f.raw_confidence,
                f.adjusted_confidence,
                format!("{:?}", f.severity).to_lowercase(),
                f.suppressed
            )
        })
        .collect();

    format!(
        r#"{{
  "file": "{}",
  "profile": "{}",
  "verdict": "{}",
  "overall_confidence": {:.3},
  "findings": [
{}
  ]
}}"#,
        result.file_path.replace('"', "\\\""),
        result.profile_name,
        format!("{:?}", result.verdict).to_lowercase(),
        result.overall_confidence,
        findings_json.join(",\n")
    )
}

/// Format a summary for multiple files
pub fn format_summary(results: &[AnalysisResult]) -> String {
    let mut output = String::new();

    let genuine = results
        .iter()
        .filter(|r| r.verdict == AnalysisVerdict::Genuine)
        .count();
    let transcoded = results
        .iter()
        .filter(|r| r.verdict == AnalysisVerdict::Transcoded)
        .count();
    let suspicious = results
        .iter()
        .filter(|r| r.verdict == AnalysisVerdict::Suspicious)
        .count();
    let unknown = results
        .iter()
        .filter(|r| r.verdict == AnalysisVerdict::Unknown)
        .count();

    output.push_str(&format!("\n{}Summary:{}\n", BOLD, RESET));
    output.push_str(&format!("  {} files analyzed\n", results.len()));

    if genuine > 0 {
        output.push_str(&format!("  \x1b[32m✓ {} genuine{}\n", genuine, RESET));
    }
    if transcoded > 0 {
        output.push_str(&format!("  \x1b[31m✗ {} transcoded{}\n", transcoded, RESET));
    }
    if suspicious > 0 {
        output.push_str(&format!("  \x1b[33m? {} suspicious{}\n", suspicious, RESET));
    }
    if unknown > 0 {
        output.push_str(&format!("  \x1b[90m— {} unknown{}\n", unknown, RESET));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DetectorType, ProfileConfig, ProfilePreset};
    use crate::detection::RawDetection;

    #[test]
    fn test_format_result() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Standard);
        let mut result = AnalysisResult::new("test.flac", &profile);

        let detections = vec![RawDetection::new(
            DetectorType::SpectralCutoff,
            0.85,
            "Sharp cutoff at 16kHz",
        )];

        result.add_detections(detections, &profile);

        let output = format_result(&result, false, false);
        assert!(output.contains("test.flac"));
        assert!(output.contains("16kHz"));
    }

    #[test]
    fn test_format_json() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Standard);
        let mut result = AnalysisResult::new("test.flac", &profile);

        let detections = vec![RawDetection::new(
            DetectorType::PreEcho,
            0.9,
            "MP3 pre-echo detected",
        )];

        result.add_detections(detections, &profile);

        let json = format_json(&result);
        assert!(json.contains("\"file\": \"test.flac\""));
        assert!(json.contains("\"detector\": \"pre_echo\""));
    }
}
