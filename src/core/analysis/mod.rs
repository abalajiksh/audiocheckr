//! Analysis types and result structures

pub mod clipping_detection;
pub mod detection_pipeline_enf_clipping;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for audio analysis
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub fft_size: usize,
    pub hop_size: usize,
    pub min_confidence: f64,
    pub enable_mqa: bool,
    pub enable_clipping: bool,
    pub enable_enf: bool,
    pub genre_profile: Option<String>,
    pub sensitivity: AnalysisSensitivity,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            fft_size: 8192,
            hop_size: 2048,
            min_confidence: 0.5,
            enable_mqa: false,
            enable_clipping: false,
            enable_enf: false,
            genre_profile: None,
            sensitivity: AnalysisSensitivity::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisSensitivity {
    Low,
    Medium,
    High,
}

/// Complete analysis result for an audio file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub file_path: PathBuf,
    pub file_hash: String,
    pub sample_rate: u32,
    pub bit_depth: u16,
    pub channels: u16,
    pub duration: f64,
    pub detections: Vec<Detection>,
    pub confidence: f64,
    pub quality_metrics: Option<QualityMetrics>,
    pub analysis_timestamp: String,
}

impl AnalysisResult {
    /// Returns true if the file appears to be genuine lossless
    pub fn is_genuine(&self) -> bool {
        self.detections.is_empty() || 
        self.detections.iter().all(|d| d.severity == Severity::Info || d.severity == Severity::Low)
    }
}

/// A single detection/finding from the analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub defect_type: DefectType,
    pub confidence: f64,
    pub severity: Severity,
    pub method: DetectionMethod,
    pub evidence: Option<String>,
    pub temporal: Option<TemporalDistribution>,
}

/// Types of defects that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DefectType {
    LossyTranscode {
        codec: String,
        estimated_bitrate: Option<u32>,
        cutoff_hz: u32,
    },
    Upsampled {
        original_rate: u32,
        current_rate: u32,
    },
    BitDepthInflated {
        actual_bits: u16,
        claimed_bits: u16,
    },
    Clipping {
        peak_level: f64,
        clipped_samples: u64,
    },
    SilencePadding {
        padding_duration: f64,
    },
    MqaEncoded {
        original_rate: Option<u32>,
        mqa_type: String,
        lsb_entropy: f64,
    },
    UpsampledLossyTranscode {
        original_rate: u32,
        current_rate: u32,
        codec: String,
        estimated_bitrate: Option<u32>,
        cutoff_hz: u32,
    },
}

/// Severity levels for detections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Detection method used to identify a defect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionMethod {
    SpectralCutoff,
    SpectralShape,
    BitDepthAnalysis,
    NullTest,
    PhaseAnalysis,
    TemporalAnalysis,
    MqaSignature,
    EnfAnalysis,
    ClippingAnalysis,
    StatisticalAnalysis,
    MultiMethod,
}

/// Quality metrics for the audio file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub dynamic_range: f64,
    pub noise_floor: f64,
    pub spectral_centroid: f64,
    pub crest_factor: f64,
    pub true_peak: f64,
    pub lufs_integrated: f64,
}

impl Default for QualityMetrics {
    fn default() -> Self {
        Self {
            dynamic_range: 0.0,
            noise_floor: -96.0,
            spectral_centroid: 0.0,
            crest_factor: 0.0,
            true_peak: 0.0,
            lufs_integrated: -23.0,
        }
    }
}

/// Quality score enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityScore {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
}

/// Temporal distribution of a detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDistribution {
    pub start_time: f64,
    pub end_time: f64,
    pub peak_time: f64,
    pub distribution: Vec<f64>,
}
