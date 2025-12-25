// src/cli/output.rs
//
// CLI output formatting for AudioCheckr

use colorful::Colorful;
use crate::core::{QualityReport, DetectedDefect, DefectType};

/// Format quality report for terminal output
pub fn format_report(report: &QualityReport, verbose: bool) -> String {
    let mut output = String::new();
    
    // Basic info
    output.push_str(&format!("  Sample Rate: {} Hz\n", report.sample_rate));
    output.push_str(&format!("  Bit Depth: {} bit (claimed: {})\n", 
        report.actual_bit_depth, report.claimed_bit_depth));
    output.push_str(&format!("  Channels: {}\n", report.channels));
    output.push_str(&format!("  Duration: {:.2}s\n", report.duration_secs));
    output.push_str(&format!("  Frequency Cutoff: {:.0} Hz\n", report.frequency_cutoff));
    output.push_str(&format!("  Dynamic Range: {:.1} dB\n", report.dynamic_range));

    // Quality score
    let score_str = format!("{:.0}%", report.quality_score * 100.0);
    if report.is_likely_lossless {
        output.push_str(&format!("  Quality Score: {} (likely lossless)\n", score_str));
    } else {
        output.push_str(&format!("  Quality Score: {} (likely transcoded)\n", score_str));
    }

    // Defects
    if report.defects.is_empty() {
        output.push_str("  Status: ✓ CLEAN - No issues detected\n");
    } else {
        output.push_str("  Status: ✗ ISSUES DETECTED\n");
        for defect in &report.defects {
            let defect_str = format_defect(defect);
            output.push_str(&format!("    • {}\n", defect_str));
            
            if verbose {
                for evidence in &defect.evidence {
                    output.push_str(&format!("      - {}\n", evidence));
                }
            }
        }
    }

    // Verbose details
    if verbose {
        output.push_str(&format!("\n  Technical Details:\n"));
        output.push_str(&format!("    Spectral rolloff: {:.0} Hz\n", report.spectral_rolloff));
        output.push_str(&format!("    Rolloff steepness: {:.1} dB/octave\n", report.rolloff_steepness));
        output.push_str(&format!("    Brick-wall cutoff: {}\n", if report.has_brick_wall { "Yes" } else { "No" }));
        output.push_str(&format!("    Spectral flatness: {:.3}\n", report.spectral_flatness));
        output.push_str(&format!("    Peak amplitude: {:.1} dBFS\n", report.peak_amplitude));
        output.push_str(&format!("    True peak: {:.1} dBFS\n", report.true_peak));
        output.push_str(&format!("    Crest factor: {:.1} dB\n", report.crest_factor));
        
        if let Some(width) = report.stereo_width {
            output.push_str(&format!("    Stereo width: {:.1}%\n", width * 100.0));
        }
        if let Some(corr) = report.channel_correlation {
            output.push_str(&format!("    Channel correlation: {:.3}\n", corr));
        }
        
        // Bit depth analysis details
        output.push_str(&format!("\n  Bit Depth Analysis:\n"));
        output.push_str(&format!("    LSB method: {} bit\n", report.bit_depth_analysis.method_results.lsb_method));
        output.push_str(&format!("    Histogram method: {} bit\n", report.bit_depth_analysis.method_results.histogram_method));
        output.push_str(&format!("    Noise floor method: {} bit\n", report.bit_depth_analysis.method_results.noise_method));
        output.push_str(&format!("    Clustering method: {} bit\n", report.bit_depth_analysis.method_results.clustering_method));
        output.push_str(&format!("    Overall confidence: {:.1}%\n", report.bit_depth_analysis.confidence * 100.0));
        
        // Pre-echo analysis
        if report.pre_echo_analysis.transient_count > 0 {
            output.push_str(&format!("\n  Pre-Echo Analysis:\n"));
            output.push_str(&format!("    Transients analyzed: {}\n", report.pre_echo_analysis.transient_count));
            output.push_str(&format!("    Pre-echo detected: {}\n", report.pre_echo_analysis.pre_echo_count));
            output.push_str(&format!("    Pre-echo score: {:.2}\n", report.pre_echo_analysis.pre_echo_score));
        }
        
        // Upsampling analysis
        if report.upsampling_analysis.is_upsampled {
            output.push_str(&format!("\n  Upsampling Analysis:\n"));
            if let Some(orig) = report.upsampling_analysis.original_sample_rate {
                output.push_str(&format!("    Original rate: {} Hz\n", orig));
            }
            output.push_str(&format!("    Detection method: {:?}\n", report.upsampling_analysis.detection_method));
            output.push_str(&format!("    Confidence: {:.1}%\n", report.upsampling_analysis.confidence * 100.0));
        }
    }
    
    output
}

