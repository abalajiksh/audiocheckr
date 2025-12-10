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

mod bit_depth;
mod spectral;
mod upsampling;
mod stereo;
mod transients;
mod phase;
mod true_peak;
mod mfcc;

// Re-export all analysis modules
pub use bit_depth::{analyze_bit_depth, BitDepthAnalysis, BitDepthMethodResults};
pub use spectral::{
    SpectralAnalyzer, SpectralAnalysis, SpectralSignature,
    get_encoder_signatures, match_signature,
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
