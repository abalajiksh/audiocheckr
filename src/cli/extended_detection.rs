//! CLI Integration for ENF and Clipping Detection
//!
//! This module provides command-line argument handling for the new detection features.
//! Integrate these options into your existing CLI module (e.g., cli/mod.rs or main.rs).

use clap::{Args, ValueEnum};

/// ENF and Clipping detection CLI arguments
/// Add these to your existing Args struct using #[command(flatten)]
#[derive(Args, Debug, Clone)]
pub struct ExtendedDetectionArgs {
    /// Enable ENF (Electrical Network Frequency) analysis for authenticity verification
    #[arg(long, help = "Analyze recording for power grid frequency signatures")]
    pub enf: bool,

    /// Use sensitive ENF detection mode for noisy recordings
    #[arg(long, requires = "enf", help = "Use higher sensitivity ENF detection")]
    pub enf_sensitive: bool,

    /// Expected ENF base frequency (auto-detect if not specified)
    #[arg(long, requires = "enf", value_enum, help = "Expected power grid frequency")]
    pub enf_frequency: Option<EnfFrequencyArg>,

    /// Disable clipping detection (enabled by default)
    #[arg(long, help = "Disable clipping and loudness analysis")]
    pub no_clipping: bool,

    /// Use strict clipping detection (broadcast standards)
    #[arg(long, help = "Use strict thresholds for broadcast compliance")]
    pub clipping_strict: bool,

    /// Disable inter-sample peak analysis
    #[arg(long, help = "Skip computationally intensive true peak calculation")]
    pub no_inter_sample: bool,

    /// Disable loudness war detection
    #[arg(long, help = "Skip loudness and dynamic range analysis")]
    pub no_loudness: bool,

    /// Output format for extended analysis results
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub extended_output: ExtendedOutputFormat,
}

#[derive(ValueEnum, Clone, Debug, Copy)]
pub enum EnfFrequencyArg {
    /// 50 Hz (Europe, Asia, Africa, Australia)
    #[value(name = "50")]
    Hz50,
    /// 60 Hz (North America, parts of South America)
    #[value(name = "60")]
    Hz60,
}

#[derive(ValueEnum, Clone, Debug, Copy, Default)]
pub enum ExtendedOutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output for programmatic consumption
    Json,
    /// Detailed report format
    Report,
}

impl Default for ExtendedDetectionArgs {
    fn default() -> Self {
        Self {
            enf: false,
            enf_sensitive: false,
            enf_frequency: None,
            no_clipping: false,
            clipping_strict: false,
            no_inter_sample: false,
            no_loudness: false,
            extended_output: ExtendedOutputFormat::Text,
        }
    }
}

// ============================================================================
// Example Integration Code
// ============================================================================

/*
// Add to your existing CLI Args struct:

use crate::cli::extended_detection::{ExtendedDetectionArgs, ExtendedOutputFormat};

#[derive(Parser)]
#[command(name = "audiocheckr")]
#[command(about = "Audio quality analysis and fake lossless detection")]
pub struct Cli {
    // ... existing arguments ...

    #[command(flatten)]
    pub extended: ExtendedDetectionArgs,
}

// In your analysis function:

fn run_analysis(cli: &Cli, samples: &[f32], sample_rate: u32) {
    // Convert CLI args to detection options
    let extended_options = ExtendedDetectionOptions {
        enable_enf: cli.extended.enf,
        enf_sensitive_mode: cli.extended.enf_sensitive,
        expected_enf_frequency: cli.extended.enf_frequency.map(|f| match f {
            EnfFrequencyArg::Hz50 => EnfBaseFrequency::Hz50,
            EnfFrequencyArg::Hz60 => EnfBaseFrequency::Hz60,
        }),
        enable_clipping: !cli.extended.no_clipping,
        clipping_strict_mode: cli.extended.clipping_strict,
        enable_inter_sample_peaks: !cli.extended.no_inter_sample,
        enable_loudness_analysis: !cli.extended.no_loudness,
    };

    let pipeline = ExtendedDetectionPipeline::with_options(extended_options);
    let result = pipeline.analyze_mono(samples, sample_rate);

    // Output results based on format
    match cli.extended.extended_output {
        ExtendedOutputFormat::Text => print_text_report(&result),
        ExtendedOutputFormat::Json => print_json_report(&result),
        ExtendedOutputFormat::Report => print_detailed_report(&result),
    }
}
*/

// ============================================================================
// Output Formatting Functions
// ============================================================================

use crate::analysis::detection_pipeline_enf_clipping::{
    ExtendedAnalysisResult, QualityGrade, QualityIssueType, AuthenticityResult,
};

