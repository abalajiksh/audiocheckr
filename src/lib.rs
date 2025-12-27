//! AudioCheckr - Advanced Audio Quality Analysis Library
//!
//! This library provides comprehensive tools for detecting fake lossless audio files,
//! including transcode detection, upsampling detection, MQA analysis, and more.

pub mod cli;
pub mod core;

// Re-export commonly used types
pub use core::analysis::{
    AnalysisConfig, AnalysisResult, DefectType, Detection, DetectionMethod,
    QualityMetrics, QualityScore, Severity, TemporalDistribution,
};
pub use core::detector::AudioDetector;
pub use core::dsp::{SpectralAnalyzer, WindowFunction};
