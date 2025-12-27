//! Core analysis and detection modules

pub mod analysis;
pub mod detector;
pub mod dsp;
pub mod visualization;

pub use analysis::{AnalysisConfig, AnalysisResult, DetectionMethod};
pub use detector::AudioDetector;
pub use dsp::SpectralAnalyzer;
