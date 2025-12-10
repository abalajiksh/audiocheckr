//! Core audio analysis functionality
//!
//! This module contains all the audio analysis algorithms, DSP utilities,
//! and visualization tools. It's the heart of AudioCheckr.

pub mod analysis;
pub mod dsp;
pub mod visualization;

mod decoder;
mod detector;
mod analyzer;

// Re-export main types for convenient access
pub use decoder::{AudioData, decode_audio, extract_mono, extract_stereo, compute_mid_side};
pub use detector::{
    DetectionConfig, QualityReport, DetectedDefect, DefectType,
    detect_quality_issues, detect_quality_issues_simple,
};
pub use analyzer::{AudioAnalyzer, AnalyzerBuilder, FileInfo};
