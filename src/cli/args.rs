// src/cli/args.rs
//
// CLI argument parsing

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "audiocheckr")]
#[command(version = "0.3.0")]
#[command(about = "Detect fake lossless, transcodes, and upsampled audio")]
pub struct Args {
    /// Input file or directory
    #[arg(short, long, default_value = ".")]
    pub input: PathBuf,

    /// Expected bit depth (16 or 24)
    #[arg(short, long, default_value = "24")]
    pub bit_depth: u32,

    /// Generate spectrogram images
    #[arg(short, long)]
    pub spectrogram: bool,

    /// Use linear frequency scale instead of mel scale
    #[arg(long)]
    pub linear_scale: bool,

    /// Generate full-length spectrogram
    #[arg(long)]
    pub full_spectrogram: bool,

    /// Output mode: "source", "current", or custom path
    #[arg(short, long, default_value = "source")]
    pub output: String,

    /// Check for upsampling
    #[arg(short = 'u', long)]
    pub check_upsampling: bool,

    /// Enable stereo analysis
    #[arg(long)]
    pub stereo: bool,

    /// Enable transient/pre-echo analysis
    #[arg(long)]
    pub transients: bool,

    /// Enable phase analysis (slower)
    #[arg(long)]
    pub phase: bool,

    /// Verbose output with detailed analysis
    #[arg(short, long)]
    pub verbose: bool,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,

     /// Enable dithering detection (24→16 bit reduction)
    #[arg(long)]
    pub dithering: bool,

    /// Enable resampling detection
    #[arg(long)]
    pub resampling: bool,

    /// Minimum confidence threshold (0.0 - 1.0)
    #[arg(long, default_value = "0.5")]
    pub min_confidence: f32,

    /// Quick mode - skip slower analyses
    #[arg(short, long)]
    pub quick: bool,

    /// Detection profile: standard, highres, electronic, noise, classical, podcast
    #[arg(long, default_value = "standard")]
    pub profile: String,

    /// Disable specific detectors (comma-separated)
    #[arg(long)]
    pub disable: Option<String>,

    /// Show suppressed findings
    #[arg(long)]
    pub show_suppressed: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}
