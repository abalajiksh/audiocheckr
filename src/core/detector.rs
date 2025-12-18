// src/core/detector.rs
//
// Quality issue detection with configurable thresholds and profiles.
// Now includes enhanced dithering and resampling detection.

use serde::{Deserialize, Serialize};
use super::decoder::AudioData;
use super::analysis::{
    analyze_bit_depth, BitDepthAnalysis,
    analyze_upsampling, UpsamplingAnalysis,
    analyze_pre_echo, PreEchoAnalysis,
    analyze_stereo,
    SpectralAnalyzer, detect_transcode, Codec,
    DitherDetector, DitherDetectionResult, DitherAlgorithm, DitherScale,
    ResampleDetector, ResampleDetectionResult, ResamplerEngine, ResampleQuality,
};

/// Detection configuration
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    pub expected_bit_depth: u32,
    pub check_upsampling: bool,
    pub check_stereo: bool,
    pub check_transients: bool,
    pub check_phase: bool,
    pub check_mfcc: bool,
    pub check_dithering: bool,
    pub check_resampling: bool,
    pub min_confidence: f32,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            expected_bit_depth: 24,
            check_upsampling: true,
            check_stereo: true,
            check_transients: true,
            check_phase: false,
            check_mfcc: false,
            check_dithering: true,
            check_resampling: true,
            min_confidence: 0.5,
        }
    }
}

/// Types of quality defects that can be detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefectType {
    Mp3Transcode { cutoff_hz: u32, estimated_bitrate: Option<u32> },
    OggVorbisTranscode { cutoff_hz: u32, estimated_quality: Option<f32> },
    AacTranscode { cutoff_hz: u32, estimated_bitrate: Option<u32> },
    OpusTranscode { cutoff_hz: u32, mode: String },
    BitDepthMismatch { claimed: u32, actual: u32, method: String },
    Upsampled { from: u32, to: u32, method: String },
    SpectralArtifacts { artifact_score: f32 },
    JointStereo { correlation: f32 },
    PreEcho { score: f32 },
    PhaseDiscontinuities { score: f32 },
    Clipping { percentage: f32 },
    InterSampleOvers { count: u32, max_level_db: f32 },
    LowQuality { description: String },
    
    // New enhanced defect types
    DitheringDetected {
        algorithm: String,
        scale: String,
        effective_bits: u8,
        container_bits: u8,
    },
    ResamplingDetected {
        original_rate: u32,
        current_rate: u32,
        engine: String,
        quality: String,
    },
}

/// A detected quality defect with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedDefect {
    pub defect_type: DefectType,
    pub confidence: f32,
    pub evidence: Vec<String>,
}

/// Complete quality analysis report
#[derive(Debug, Clone)]
pub struct QualityReport {
    // File info
    pub sample_rate: u32,
    pub channels: usize,
    pub claimed_bit_depth: u32,
    pub actual_bit_depth: u32,
    pub duration_secs: f64,
    
    // Spectral analysis
    pub frequency_cutoff: f32,
    pub spectral_rolloff: f32,
    pub rolloff_steepness: f32,
    pub has_brick_wall: bool,
    pub spectral_flatness: f32,
    
    // Dynamics
    pub dynamic_range: f32,
    pub peak_amplitude: f32,
    pub true_peak: f32,
    pub crest_factor: f32,
    
    // Stereo
    pub stereo_width: Option<f32>,
    pub channel_correlation: Option<f32>,
    
    // Overall assessment
    pub quality_score: f32,
    pub is_likely_lossless: bool,
    pub defects: Vec<DetectedDefect>,
    
    // Detailed analysis results
    pub bit_depth_analysis: BitDepthAnalysis,
    pub upsampling_analysis: UpsamplingAnalysis,
    pub pre_echo_analysis: PreEchoAnalysis,
    
    // New enhanced analysis results
    pub dither_analysis: Option<DitherDetectionResult>,
    pub resample_analysis: Option<ResampleDetectionResult>,
}

