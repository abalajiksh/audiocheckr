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
use core::analysis::{AnalysisConfig, AnalysisResult, AnalysisSensitivity};
use core::detector::AudioDetector;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args = Args::parse();

    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .context("Failed to configure thread pool")?;
    }

    let files = collect_files(&args.input, args.recursive)?;

    if files.is_empty() {
        eprintln!("No audio files found to analyze");
        return Ok(());
    }

    let config = AnalysisConfig {
        fft_size: 8192,
        hop_size: 2048,
        min_confidence: args.min_confidence,
        enable_mqa: args.mqa,
        enable_clipping: args.clipping,
        enable_enf: args.enf,
        enable_mfcc: args.mfcc,
        genre_profile: args.genre.map(|g| format!("{:?}", g)),
        sensitivity: match args.sensitivity {
            Sensitivity::Low => AnalysisSensitivity::Low,
            Sensitivity::Medium => AnalysisSensitivity::Medium,
            Sensitivity::High => AnalysisSensitivity::High,
        },
    };

    // Progress bar (hidden in JSON-only mode to keep stdout clean)
    let show_progress = !matches!(args.format, OutputFormat::Json);
    let progress = ProgressBar::new(files.len() as u64);
    if show_progress {
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );
    } else {
        progress.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }

    let results: Vec<Result<AnalysisResult>> = files
        .par_iter()
        .map(|file| {
            let detector = AudioDetector::new(config.clone());
            let result = detector.analyze(file);
            progress.inc(1);
            result
        })
        .collect();

    progress.finish_and_clear();

    // For "detailed" mode, force verbose on the handler
    let verbose = args.verbose || matches!(args.format, OutputFormat::Detailed);
    let output_handler = OutputHandler::new(verbose);

    let mut success_count = 0;
    let mut genuine_count = 0;
    let mut suspect_count = 0;
    let mut error_count = 0;

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
                    OutputFormat::Both => {
                        output_handler.print_both(&analysis)?;
                    }
                }
            }
            Err(e) => {
                error_count += 1;
                eprintln!("Error: {}", e);
            }
        }
    }

    // Summary
    match args.format {
        // Skip summary for single-file JSON (already self-contained)
        OutputFormat::Json if success_count <= 1 => {}
        // In "both" mode, summary goes to stderr to keep stdout as pure JSON
        OutputFormat::Both => {
            output_handler.print_summary_stderr(
                success_count,
                genuine_count,
                suspect_count,
                error_count,
            );
        }
        _ => {
            output_handler.print_summary(success_count, genuine_count, suspect_count, error_count);
        }
    }

    if let Some(report_path) = args.report {
        export_report(&report_path)?;
    }

    Ok(())
}

/// Collect audio files from path
fn collect_files(path: &PathBuf, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let supported_extensions = [
        "flac", "wav", "aiff", "aif", "alac", "m4a", "ape", "wv", "dsf", "dff",
    ];

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

fn export_report(path: &PathBuf) -> Result<()> {
    eprintln!("Report export to {} — not yet implemented", path.display());
    Ok(())
}
