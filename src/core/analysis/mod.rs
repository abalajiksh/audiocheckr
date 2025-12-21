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

pub use detection_pipeline::{DetectionContext, DetectionPipeline, DetectionResult};

// Re-export key types from each detection module
pub use dither_detection::{
    DitherDetector, DitherDetectionResult, DitherType, DitherCharacteristics,
};
pub use resample_detection::{
    ResampleDetector, ResampleDetectionResult, ResamplerType, ResamplerQuality,
};
pub use mqa_detection::{
    MqaDetector, MqaDetectionResult, MqaAuthenticationStatus, MqaStudioProvenance,
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
// NOTE: ClippingSeverity does NOT exist - use ClippingCause instead
pub use clipping_detection::{
    ClippingDetector,
    ClippingAnalysisResult,
    ClippingType,
    ClippingCause,           // NOT ClippingSeverity
    TemporalDistribution,
    LikelyCause,
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

// Re-export internal analysis utilities
pub(crate) use bit_depth::BitDepthAnalyzer;
pub(crate) use spectral::SpectralAnalyzer;
pub(crate) use upsampling::UpsamplingDetector;
pub(crate) use stereo::StereoAnalyzer;
pub(crate) use transients::TransientAnalyzer;
pub(crate) use phase::PhaseAnalyzer;
pub(crate) use true_peak::TruePeakMeter;
pub(crate) use mfcc::MfccAnalyzer;