/// Run quality detection on decoded audio
pub fn detect_quality_issues(audio: &AudioData, config: &DetectionConfig) -> QualityReport {
    let mono = super::decoder::extract_mono(audio);
    
    // =========================================================================
    // SPECTRAL ANALYSIS - Detect lossy codec transcodes
    // =========================================================================
    let spectral_analyzer = SpectralAnalyzer::default();
    let spectral_analysis = spectral_analyzer.analyze(&mono, audio.sample_rate);
    let transcode_result = detect_transcode(&mono, audio.sample_rate);
    
    // =========================================================================
    // BIT DEPTH ANALYSIS
    // =========================================================================
    let bit_depth_analysis = analyze_bit_depth(audio);
    
    // =========================================================================
    // UPSAMPLING ANALYSIS (basic)
    // =========================================================================
    let upsampling_analysis = if config.check_upsampling {
        analyze_upsampling(&mono, audio.sample_rate)
    } else {
        UpsamplingAnalysis::default()
    };
    
    // =========================================================================
    // PRE-ECHO ANALYSIS
    // =========================================================================
    let pre_echo_analysis = if config.check_transients {
        analyze_pre_echo(&mono, audio.sample_rate)
    } else {
        PreEchoAnalysis::default()
    };
    
    // =========================================================================
    // STEREO ANALYSIS
    // =========================================================================
    let stereo_analysis = if config.check_stereo && audio.channels >= 2 {
        if let Some((left, right)) = super::decoder::extract_stereo(audio) {
            Some(analyze_stereo(&left, &right, audio.sample_rate))
        } else {
            None
        }
    } else {
        None
    };
    
    // =========================================================================
    // ENHANCED DITHERING DETECTION
    // =========================================================================
    let dither_analysis = if config.check_dithering {
        let detector = DitherDetector::new(audio.sample_rate);
        Some(detector.analyze(&mono, audio.claimed_bit_depth as u8))
    } else {
        None
    };
    
    // =========================================================================
    // ENHANCED RESAMPLING DETECTION
    // =========================================================================
    let resample_analysis = if config.check_resampling {
        let detector = ResampleDetector::new();
        Some(detector.analyze(&mono, audio.sample_rate))
    } else {
        None
    };
    
    // =========================================================================
    // COLLECT DEFECTS
    // =========================================================================
    let mut defects = Vec::new();
    
    // ----- Check for lossy transcode -----
    if transcode_result.is_transcode && transcode_result.confidence > config.min_confidence {
        let mut evidence = vec![transcode_result.reason.clone()];
        
        if let Some(ref codec_sig) = transcode_result.likely_codec {
            evidence.push(format!(
                "Matches {:?} signature at {}kbps (confidence: {:.0}%)",
                codec_sig.codec,
                codec_sig.bitrate.unwrap_or(0),
                codec_sig.confidence * 100.0
            ));
        }
        
        let defect_type = match transcode_result.likely_codec {
            Some(ref sig) => match sig.codec {
                Codec::MP3 => DefectType::Mp3Transcode {
                    cutoff_hz: transcode_result.cutoff_hz as u32,
                    estimated_bitrate: sig.bitrate,
                },
                Codec::AAC => DefectType::AacTranscode {
                    cutoff_hz: transcode_result.cutoff_hz as u32,
                    estimated_bitrate: sig.bitrate,
                },
                Codec::Opus => DefectType::OpusTranscode {
                    cutoff_hz: transcode_result.cutoff_hz as u32,
                    mode: format!("{}kbps", sig.bitrate.unwrap_or(0)),
                },
                Codec::Vorbis => DefectType::OggVorbisTranscode {
                    cutoff_hz: transcode_result.cutoff_hz as u32,
                    estimated_quality: sig.bitrate.map(|b| b as f32 / 32.0),
                },
                Codec::Unknown => DefectType::Mp3Transcode {
                    cutoff_hz: transcode_result.cutoff_hz as u32,
                    estimated_bitrate: estimate_bitrate_from_cutoff(transcode_result.cutoff_hz),
                },
            },
            None => DefectType::Mp3Transcode {
                cutoff_hz: transcode_result.cutoff_hz as u32,
                estimated_bitrate: estimate_bitrate_from_cutoff(transcode_result.cutoff_hz),
            },
        };
        
        defects.push(DetectedDefect {
            defect_type,
            confidence: transcode_result.confidence,
            evidence,
        });
    }
    
    // ----- Check bit depth mismatch -----
    if bit_depth_analysis.is_mismatch && bit_depth_analysis.confidence > config.min_confidence {
        defects.push(DetectedDefect {
            defect_type: DefectType::BitDepthMismatch {
                claimed: bit_depth_analysis.claimed_bit_depth,
                actual: bit_depth_analysis.actual_bit_depth,
                method: "multi-method".to_string(),
            },
            confidence: bit_depth_analysis.confidence,
            evidence: bit_depth_analysis.evidence.clone(),
        });
    }
    
    // ----- Check dithering (new) -----
    if let Some(ref dither) = dither_analysis {
        if dither.is_bit_reduced && dither.algorithm_confidence > config.min_confidence {
            // Only report as defect if it indicates quality loss
            if dither.effective_bit_depth < dither.container_bit_depth {
                defects.push(DetectedDefect {
                    defect_type: DefectType::DitheringDetected {
                        algorithm: format!("{}", dither.algorithm),
                        scale: format!("{}", dither.scale),
                        effective_bits: dither.effective_bit_depth,
                        container_bits: dither.container_bit_depth,
                    },
                    confidence: dither.algorithm_confidence,
                    evidence: dither.evidence.clone(),
                });
            }
        }
    }
    
    // ----- Check upsampling (basic) -----
    if upsampling_analysis.is_upsampled && upsampling_analysis.confidence > config.min_confidence {
        if let Some(orig_rate) = upsampling_analysis.original_sample_rate {
            defects.push(DetectedDefect {
                defect_type: DefectType::Upsampled {
                    from: orig_rate,
                    to: audio.sample_rate,
                    method: format!("{:?}", upsampling_analysis.detection_method),
                },
                confidence: upsampling_analysis.confidence,
                evidence: upsampling_analysis.evidence.clone(),
            });
        }
    }
    
    // ----- Check resampling (enhanced) -----
    if let Some(ref resample) = resample_analysis {
        if resample.is_resampled && resample.confidence > config.min_confidence {
            // Only report if not already caught by basic upsampling
            let already_reported = defects.iter().any(|d| matches!(d.defect_type, DefectType::Upsampled { .. }));
            
            if !already_reported {
                if let Some(orig_rate) = resample.original_sample_rate {
                    defects.push(DetectedDefect {
                        defect_type: DefectType::ResamplingDetected {
                            original_rate: orig_rate,
                            current_rate: resample.current_sample_rate,
                            engine: format!("{}", resample.engine),
                            quality: format!("{}", resample.quality),
                        },
                        confidence: resample.confidence,
                        evidence: resample.evidence.clone(),
                    });
                }
            }
        }
    }
    
    // ----- Check pre-echo -----
    if pre_echo_analysis.pre_echo_score > 0.5 {
        let confidence = pre_echo_analysis.pre_echo_score.min(1.0);
        if confidence > config.min_confidence {
            defects.push(DetectedDefect {
                defect_type: DefectType::PreEcho {
                    score: pre_echo_analysis.pre_echo_score,
                },
                confidence,
                evidence: pre_echo_analysis.evidence.clone(),
            });
        }
    }
    
    // =========================================================================
    // CALCULATE OVERALL QUALITY SCORE
    // =========================================================================
    let quality_score = calculate_quality_score(&defects, &bit_depth_analysis, &upsampling_analysis);
    let is_likely_lossless = defects.is_empty() && quality_score > 0.8;
    
    // =========================================================================
    // COMPUTE BASIC METRICS
    // =========================================================================
    let peak = mono.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
    let dynamic_range = if rms > 1e-10 { 20.0 * (peak / rms).log10() } else { 0.0 };
    let peak_db = if peak > 1e-10 { 20.0 * peak.log10() } else { -100.0 };
    let crest_factor = if rms > 1e-10 { 20.0 * (peak / rms).log10() } else { 0.0 };
    
    let has_brick_wall = spectral_analysis.rolloff_steepness > 60.0;
    
    QualityReport {
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        claimed_bit_depth: audio.claimed_bit_depth,
        actual_bit_depth: bit_depth_analysis.actual_bit_depth,
        duration_secs: audio.duration_secs,
        
        frequency_cutoff: spectral_analysis.cutoff_hz,
        spectral_rolloff: spectral_analysis.cutoff_hz * 0.85,
        rolloff_steepness: spectral_analysis.rolloff_steepness,
        has_brick_wall,
        spectral_flatness: 0.5,
        
        dynamic_range,
        peak_amplitude: peak_db,
        true_peak: peak_db,
        crest_factor,
        
        stereo_width: stereo_analysis.as_ref().map(|s| s.stereo_width),
        channel_correlation: stereo_analysis.as_ref().map(|s| s.correlation),
        
        quality_score,
        is_likely_lossless,
        defects,
        
        bit_depth_analysis,
        upsampling_analysis,
        pre_echo_analysis,
        
        dither_analysis,
        resample_analysis,
    }
}

