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
//! ## Quick Start
//!
//! ```rust,ignore
//! use audiocheckr::config::{ProfileConfig, ProfilePreset};
//! use audiocheckr::detection::{AnalysisResult, RawDetection};
//!
//! // Use a preset profile
//! let profile = ProfileConfig::from_preset(ProfilePreset::Electronic);
//!
//! // Create analysis result
//! let mut result = AnalysisResult::new("track.flac", &profile);
//!
//! // Add detections (these would come from actual analysis)
//! let detections = vec![
//!     RawDetection::new(DetectorType::SpectralCutoff, 0.7, "Cutoff at 18kHz"),
//! ];
//! result.add_detections(detections, &profile);
//!
//! println!("Verdict: {:?}", result.verdict);
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

pub mod cli;
pub mod config;
pub mod detection;

// Re-export commonly used types
pub use config::{DetectorType, ProfileConfig, ProfilePreset};
pub use detection::{AnalysisResult, AnalysisVerdict, Finding, RawDetection};
