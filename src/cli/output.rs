use crate::core::analysis::{AnalysisResult, DefectType, Severity};
use anyhow::Result;
use colorful::Colorful;
use serde_json::json;

pub struct OutputHandler {
    verbose: bool,
}

impl OutputHandler {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    pub fn print_result(&self, result: &AnalysisResult) -> Result<()> {
        let status = if result.is_genuine() {
            "[OK]".green()
        } else {
            "[FAIL]".red()
        };

        let filename = result.file_path.file_name().unwrap_or_default().to_string_lossy();
        println!("{} {}", status, filename);

        if self.verbose || !result.is_genuine() {
            println!("   Path: {}", result.file_path.display());
            println!("   Format: {} Hz / {} bit / {} ch", 
                result.sample_rate, result.bit_depth, result.channels);
            
            if let Some(metrics) = &result.quality_metrics {
                println!("   Dynamic Range: {:.1} dB", metrics.dynamic_range);
                println!("   Noise Floor: {:.1} dB", metrics.noise_floor);
            }

            if !result.detections.is_empty() {
                println!("   Detections:");
                for detection in &result.detections {
                    // Use RGB colors since colorful might not support string color names easily
                    let severity_color = match detection.severity {
                        Severity::Critical => colorful::Color::Red,
                        Severity::High => colorful::Color::Red,
                        Severity::Medium => colorful::Color::Yellow,
                        Severity::Low => colorful::Color::Yellow,
                        Severity::Info => colorful::Color::Blue,
                    };

                    let severity_str = format!("{:?}", detection.severity);
                    let colored_severity = severity_str.as_str().color(severity_color);
                    let confidence_str = format!("{:.0}%", detection.confidence * 100.0);
                    
                    print!("     - [{}] ({}) ", colored_severity, confidence_str);

                    match &detection.defect_type {
                        DefectType::LossyTranscode { codec, estimated_bitrate, cutoff_hz } => {
                            let bitrate = estimated_bitrate.map(|b| format!("~{}kbps", b)).unwrap_or_default();
                            println!("Lossy Transcode: {}{} (cutoff: {} Hz)", codec, bitrate, cutoff_hz);
                        },
                        DefectType::Upsampled { original_rate, current_rate } => {
                            println!("Upsampled: {} Hz -> {} Hz", original_rate, current_rate);
                        },
                        DefectType::BitDepthInflated { actual_bits, claimed_bits } => {
                            println!("Bit Depth Inflation: {} bits claimed, {} bits actual", claimed_bits, actual_bits);
                        },
                        DefectType::Clipping { peak_level, clipped_samples } => {
                            println!("Clipping: Peak {:.2} dB, {} clipped samples", peak_level, clipped_samples);
                        },
                        DefectType::SilencePadding { padding_duration } => {
                            println!("Silence Padding: {:.2}s", padding_duration);
                        },
                        DefectType::MqaEncoded { encoder_version, bit_depth, .. } => {
                            // Display only encoder version and bit depth as requested
                            println!("MQA Encoded: Version {} ({}-bit)", encoder_version, bit_depth);
                        },
                        DefectType::UpsampledLossyTranscode { codec, cutoff_hz, .. } => {
                            println!("Upsampled Lossy Transcode: {} (cutoff: {} Hz)", codec, cutoff_hz);
                        },
                        DefectType::DitheringDetected { dither_type, bit_depth, noise_shaping } => {
                            let shaping = if *noise_shaping { " (shaped)" } else { "" };
                            println!("Dithering: {} {}-bit{}", dither_type, bit_depth, shaping);
                        },
                        DefectType::ResamplingDetected { original_rate, target_rate, quality } => {
                            let orig = if *original_rate > 0 { format!("{} Hz -> ", original_rate) } else { String::new() };
                            println!("Resampling: {}{} Hz ({})", orig, target_rate, quality);
                        }
                    }

                    if let Some(evidence) = &detection.evidence {
                        println!("       Evidence: {}", evidence);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn print_json(&self, result: &AnalysisResult) -> Result<()> {
        let json = json!(result);
        println!("{}", serde_json::to_string(&json)?);
        Ok(())
    }
}