/// Estimate MP3 bitrate from cutoff frequency
fn estimate_bitrate_from_cutoff(cutoff_hz: f32) -> Option<u32> {
    if cutoff_hz < 12000.0 {
        Some(64)
    } else if cutoff_hz < 14000.0 {
        Some(96)
    } else if cutoff_hz < 17000.0 {
        Some(128)
    } else if cutoff_hz < 19000.0 {
        Some(192)
    } else if cutoff_hz < 20000.0 {
        Some(256)
    } else if cutoff_hz < 20500.0 {
        Some(320)
    } else {
        None
    }
}

/// Simplified detection for quick checks
pub fn detect_quality_issues_simple(audio: &AudioData) -> QualityReport {
    detect_quality_issues(audio, &DetectionConfig::default())
}

fn calculate_quality_score(
    defects: &[DetectedDefect],
    bit_depth: &BitDepthAnalysis,
    upsampling: &UpsamplingAnalysis,
) -> f32 {
    let mut score = 1.0f32;
    
    for defect in defects {
        let penalty = match &defect.defect_type {
            DefectType::Mp3Transcode { .. } => 0.4,
            DefectType::OggVorbisTranscode { .. } => 0.35,
            DefectType::AacTranscode { .. } => 0.35,
            DefectType::OpusTranscode { .. } => 0.3,
            DefectType::BitDepthMismatch { .. } => 0.25,
            DefectType::Upsampled { .. } => 0.2,
            DefectType::SpectralArtifacts { .. } => 0.15,
            DefectType::JointStereo { .. } => 0.1,
            DefectType::PreEcho { .. } => 0.2,
            DefectType::PhaseDiscontinuities { .. } => 0.1,
            DefectType::Clipping { .. } => 0.1,
            DefectType::InterSampleOvers { .. } => 0.05,
            DefectType::LowQuality { .. } => 0.15,
            DefectType::DitheringDetected { .. } => 0.15, // Informational, lower penalty
            DefectType::ResamplingDetected { .. } => 0.2,
        };
        score -= penalty * defect.confidence;
    }
    
    if !bit_depth.is_mismatch && bit_depth.actual_bit_depth >= 24 {
        score += 0.05;
    }
    
    if !upsampling.is_upsampled {
        score += 0.05;
    }
    
    score.clamp(0.0, 1.0)
}
