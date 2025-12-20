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
pub use detection_pipeline::{DetectionContext, CodecConstraints, ArtifactDiscrimination};

// Re-export all analysis modules
pub use bit_depth::{analyze_bit_depth, BitDepthAnalysis, BitDepthMethodResults};
pub use spectral::{
    SpectralAnalyzer, SpectralAnalysis, CodecSignature, Codec,
    detect_transcode, TranscodeResult,
};
pub use upsampling::{
    analyze_upsampling, UpsamplingAnalysis, UpsamplingMethodResults,
    detect_upsampling_ratio,
};
pub use stereo::{analyze_stereo, StereoAnalysis};
pub use transients::{
    analyze_pre_echo, PreEchoAnalysis, TransientInfo,
    analyze_frame_boundaries, FrameBoundaryAnalysis,
};
pub use phase::{
    analyze_phase, PhaseAnalysis,
    analyze_instantaneous_frequency, InstantaneousFrequencyAnalysis,
};
pub use true_peak::{
    analyze_true_peak, TruePeakAnalysis, LoudnessInfo,
    analyze_true_peak_stereo, ChannelTruePeak,
};
pub use mfcc::{analyze_mfcc, MfccAnalysis, MfccParams};

// Legacy dither module
pub use dither::{DitherAnalyzer, DitherAnalysis, DitherType};

// New enhanced detection modules
pub use dither_detection::{
    DitherDetector, DitherDetectionResult, DitherAlgorithm, DitherScale,
    NoiseSpectrumProfile,
};
pub use resample_detection::{
    ResampleDetector, ResampleDetectionResult, ResamplerEngine,
    ResampleQuality, ResampleDirection,
};