/// Format a single defect for display
pub fn format_defect(defect: &DetectedDefect) -> String {
    let conf_str = format!(" ({:.0}% confidence)", defect.confidence * 100.0);
    
    match &defect.defect_type {
        DefectType::Mp3Transcode { cutoff_hz, estimated_bitrate } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" (~{}kbps)", b))
                .unwrap_or_default();
            format!("MP3 transcode{} - cutoff at {} Hz{}", bitrate_str, cutoff_hz, conf_str)
        },
        DefectType::OggVorbisTranscode { cutoff_hz, estimated_quality } => {
            let quality_str = estimated_quality
                .map(|q| format!(" (~q{:.1})", q))
                .unwrap_or_default();
            format!("Ogg Vorbis transcode{} - cutoff at {} Hz{}", quality_str, cutoff_hz, conf_str)
        },
        DefectType::AacTranscode { cutoff_hz, estimated_bitrate } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" (~{}kbps)", b))
                .unwrap_or_default();
            format!("AAC transcode{} - cutoff at {} Hz{}", bitrate_str, cutoff_hz, conf_str)
        },
        DefectType::OpusTranscode { cutoff_hz, mode } => {
            format!("Opus transcode ({}) - cutoff at {} Hz{}", mode, cutoff_hz, conf_str)
        },
        DefectType::BitDepthMismatch { claimed, actual, .. } => {
            format!("Bit depth mismatch: claims {}-bit, actually {}-bit{}", claimed, actual, conf_str)
        },
        DefectType::Upsampled { from, to, .. } => {
            format!("Upsampled from {} Hz to {} Hz{}", from, to, conf_str)
        },
        DefectType::SpectralArtifacts { artifact_score } => {
            format!("Spectral artifacts detected (score: {:.2}){}", artifact_score, conf_str)
        },
        DefectType::JointStereo { correlation } => {
            format!("Joint stereo encoding detected (correlation: {:.2}){}", correlation, conf_str)
        },
        DefectType::PreEcho { score } => {
            format!("Pre-echo artifacts (score: {:.2}){}", score, conf_str)
        },
        DefectType::PhaseDiscontinuities { score } => {
            format!("Phase discontinuities (score: {:.2}){}", score, conf_str)
        },
        DefectType::Clipping { percentage } => {
            format!("Clipping detected ({:.3}% of samples){}", percentage * 100.0, conf_str)
        },
        DefectType::InterSampleOvers { count, max_level_db } => {
            format!("{} inter-sample overs (max: {:.1} dB){}", count, max_level_db, conf_str)
        },
        DefectType::LowQuality { description } => {
            format!("Low quality: {}{}", description, conf_str)
        },
        DefectType::DitheringDetected { algorithm, scale, effective_bits, container_bits } => {
            format!("Dithering detected: {} at {} scale ({}→{} bit){}", 
                    algorithm, scale, container_bits, effective_bits, conf_str)
        },
        DefectType::ResamplingDetected { engine, quality, original_rate, current_rate } => {
            format!(
                "Resampling detected: {} Hz → {} Hz using {} ({} quality){}",
                original_rate, current_rate, engine, quality, conf_str
            )
        },
        DefectType::MqaEncoded { original_rate, mqa_type, lsb_entropy } => {
            let orig_str = original_rate
                .map(|r| format!(" (original: {} Hz)", r))
                .unwrap_or_default();
            format!("MQA encoded: {}{} - LSB entropy: {:.2}{}", 
                    mqa_type, orig_str, lsb_entropy, conf_str)
        },
        DefectType::UpsampledLossyTranscode { 
            original_rate, 
            current_rate, 
            codec, 
            estimated_bitrate, 
            cutoff_hz 
        } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" ~{}kbps", b))
                .unwrap_or_default();
            format!(
                "Upsampled lossy transcode: {}{} upsampled {} Hz → {} Hz (cutoff: {} Hz){}",
                codec, bitrate_str, original_rate, current_rate, cutoff_hz, conf_str
            )
        },
    }
}

/// Format defect type as a simple string label
pub fn format_defect_type(defect: &DefectType) -> String {
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
        DefectType::DitheringDetected { .. } => "Dithering Detected".to_string(),
        DefectType::ResamplingDetected { .. } => "Resampling Detected".to_string(),
        DefectType::MqaEncoded { .. } => "MQA Encoded".to_string(),
        DefectType::UpsampledLossyTranscode { .. } => "Upsampled Lossy Transcode".to_string(),
    }
}

/// Get severity color for defect
pub fn defect_severity_color(defect: &DefectType) -> &'static str {
    match defect {
        // Critical - definitely not lossless
        DefectType::Mp3Transcode { .. } |
        DefectType::OggVorbisTranscode { .. } |
        DefectType::AacTranscode { .. } |
        DefectType::OpusTranscode { .. } |
        DefectType::UpsampledLossyTranscode { .. } => "red",
        
        // High - likely not genuine
        DefectType::BitDepthMismatch { .. } |
        DefectType::Upsampled { .. } |
        DefectType::MqaEncoded { .. } => "yellow",
        
        // Medium - quality issues
        DefectType::PreEcho { .. } |
        DefectType::SpectralArtifacts { .. } |
        DefectType::Clipping { .. } => "orange",
        
        // Low - informational
        DefectType::JointStereo { .. } |
        DefectType::PhaseDiscontinuities { .. } |
        DefectType::InterSampleOvers { .. } |
        DefectType::LowQuality { .. } |
        DefectType::DitheringDetected { .. } |
        DefectType::ResamplingDetected { .. } => "cyan",
    }
}
