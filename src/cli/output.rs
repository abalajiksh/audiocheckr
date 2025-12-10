// src/cli/output.rs
//
// Output formatting for CLI

use colorful::Colorful;
use crate::core::{QualityReport, DetectedDefect, DefectType};

/// Print human-readable report
pub fn print_report(report: &QualityReport, verbose: bool) {
    println!("  Sample Rate: {} Hz", report.sample_rate);
    println!("  Bit Depth: {} bit (claimed: {})", 
        report.actual_bit_depth, report.claimed_bit_depth);
    println!("  Channels: {}", report.channels);
    println!("  Duration: {:.2}s", report.duration_secs);
    println!("  Frequency Cutoff: {:.0} Hz", report.frequency_cutoff);
    println!("  Dynamic Range: {:.1} dB", report.dynamic_range);

    let score_str = format!("{:.0}%", report.quality_score * 100.0);
    let quality_label = if report.is_likely_lossless {
        format!("Quality Score: {} (likely lossless)", score_str).green()
    } else {
        format!("Quality Score: {} (likely transcoded)", score_str).yellow()
    };
    println!("  {}", quality_label);

    if report.defects.is_empty() {
        println!("  Status: {}", "✓ CLEAN - No issues detected".green());
    } else {
        println!("  Status: {}", "✗ ISSUES DETECTED".red());
        for defect in &report.defects {
            println!("    • {}", format_defect(defect).yellow());
            
            if verbose {
                for evidence in &defect.evidence {
                    let evidence_clone = evidence.clone();
                    println!("      - {}", evidence_clone.dark_gray());
                }
            }
        }
    }
}

/// Print JSON output
pub fn print_json(report: &QualityReport) -> anyhow::Result<()> {
    let json_output = serde_json::json!({
        "sample_rate": report.sample_rate,
        "channels": report.channels,
        "claimed_bit_depth": report.claimed_bit_depth,
        "actual_bit_depth": report.actual_bit_depth,
        "duration_secs": report.duration_secs,
        "frequency_cutoff": report.frequency_cutoff,
        "quality_score": report.quality_score,
        "is_likely_lossless": report.is_likely_lossless,
        "defects": report.defects.iter().map(|d| {
            serde_json::json!({
                "type": format_defect_type(&d.defect_type),
                "confidence": d.confidence,
                "evidence": d.evidence,
            })
        }).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&json_output)?);
    Ok(())
}

fn format_defect(defect: &DetectedDefect) -> String {
    let conf_str = format!(" ({:.0}% confidence)", defect.confidence * 100.0);
    
    match &defect.defect_type {
        DefectType::Mp3Transcode { cutoff_hz, estimated_bitrate } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" (~{}kbps)", b))
                .unwrap_or_default();
            format!("MP3 transcode{} - cutoff at {} Hz{}", bitrate_str, cutoff_hz, conf_str)
        },
        DefectType::BitDepthMismatch { claimed, actual, .. } => {
            format!("Bit depth mismatch: claims {}-bit, actually {}-bit{}", claimed, actual, conf_str)
        },
        DefectType::Upsampled { from, to, .. } => {
            format!("Upsampled from {} Hz to {} Hz{}", from, to, conf_str)
        },
        DefectType::PreEcho { score } => {
            format!("Pre-echo artifacts (score: {:.2}){}", score, conf_str)
        },
        _ => format!("{:?}{}", defect.defect_type, conf_str),
    }
}

fn format_defect_type(defect: &DefectType) -> String {
    match defect {
        DefectType::Mp3Transcode { .. } => "MP3 Transcode".to_string(),
        DefectType::OggVorbisTranscode { .. } => "Ogg Vorbis Transcode".to_string(),
        DefectType::AacTranscode { .. } => "AAC Transcode".to_string(),
        DefectType::OpusTranscode { .. } => "Opus Transcode".to_string(),
        DefectType::BitDepthMismatch { .. } => "Bit Depth Mismatch".to_string(),
        DefectType::Upsampled { .. } => "Upsampled".to_string(),
        DefectType::SpectralArtifacts { .. } => "Spectral Artifacts".to_string(),
        DefectType::JointStereo { .. } => "Joint Stereo".to_string(),
        DefectType::PreEcho { .. } => "Pre-Echo".to_string(),
        DefectType::PhaseDiscontinuities { .. } => "Phase Discontinuities".to_string(),
        DefectType::Clipping { .. } => "Clipping".to_string(),
        DefectType::InterSampleOvers { .. } => "Inter-Sample Overs".to_string(),
        DefectType::LowQuality { .. } => "Low Quality".to_string(),
    }
}