/// Print text format report
pub fn print_text_report(result: &ExtendedAnalysisResult) {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              EXTENDED AUDIO ANALYSIS REPORT                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Quality Assessment
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ QUALITY ASSESSMENT                                          │");
    println!("├─────────────────────────────────────────────────────────────┤");
    
    let grade_icon = match result.quality_assessment.grade {
        QualityGrade::Excellent => "★★★★★",
        QualityGrade::Good => "★★★★☆",
        QualityGrade::Acceptable => "★★★☆☆",
        QualityGrade::Poor => "★★☆☆☆",
        QualityGrade::Severe => "★☆☆☆☆",
    };
    
    println!("│ Grade: {} {}                          │", 
        result.quality_assessment.grade, grade_icon);
    println!("│ Score: {:.1}/100                                             │",
        result.quality_assessment.score * 100.0);
    println!("└─────────────────────────────────────────────────────────────┘");

    // Issues
    if !result.quality_assessment.issues.is_empty() {
        println!("\n┌─────────────────────────────────────────────────────────────┐");
        println!("│ ISSUES DETECTED                                             │");
        println!("├─────────────────────────────────────────────────────────────┤");
        
        for issue in &result.quality_assessment.issues {
            let icon = match issue.issue_type {
                QualityIssueType::DigitalClipping => "🔴",
                QualityIssueType::InterSamplePeaks => "🟠",
                QualityIssueType::LoudnessWarVictim => "🔴",
                QualityIssueType::LowDynamicRange => "🟡",
                QualityIssueType::HighCompressionSeverity => "🟠",
                QualityIssueType::SoftClipping => "🟡",
                QualityIssueType::LimiterArtifacts => "🟡",
            };
            println!("│ {} {:?}: {}",
                icon, issue.issue_type, issue.description);
        }
        println!("└─────────────────────────────────────────────────────────────┘");
    }

    // Clipping Details
    if let Some(ref clip) = result.clipping_result {
        println!("\n┌─────────────────────────────────────────────────────────────┐");
        println!("│ CLIPPING ANALYSIS                                           │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ Digital Clipping: {}                                        │",
            if clip.has_clipping { "YES" } else { "NO" });
        println!("│ Clipped Samples: {} ({:.4}%)                                │",
            clip.statistics.samples_at_digital_max,
            clip.statistics.clipping_percentage);
        println!("│ Sample Peak: {:.2} dBFS                                     │",
            clip.statistics.peak_level_db);
        println!("│ True Peak: {:.2} dBTP                                       │",
            clip.inter_sample_analysis.true_peak_db);
        println!("│ Headroom: {:.2} dB                                          │",
            clip.inter_sample_analysis.headroom_db);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ LOUDNESS METRICS                                            │");
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ Integrated Loudness: {:.1} LUFS                             │",
            clip.loudness_analysis.integrated_loudness_lufs);
        println!("│ Dynamic Range (DR): {:.1} dB                                │",
            clip.loudness_analysis.dynamic_range_db);
        println!("│ Crest Factor: {:.1} dB                                      │",
            clip.loudness_analysis.crest_factor_db);
        println!("│ PLR (Peak-to-Loudness): {:.1} dB                            │",
            clip.loudness_analysis.peak_to_loudness_ratio);
        println!("│ Loudness War Victim: {}                                     │",
            if clip.loudness_analysis.loudness_war_victim { "YES ⚠️" } else { "NO" });
        println!("└─────────────────────────────────────────────────────────────┘");

        // Restoration Assessment
        if clip.has_clipping {
            println!("\n┌─────────────────────────────────────────────────────────────┐");
            println!("│ RESTORATION ASSESSMENT                                      │");
            println!("├─────────────────────────────────────────────────────────────┤");
            println!("│ Restorable: {}                                              │",
                if clip.restoration_assessment.restorable { "YES" } else { "NO" });
            if let Some(method) = &clip.restoration_assessment.recommended_method {
                println!("│ Recommended Method: {:?}                                    │", method);
            }
            println!("│ Estimated Quality: {:.0}%                                    │",
                clip.restoration_assessment.estimated_quality * 100.0);
            println!("│ Recoverable: {:.0}%                                          │",
                clip.restoration_assessment.recoverable_percentage);
            println!("└─────────────────────────────────────────────────────────────┘");
        }
    }

    // ENF/Authenticity Assessment
    if let Some(ref auth) = result.authenticity_assessment {
        println!("\n┌─────────────────────────────────────────────────────────────┐");
        println!("│ AUTHENTICITY ASSESSMENT (ENF Analysis)                      │");
        println!("├─────────────────────────────────────────────────────────────┤");
        
        let auth_icon = match auth.result {
            AuthenticityResult::Authentic => "✅",
            AuthenticityResult::LikelyAuthentic => "✅",
            AuthenticityResult::Inconclusive => "❓",
            AuthenticityResult::PotentiallyEdited => "⚠️",
            AuthenticityResult::LikelySynthetic => "🤖",
        };
        
        println!("│ Result: {} {}                                              │",
            auth_icon, auth.result);
        println!("│ Confidence: {:.1}%                                          │",
            auth.confidence * 100.0);
        
        if let Some(ref region) = auth.estimated_region {
            println!("│ Estimated Region: {:?}                                      │", region);
        }
        
        if !auth.anomalies.is_empty() {
            println!("├─────────────────────────────────────────────────────────────┤");
            println!("│ DETECTED ANOMALIES                                          │");
            for anomaly in &auth.anomalies {
                println!("│ • {:.1}s: {} (severity: {:.0}%)                             │",
                    anomaly.timestamp_secs, anomaly.anomaly_type, anomaly.severity * 100.0);
            }
        }
        
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ EVIDENCE                                                     │");
        for evidence in &auth.evidence {
            println!("│ • {}                                                        │", evidence);
        }
        println!("└─────────────────────────────────────────────────────────────┘");
    }

    // Recommendations
    if !result.quality_assessment.recommendations.is_empty() {
        println!("\n┌─────────────────────────────────────────────────────────────┐");
        println!("│ RECOMMENDATIONS                                             │");
        println!("├─────────────────────────────────────────────────────────────┤");
        for rec in &result.quality_assessment.recommendations {
            println!("│ → {}                                                        │", rec);
        }
        println!("└─────────────────────────────────────────────────────────────┘");
    }

    println!();
}

