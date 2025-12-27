//! Output formatting and display

use crate::core::analysis::{AnalysisResult, DefectType, Detection, Severity};
use std::io::{self, Write};

/// Handles output formatting for analysis results
pub struct OutputHandler {
    verbose: bool,
}

impl OutputHandler {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Print analysis result to stdout
    pub fn print_result(&self, result: &AnalysisResult) -> io::Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        writeln!(handle, "\n{}", "=".repeat(60))?;
        writeln!(handle, "File: {}", result.file_path.display())?;
        writeln!(handle, "{}", "=".repeat(60))?;

        // Print basic info
        writeln!(handle, "Sample Rate: {} Hz", result.sample_rate)?;
        writeln!(handle, "Bit Depth: {}", result.bit_depth)?;
        writeln!(handle, "Channels: {}", result.channels)?;
        writeln!(handle, "Duration: {:.2} seconds", result.duration)?;

        // Print overall verdict
        let verdict = if result.is_genuine() {
            "GENUINE LOSSLESS"
        } else {
            "POTENTIALLY FAKE"
        };
        writeln!(handle, "\nVerdict: {}", verdict)?;
        writeln!(handle, "Confidence: {:.1}%", result.confidence * 100.0)?;

        // Print detections
        if !result.detections.is_empty() {
            writeln!(handle, "\nDetections:")?;
            for detection in &result.detections {
                self.print_detection(&mut handle, detection)?;
            }
        }

        if self.verbose {
            if let Some(ref metrics) = result.quality_metrics {
                writeln!(handle, "\nQuality Metrics:")?;
                writeln!(handle, "  Dynamic Range: {:.2} dB", metrics.dynamic_range)?;
                writeln!(handle, "  Noise Floor: {:.2} dB", metrics.noise_floor)?;
                writeln!(handle, "  Spectral Centroid: {:.2} Hz", metrics.spectral_centroid)?;
            }
        }

        writeln!(handle)?;
        Ok(())
    }

    fn print_detection(&self, handle: &mut io::StdoutLock, detection: &Detection) -> io::Result<()> {
        let severity_icon = match detection.severity {
            Severity::Critical => "[!!!]",
            Severity::High => "[!!]",
            Severity::Medium => "[!]",
            Severity::Low => "[.]",
            Severity::Info => "[i]",
        };

        let description = self.format_defect(&detection.defect_type);
        writeln!(
            handle,
            "  {} {} (confidence: {:.1}%)",
            severity_icon,
            description,
            detection.confidence * 100.0
        )?;

        if self.verbose {
            writeln!(handle, "      Method: {:?}", detection.method)?;
            if let Some(ref evidence) = detection.evidence {
                writeln!(handle, "      Evidence: {}", evidence)?;
            }
        }

        Ok(())
    }

    fn format_defect(&self, defect: &DefectType) -> String {
        match defect {
            DefectType::LossyTranscode { codec, estimated_bitrate, cutoff_hz } => {
                let bitrate_str = estimated_bitrate
                    .map(|b| format!(" ~{}kbps", b))
                    .unwrap_or_default();
                format!(
                    "Lossy transcode detected: {}{} (cutoff: {} Hz)",
                    codec, bitrate_str, cutoff_hz
                )
            }
            DefectType::Upsampled { original_rate, current_rate } => {
                format!(
                    "Upsampled from {} Hz to {} Hz",
                    original_rate, current_rate
                )
            }
            DefectType::BitDepthInflated { actual_bits, claimed_bits } => {
                format!(
                    "Bit depth inflated: actual {} bits, claimed {} bits",
                    actual_bits, claimed_bits
                )
            }
            DefectType::Clipping { peak_level, clipped_samples } => {
                format!(
                    "Clipping detected: peak {:.2} dB, {} clipped samples",
                    peak_level, clipped_samples
                )
            }
            DefectType::SilencePadding { padding_duration } => {
                format!("Silence padding: {:.2} seconds", padding_duration)
            }
            DefectType::MqaEncoded { original_rate, mqa_type, lsb_entropy } => {
                let rate_str = original_rate
                    .map(|r| format!(" (original: {} Hz)", r))
                    .unwrap_or_default();
                format!(
                    "MQA encoded: {}{} - LSB entropy: {:.2}",
                    mqa_type, rate_str, lsb_entropy
                )
            }
            DefectType::UpsampledLossyTranscode {
                original_rate,
                current_rate,
                codec,
                estimated_bitrate,
                cutoff_hz,
            } => {
                let bitrate_str = estimated_bitrate
                    .map(|b| format!(" ~{}kbps", b))
                    .unwrap_or_default();
                format!(
                    "Upsampled lossy transcode: {}{} upsampled {} Hz → {} Hz (cutoff: {} Hz)",
                    codec, bitrate_str, original_rate, current_rate, cutoff_hz
                )
            }
        }
    }

    /// Output result as JSON
    pub fn print_json(&self, result: &AnalysisResult) -> io::Result<()> {
        let json = serde_json::to_string_pretty(result)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        println!("{}", json);
        Ok(())
    }
}
