//! Command-line argument parsing

use clap::Parser;
use std::path::PathBuf;

/// AudioCheckr - Advanced audio quality analysis tool
#[derive(Parser, Debug, Clone)]
#[command(name = "audiocheckr")]
#[command(author = "Ashwin")]
#[command(version = "0.12.0")]
#[command(about = "Detects fake lossless audio files", long_about = None)]
pub struct Args {
    /// Input file or directory to analyze
    #[arg(required = true)]
    pub input: PathBuf,

    /// Output format (text, json, or detailed)
    #[arg(short, long, default_value = "text")]
    pub format: OutputFormat,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Process directories recursively
    #[arg(short, long)]
    pub recursive: bool,

    /// Number of parallel threads (0 = auto)
    #[arg(short = 'j', long, default_value = "0")]
    pub threads: usize,

    /// Detection sensitivity (low, medium, high)
    #[arg(short, long, default_value = "medium")]
    pub sensitivity: Sensitivity,

    /// Enable MQA detection
    #[arg(long)]
    pub mqa: bool,

    /// Enable clipping detection
    #[arg(long)]
    pub clipping: bool,

    /// Enable ENF (Electrical Network Frequency) analysis
    #[arg(long)]
    pub enf: bool,

    /// Show spectrogram visualization
    #[arg(long)]
    pub spectrogram: bool,

    /// Export detailed report to file
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// Minimum confidence threshold (0.0-1.0)
    #[arg(long, default_value = "0.5")]
    pub min_confidence: f64,

    /// Genre profile for detection tuning
    #[arg(long)]
    pub genre: Option<GenreProfile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Sensitivity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GenreProfile {
    Electronic,
    Classical,
    Rock,
    Jazz,
    Noise,
    Ambient,
    Pop,
    HipHop,
    Metal,
    Acoustic,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            format: OutputFormat::Text,
            verbose: false,
            recursive: false,
            threads: 0,
            sensitivity: Sensitivity::Medium,
            mqa: false,
            clipping: false,
            enf: false,
            spectrogram: false,
            report: None,
            min_confidence: 0.5,
            genre: None,
        }
    }
}