/// Print JSON format report
pub fn print_json_report(result: &ExtendedAnalysisResult) {
    // In production, use serde_json::to_string_pretty
    println!("{{");
    println!("  \"quality_assessment\": {{");
    println!("    \"score\": {:.3},", result.quality_assessment.score);
    println!("    \"grade\": \"{}\",", result.quality_assessment.grade);
    println!("    \"issues_count\": {}", result.quality_assessment.issues.len());
    println!("  }},");
    
    if let Some(ref clip) = result.clipping_result {
        println!("  \"clipping\": {{");
        println!("    \"has_clipping\": {},", clip.has_clipping);
        println!("    \"severity\": {:.4},", clip.severity);
        println!("    \"clipped_samples\": {},", clip.statistics.samples_at_digital_max);
        println!("    \"true_peak_db\": {:.2},", clip.inter_sample_analysis.true_peak_db);
        println!("    \"dynamic_range_db\": {:.1},", clip.loudness_analysis.dynamic_range_db);
        println!("    \"loudness_war_victim\": {}", clip.loudness_analysis.loudness_war_victim);
        println!("  }},");
    }
    
    if let Some(ref auth) = result.authenticity_assessment {
        println!("  \"authenticity\": {{");
        println!("    \"result\": \"{}\",", auth.result);
        println!("    \"confidence\": {:.3},", auth.confidence);
        println!("    \"anomaly_count\": {}", auth.anomalies.len());
        println!("  }}");
    }
    
    println!("}}");
}

/// Print detailed report format
pub fn print_detailed_report(result: &ExtendedAnalysisResult) {
    println!("================================================================================");
    println!("                     AUDIOCHECKR EXTENDED ANALYSIS REPORT                       ");
    println!("================================================================================");
    println!();
    
    // Call text report for now, but this could be expanded
    print_text_report(result);
    
    // Additional technical details for report format
    if let Some(ref enf) = result.enf_result {
        println!("================================================================================");
        println!("                         ENF TECHNICAL DETAILS                                 ");
        println!("================================================================================");
        println!();
        println!("ENF Detected: {}", enf.enf_detected);
        println!("Base Frequency: {:?}", enf.base_frequency);
        println!("SNR: {:.2} dB", enf.enf_snr_db);
        println!("Stability Score: {:.4}", enf.stability_score);
        println!("Confidence: {:.4}", enf.confidence);
        println!();
        
        if !enf.harmonics.is_empty() {
            println!("Harmonics Detected:");
            for harmonic in &enf.harmonics {
                println!("  - {:.1} Hz: {:.2} dB (SNR: {:.1} dB)",
                    harmonic.frequency_hz, harmonic.amplitude_db, harmonic.snr_db);
            }
            println!();
        }
        
        println!("Frequency Trace: {} measurements", enf.frequency_trace.len());
        if let Some(first) = enf.frequency_trace.first() {
            if let Some(last) = enf.frequency_trace.last() {
                println!("  Time span: {:.1}s - {:.1}s", first.time_secs, last.time_secs);
                println!("  Frequency range: {:.4} Hz - {:.4} Hz",
                    enf.frequency_trace.iter().map(|m| m.frequency_hz).fold(f32::INFINITY, f32::min),
                    enf.frequency_trace.iter().map(|m| m.frequency_hz).fold(f32::NEG_INFINITY, f32::max));
            }
        }
    }
    
    println!();
    println!("================================================================================");
    println!("                              END OF REPORT                                     ");
    println!("================================================================================");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_args() {
        let args = ExtendedDetectionArgs::default();
        assert!(!args.enf);
        assert!(!args.no_clipping);
        assert!(!args.clipping_strict);
    }
}
