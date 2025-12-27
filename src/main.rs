//! AudioCheckr - Advanced Audio Quality Analysis Tool
//!
//! Detects fake lossless audio files through spectral analysis, bit depth
//! verification, MQA detection, and other sophisticated techniques.

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::PathBuf;
use walkdir::WalkDir;

mod cli;
mod core;

use cli::args::{Args, OutputFormat, Sensitivity};
use cli::output::OutputHandler;
use core::analysis::{AnalysisConfig, AnalysisResult, AnalysisSensitivity, DefectType};
use core::detector::AudioDetector;

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args = Args::parse();

    // Set up thread pool
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .context("Failed to configure thread pool")?;
    }

    // Collect files to analyze
    let files = collect_files(&args.input, args.recursive)?;

    if files.is_empty() {
        eprintln!("No audio files found to analyze");
        return Ok(());
    }

    // Create analysis config
    let config = AnalysisConfig {
        fft_size: 8192,
        hop_size: 2048,
        min_confidence: args.min_confidence,
        enable_mqa: args.mqa,
        enable_clipping: args.clipping,
        enable_enf: args.enf,
        genre_profile: args.genre.map(|g| format!("{:?}", g)),
        sensitivity: match args.sensitivity {
            Sensitivity::Low => AnalysisSensitivity::Low,
            Sensitivity::Medium => AnalysisSensitivity::Medium,
            Sensitivity::High => AnalysisSensitivity::High,
        },
    };

    // Set up progress bar
    let progress = ProgressBar::new(files.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Analyze files
    let results: Vec<Result<AnalysisResult>> = files
        .par_iter()
        .map(|file| {
            let detector = AudioDetector::new(config.clone());
            let result = detector.analyze(file);
            progress.inc(1);
            result
        })
        .collect();

    progress.finish_with_message("Analysis complete");

    // Output results
    let output_handler = OutputHandler::new(args.verbose);
    let mut success_count = 0;
    let mut genuine_count = 0;
    let mut suspect_count = 0;

    for result in results {
        match result {
            Ok(analysis) => {
                success_count += 1;
                if analysis.is_genuine() {
                    genuine_count += 1;
                } else {
                    suspect_count += 1;
                }

                match args.format {
                    OutputFormat::Text | OutputFormat::Detailed => {
                        output_handler.print_result(&analysis)?;
                    }
                    OutputFormat::Json => {
                        output_handler.print_json(&analysis)?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error analyzing file: {}", e);
            }
        }
    }

    // Print summary
    println!("\n{}", "=".repeat(60));
    println!("SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Files analyzed: {}", success_count);
    println!("Genuine lossless: {}", genuine_count);
    println!("Potentially fake: {}", suspect_count);

    // Export report if requested
    if let Some(report_path) = args.report {
        export_report(&report_path)?;
    }

    Ok(())
}

/// Collect audio files from path
fn collect_files(path: &PathBuf, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let supported_extensions = ["flac", "wav", "aiff", "aif", "alac", "m4a", "ape", "wv", "dsf", "dff"];

    if path.is_file() {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                files.push(path.clone());
            }
        }
    } else if path.is_dir() {
        let walker = if recursive {
            WalkDir::new(path)
        } else {
            WalkDir::new(path).max_depth(1)
        };

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if entry_path.is_file() {
                if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                    if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                        files.push(entry_path.to_path_buf());
                    }
                }
            }
        }
    }

    Ok(files)
}

/// Export detailed report to file
fn export_report(path: &PathBuf) -> Result<()> {
    // Placeholder for report export functionality
    println!("Report export to {} - not yet implemented", path.display());
    Ok(())
}

/// Format a defect type for display
fn format_defect(defect: &DefectType, confidence: f64) -> String {
    let conf_str = format!(" [{:.0}%]", confidence * 100.0);

    match defect {
        DefectType::LossyTranscode {
            codec,
            estimated_bitrate,
            cutoff_hz,
        } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" ~{}kbps", b))
                .unwrap_or_default();
            format!(
                "Lossy transcode: {}{} (cutoff: {} Hz){}",
                codec, bitrate_str, cutoff_hz, conf_str
            )
        }
        DefectType::Upsampled {
            original_rate,
            current_rate,
        } => {
            format!(
                "Upsampled: {} Hz → {} Hz{}",
                original_rate, current_rate, conf_str
            )
        }
        DefectType::BitDepthInflated {
            actual_bits,
            claimed_bits,
        } => {
            format!(
                "Bit depth inflation: {} bits claimed, {} bits actual{}",
                claimed_bits, actual_bits, conf_str
            )
        }
        DefectType::Clipping {
            peak_level,
            clipped_samples,
        } => {
            format!(
                "Clipping: peak {:.2} dB, {} samples{}",
                peak_level, clipped_samples, conf_str
            )
        }
        DefectType::SilencePadding { padding_duration } => {
            format!("Silence padding: {:.2} seconds{}", padding_duration, conf_str)
        }
        DefectType::MqaEncoded {
            original_rate,
            mqa_type,
            lsb_entropy,
        } => {
            let orig_str = original_rate
                .map(|r| format!(" (original: {} Hz)", r))
                .unwrap_or_default();
            format!(
                "MQA encoded: {}{} - LSB entropy: {:.2}{}",
                mqa_type, orig_str, lsb_entropy, conf_str
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
                "Upsampled lossy transcode: {}{} upsampled {} Hz → {} Hz (cutoff: {} Hz){}",
                codec, bitrate_str, original_rate, current_rate, cutoff_hz, conf_str
            )
        }
    }
}

/// Get severity indicator icon
fn get_severity_icon(defect: &DefectType) -> &'static str {
    match defect {
        DefectType::LossyTranscode { .. } => "[!!!]",
        DefectType::Upsampled { .. } => "[!!]",
        DefectType::BitDepthInflated { .. } => "[!!]",
        DefectType::Clipping { .. } => "[!]",
        DefectType::SilencePadding { .. } => "[.]",
        DefectType::MqaEncoded { .. } => "[i]",
        DefectType::UpsampledLossyTranscode { .. } => "[!!!]",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_defect() {
        let defect = DefectType::LossyTranscode {
            codec: "MP3".to_string(),
            estimated_bitrate: Some(128),
            cutoff_hz: 16000,
        };

        let formatted = format_defect(&defect, 0.95);
        assert!(formatted.contains("MP3"));
        assert!(formatted.contains("128kbps"));
        assert!(formatted.contains("16000"));
    }

    #[test]
    fn test_severity_icon() {
        let defect = DefectType::LossyTranscode {
            codec: "MP3".to_string(),
            estimated_bitrate: Some(128),
            cutoff_hz: 16000,
        };

        assert_eq!(get_severity_icon(&defect), "[!!!]");
    }
}
