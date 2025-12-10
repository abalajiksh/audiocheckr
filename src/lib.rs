//! AudioCheckr - Detect fake lossless audio files
//!
//! A sophisticated audio analysis tool that detects fake lossless files,
//! transcoding artifacts, and quality issues using advanced DSP techniques.
//!
//! ## Features
//!
//! - **Genre-aware detection profiles**: Adjust sensitivity based on audio type
//! - **Multiple detectors**: Spectral analysis, pre-echo, bit depth, upsampling, etc.
//! - **Confidence scoring**: Profile-adjusted confidence with suppression for edge cases
//! - **Flexible CLI**: Profiles, individual overrides, and verbose debugging
//!
//! ## Module Structure
//!
//! - `core` - Audio analysis algorithms and DSP utilities
//! - `cli` - Command-line interface
//! - `config` - Detection profiles and configuration
//! - `detection` - Detection result types
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use audiocheckr::core::{AudioAnalyzer, DetectionConfig};
//! use audiocheckr::config::{ProfileConfig, ProfilePreset};
//!
//! // Use a preset profile
//! let profile = ProfileConfig::from_preset(ProfilePreset::Electronic);
//!
//! // Analyze a file
//! let analyzer = AudioAnalyzer::new(path)?;
//! let report = analyzer.analyze()?;
//!
//! println!("Quality score: {:.0}%", report.quality_score * 100.0);
//! ```
//!
//! ## Detection Profiles
//!
//! | Profile    | Use Case                          | Key Adjustments                    |
//! |------------|-----------------------------------|-----------------------------------|
//! | Standard   | General music                     | Balanced defaults                 |
//! | HighRes    | Verified hi-res sources           | Reduced cutoff sensitivity        |
//! | Electronic | EDM, synthwave                    | Tolerates sharp cutoffs           |
//! | Noise      | Ambient, drone, noise             | Full-spectrum tolerance           |
//! | Classical  | Orchestral, acoustic              | Strict dynamic range              |
//! | Podcast    | Speech, voice content             | Limited detectors enabled         |

// Core analysis functionality
pub mod core;

// Command-line interface
pub mod cli;

// Configuration and profiles
pub mod config;

// Detection result types
pub mod detection;

// Re-export commonly used types at crate root for convenience
pub use config::{DetectorType, ProfileConfig, ProfilePreset, ProfileBuilder, ConfidenceModifier};
pub use detection::{AnalysisResult, AnalysisVerdict, Finding, RawDetection, Severity};
pub use core::{AudioAnalyzer, AnalyzerBuilder, FileInfo, AudioData, QualityReport, DetectedDefect, DefectType, DetectionConfig};
