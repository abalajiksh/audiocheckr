//! Audio analysis algorithms
//!
//! Contains specialized detection algorithms for:
//! - Bit depth analysis (fake 24-bit detection)
//! - Spectral analysis (frequency cutoff, lossy codec detection)
//! - Upsampling detection
//! - Stereo field analysis (joint stereo detection)
//! - Transient/pre-echo analysis
//! - Phase analysis
//! - True peak measurement
//! - MFCC (codec fingerprinting)
//! - Dithering detection (rectangular, triangular, Shibata, etc.)
//! - Resampling detection (SWR, SoXR, quality levels)
//! - MQA detection (Master Quality Authenticated)
//! - ENF detection (Electrical Network Frequency analysis)
//! - Clipping detection (digital overs, inter-sample peaks, loudness war)

mod bit_depth;
mod spectral;
mod upsampling;
mod stereo;
mod transients;
mod phase;
mod true_peak;
mod mfcc;
pub mod dither;
pub mod dither_detection;
pub mod resample_detection;
pub mod detection_pipeline;
pub mod mqa_detection;
pub mod enf_detection;
pub mod clipping_detection;
pub mod detection_pipeline_enf_clipping;

// Add these public re-exports near the top of the file:
pub use bit_depth::{BitDepthAnalysis, analyze_bit_depth};
pub use upsampling::{UpsamplingAnalysis, analyze_upsampling};
pub use transients::{PreEchoAnalysis, analyze_pre_echo};
pub use stereo::{analyze_stereo, StereoAnalysis};
pub use spectral::{SpectralAnalysis, Codec, detect_transcode, TranscodeResult};
pub use dither_detection::{DitherAlgorithm, DitherScale, NoiseSpectrumProfile};
pub use resample_detection::{ResamplerEngine, ResampleQuality, ResampleDirection};


pub use detection_pipeline::DetectionContext;

// Re-export key types from each detection module
pub use dither_detection::{
    DitherDetector, DitherDetectionResult,
};
pub use resample_detection::{
    ResampleDetector, ResampleDetectionResult,
};
pub use mqa_detection::{
    MqaDetector, MqaDetectionResult, MqaType,
};
// Re-export ENF detection types
pub use enf_detection::{
    EnfDetector,
    EnfDetectionResult,
    EnfBaseFrequency,
    EnfRegion,
    EnfAnomaly,
    EnfAnomalyType,
    EnfHarmonic,
    EnfMeasurement,
};
// Re-export Clipping detection types
pub use clipping_detection::{
    ClippingDetector,
    ClippingAnalysisResult,
    ClippingType,
    ClippingCause,
    TemporalDistribution,
    RestorationAssessment,
    LoudnessAnalysis,
    InterSampleAnalysis,
    ClippingStatistics,
    ClippingEvent,
};

// Re-export Extended Detection Pipeline types
pub use detection_pipeline_enf_clipping::{
    ExtendedDetectionPipeline,
    ExtendedDetectionOptions,
    ExtendedAnalysisResult,
    QualityAssessment,
    QualityGrade,
    QualityIssue,
    QualityIssueType,
    AuthenticityAssessment,
    AuthenticityResult,
    AuthenticityAnomaly,
    // Convenience functions
    analyze_audio_quality,
    analyze_stereo_quality,
    analyze_authenticity,
};

// REMOVED - These types don't exist:
// - BitDepthAnalyzer, UpsamplingDetector, StereoAnalyzer
// - TransientAnalyzer, PhaseAnalyzer, TruePeakMeter, MfccAnalyzer
// - DetectionPipeline, DetectionResult
// - DitherType, DitherCharacteristics
// - ResamplerType, ResamplerQuality
// - MqaAuthenticationStatus, MqaStudioProvenance
// - LikelyCause
