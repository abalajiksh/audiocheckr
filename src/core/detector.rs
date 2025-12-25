// src/core/detector.rs
//
// Quality issue detection with configurable thresholds and profiles.
// Now includes enhanced dithering and resampling detection.
//
// CRITICAL FIX: Detection order matters!
// 1. Resampling detection FIRST
// 2. Lossy codec detection ONLY IF:
//    a) File is at native 44.1/48kHz, OR
//    b) Upsampling was detected (check against ORIGINAL sample rate)
// 3. Other detections (bit depth, dithering, etc.)

use serde::{Deserialize, Serialize};
use super::decoder::AudioData;
use super::analysis::{
    analyze_bit_depth, BitDepthAnalysis,
    analyze_upsampling, UpsamplingAnalysis,
    analyze_pre_echo, PreEchoAnalysis,
    analyze_stereo,
    SpectralAnalyzer, detect_transcode, Codec, TranscodeResult,
    DitherDetector, DitherDetectionResult, DitherAlgorithm, DitherScale,
    ResampleDetector, ResampleDetectionResult, ResamplerEngine, ResampleQuality, ResampleDirection,
    MqaDetector, MqaType,
    DetectionContext,
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
    
    // Enhanced defect types
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
    MqaEncoded {
        original_rate: Option<u32>,
        mqa_type: String,
        lsb_entropy: f32,
    },
    /// NEW: Upsampled lossy transcode (the key case we're fixing)
    UpsampledLossyTranscode {
        original_rate: u32,
        current_rate: u32,
        codec: String,
        estimated_bitrate: Option<u32>,
        cutoff_hz: u32,
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
    
    // Enhanced analysis results
    pub dither_analysis: Option<DitherDetectionResult>,
    pub resample_analysis: Option<ResampleDetectionResult>,
}

/// Run quality detection on decoded audio
pub fn detect_quality_issues(audio: &AudioData, config: &DetectionConfig) -> QualityReport {
    let mono = super::decoder::extract_mono(audio);
    let mut defects = Vec::new();
    
    // =========================================================================
    // STEP 1: RESAMPLING DETECTION (MUST BE FIRST!)
    // This determines whether lossy codec detection is applicable
    // =========================================================================
    let resample_analysis = if config.check_resampling {
        let detector = ResampleDetector::new();
        Some(detector.analyze(&mono, audio.sample_rate))
    } else {
        None
    };
    
    // Also run basic upsampling analysis
    let upsampling_analysis = if config.check_upsampling {
        analyze_upsampling(&mono, audio.sample_rate)
    } else {
        UpsamplingAnalysis::default()
    };
    
    // Determine if file was upsampled and what the original rate was
    let (is_upsampled, original_sample_rate) = determine_original_sample_rate(
        &resample_analysis,
        &upsampling_analysis,
        audio.sample_rate,
    );
    
    // Create detection context for sample-rate-aware detection
    let mut detection_context = DetectionContext::new(audio.sample_rate, audio.claimed_bit_depth as u8);
    
    // Update context with resampling info
    if let Some(ref resample) = resample_analysis {
        detection_context.set_resampling(resample.clone());
    }
    
    // =========================================================================
    // STEP 2: LOSSY CODEC DETECTION (CONDITIONAL)
    // Only run if:
    // - Native 44.1/48kHz file, OR
    // - Upsampled file (check against original sample rate)
    // =========================================================================
    let (spectral_analysis, transcode_result) = run_lossy_detection_if_applicable(
        &mono,
        audio.sample_rate,
        is_upsampled,
        original_sample_rate,
        &detection_context,
    );
    
    // Add lossy transcode defect if detected
    if let Some(ref transcode) = transcode_result {
        if transcode.is_transcode && transcode.confidence > config.min_confidence {
            let defect = create_transcode_defect(
                transcode,
                is_upsampled,
                original_sample_rate,
                audio.sample_rate,
            );
            defects.push(defect);
        }
    }
    
    // =========================================================================
    // STEP 3: RESAMPLING/UPSAMPLING DEFECTS
    // Report resampling as a defect (separate from lossy detection)
    // =========================================================================
    if let Some(ref resample) = resample_analysis {
        if resample.is_resampled && resample.confidence > config.min_confidence {
            // Only report if not already covered by upsampled lossy transcode
            let already_reported_as_lossy = defects.iter().any(|d| 
                matches!(d.defect_type, DefectType::UpsampledLossyTranscode { .. })
            );
            
            if !already_reported_as_lossy {
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
    
    // Also check basic upsampling if not caught by enhanced detection
    if upsampling_analysis.is_upsampled && upsampling_analysis.confidence > config.min_confidence {
        let already_reported = defects.iter().any(|d| matches!(
            d.defect_type,
            DefectType::Upsampled { .. } | 
            DefectType::ResamplingDetected { .. } |
            DefectType::UpsampledLossyTranscode { .. }
        ));
        
        if !already_reported {
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
    }
    
    // =========================================================================
    // STEP 4: BIT DEPTH ANALYSIS
    // =========================================================================
    let bit_depth_analysis = analyze_bit_depth(audio);
    
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
    
    // =========================================================================
    // STEP 5: DITHERING DETECTION
    // =========================================================================
    let dither_analysis = if config.check_dithering {
        let detector = DitherDetector::new(audio.sample_rate);
        let result = detector.analyze(&mono, audio.claimed_bit_depth as u8);
        
        // Update detection context
        detection_context.set_dithering(result.clone());
        
        if result.is_bit_reduced && result.algorithm_confidence > config.min_confidence {
            if result.effective_bit_depth < result.container_bit_depth {
                defects.push(DetectedDefect {
                    defect_type: DefectType::DitheringDetected {
                        algorithm: format!("{}", result.algorithm),
                        scale: format!("{}", result.scale),
                        effective_bits: result.effective_bit_depth,
                        container_bits: result.container_bit_depth,
                    },
                    confidence: result.algorithm_confidence,
                    evidence: result.evidence.clone(),
                });
            }
        }
        
        Some(result)
    } else {
        None
    };
    
    // =========================================================================
    // STEP 6: MQA DETECTION
    // =========================================================================
    let mqa_detector = MqaDetector::default();
    let mqa_result = mqa_detector.detect(&mono, audio.sample_rate, audio.claimed_bit_depth);
    
    if mqa_result.is_mqa_encoded && mqa_result.confidence > config.min_confidence {
        defects.push(DetectedDefect {
            defect_type: DefectType::MqaEncoded {
                original_rate: mqa_result.original_sample_rate,
                mqa_type: format!("{:?}", mqa_result.mqa_type.unwrap_or(MqaType::Unknown)),
                lsb_entropy: mqa_result.lsb_entropy,
            },
            confidence: mqa_result.confidence,
            evidence: mqa_result.evidence,
        });
    }
    
    // =========================================================================
    // STEP 7: PRE-ECHO ANALYSIS
    // =========================================================================
    let pre_echo_analysis = if config.check_transients {
        analyze_pre_echo(&mono, audio.sample_rate)
    } else {
        PreEchoAnalysis::default()
    };
    
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
    // STEP 8: STEREO ANALYSIS
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
    
    // Use spectral analysis results or defaults
    let (frequency_cutoff, rolloff_steepness) = spectral_analysis
        .as_ref()
        .map(|s| (s.cutoff_hz, s.rolloff_steepness))
        .unwrap_or((audio.sample_rate as f32 / 2.0, 0.0));
    
    let has_brick_wall = rolloff_steepness > 60.0;
    
    QualityReport {
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        claimed_bit_depth: audio.claimed_bit_depth,
        actual_bit_depth: bit_depth_analysis.actual_bit_depth,
        duration_secs: audio.duration_secs,
        
        frequency_cutoff,
        spectral_rolloff: frequency_cutoff * 0.85,
        rolloff_steepness,
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

/// Determine if file was upsampled and what the original sample rate was
fn determine_original_sample_rate(
    resample: &Option<ResampleDetectionResult>,
    upsampling: &UpsamplingAnalysis,
    current_rate: u32,
) -> (bool, Option<u32>) {
    // Prefer enhanced resampling detection
    if let Some(ref r) = resample {
        if r.is_resampled && r.direction == ResampleDirection::Upsample {
            return (true, r.original_sample_rate);
        }
    }
    
    // Fall back to basic upsampling detection
    if upsampling.is_upsampled {
        return (true, upsampling.original_sample_rate);
    }
    
    (false, None)
}

/// Run lossy codec detection only if applicable based on sample rate
fn run_lossy_detection_if_applicable(
    samples: &[f32],
    current_sample_rate: u32,
    is_upsampled: bool,
    original_sample_rate: Option<u32>,
    context: &DetectionContext,
) -> (Option<super::analysis::SpectralAnalysis>, Option<TranscodeResult>) {
    let spectral_analyzer = SpectralAnalyzer::default();
    
    // Always run spectral analysis for visualization/metrics
    let spectral_analysis = spectral_analyzer.analyze(samples, current_sample_rate);
    
    // Determine if we should check for lossy transcodes
    let effective_rate_for_lossy_check = if is_upsampled {
        // File was upsampled - check against ORIGINAL rate
        original_sample_rate.unwrap_or(current_sample_rate)
    } else {
        current_sample_rate
    };
    
    // MP3/AAC max sample rate is 48kHz
    // If effective rate > 48kHz and NOT upsampled, skip lossy detection
    if effective_rate_for_lossy_check > 48000 && !is_upsampled {
        // Native hi-res file - cannot have MP3/AAC artifacts
        return (Some(spectral_analysis), None);
    }
    
    // Check if context says we should suppress lossy detection
    // (e.g., due to detected high-quality resampling filter artifacts)
    if context.suppress_lossy_detection && !is_upsampled {
        return (Some(spectral_analysis), None);
    }
    
    // Run lossy detection
    let transcode_result = detect_transcode(samples, current_sample_rate);
    
    // For upsampled files, adjust the interpretation
    // The cutoff should be relative to the ORIGINAL Nyquist
    if is_upsampled && transcode_result.is_transcode {
        if let Some(orig_rate) = original_sample_rate {
            let orig_nyquist = orig_rate as f32 / 2.0;
            
            // If the detected cutoff is near or below the original Nyquist,
            // it's a valid lossy transcode detection
            if transcode_result.cutoff_hz <= orig_nyquist * 1.05 {
                return (Some(spectral_analysis), Some(transcode_result));
            } else {
                // Cutoff is above original Nyquist - likely false positive from resampling
                // The rolloff we're seeing is from the resampling filter, not lossy codec
                return (Some(spectral_analysis), None);
            }
        }
    }
    
    (Some(spectral_analysis), Some(transcode_result))
}

/// Create appropriate defect type for detected transcode
fn create_transcode_defect(
    transcode: &TranscodeResult,
    is_upsampled: bool,
    original_sample_rate: Option<u32>,
    current_sample_rate: u32,
) -> DetectedDefect {
    let mut evidence = vec![transcode.reason.clone()];
    
    if let Some(ref codec_sig) = transcode.likely_codec {
        evidence.push(format!(
            "Matches {:?} signature at {}kbps (confidence: {:.0}%)",
            codec_sig.codec,
            codec_sig.bitrate.unwrap_or(0),
            codec_sig.confidence * 100.0
        ));
    }
    
    // If upsampled, create the special UpsampledLossyTranscode defect
    if is_upsampled {
        if let Some(orig_rate) = original_sample_rate {
            let codec_name = transcode.likely_codec
                .as_ref()
                .map(|c| format!("{:?}", c.codec))
                .unwrap_or_else(|| "Unknown Lossy".to_string());
            
            let estimated_bitrate = transcode.likely_codec
                .as_ref()
                .and_then(|c| c.bitrate);
            
            evidence.push(format!(
                "File was upsampled from {} Hz to {} Hz, but contains lossy codec artifacts",
                orig_rate, current_sample_rate
            ));
            evidence.push(format!(
                "Original lossy source was likely {} at ~{}kbps, then upsampled to fake hi-res",
                codec_name,
                estimated_bitrate.unwrap_or(0)
            ));
            
            return DetectedDefect {
                defect_type: DefectType::UpsampledLossyTranscode {
                    original_rate: orig_rate,
                    current_rate: current_sample_rate,
                    codec: codec_name,
                    estimated_bitrate,
                    cutoff_hz: transcode.cutoff_hz as u32,
                },
                confidence: transcode.confidence,
                evidence,
            };
        }
    }
    
    // Standard transcode defect (not upsampled)
    let defect_type = match transcode.likely_codec {
        Some(ref sig) => match sig.codec {
            Codec::MP3 => DefectType::Mp3Transcode {
                cutoff_hz: transcode.cutoff_hz as u32,
                estimated_bitrate: sig.bitrate,
            },
            Codec::AAC => DefectType::AacTranscode {
                cutoff_hz: transcode.cutoff_hz as u32,
                estimated_bitrate: sig.bitrate,
            },
            Codec::Opus => DefectType::OpusTranscode {
                cutoff_hz: transcode.cutoff_hz as u32,
                mode: format!("{}kbps", sig.bitrate.unwrap_or(0)),
            },
            Codec::Vorbis => DefectType::OggVorbisTranscode {
                cutoff_hz: transcode.cutoff_hz as u32,
                estimated_quality: sig.bitrate.map(|b| b as f32 / 32.0),
            },
            Codec::Unknown => DefectType::Mp3Transcode {
                cutoff_hz: transcode.cutoff_hz as u32,
                estimated_bitrate: estimate_bitrate_from_cutoff(transcode.cutoff_hz),
            },
        },
        None => DefectType::Mp3Transcode {
            cutoff_hz: transcode.cutoff_hz as u32,
            estimated_bitrate: estimate_bitrate_from_cutoff(transcode.cutoff_hz),
        },
    };
    
    DetectedDefect {
        defect_type,
        confidence: transcode.confidence,
        evidence,
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
            // Upsampled lossy is the worst - fake hi-res from lossy source
            DefectType::UpsampledLossyTranscode { .. } => 0.5,
            DefectType::Mp3Transcode { .. } => 0.4,
            DefectType::OggVorbisTranscode { .. } => 0.35,
            DefectType::AacTranscode { .. } => 0.35,
            DefectType::OpusTranscode { .. } => 0.3,
            DefectType::MqaEncoded { .. } => 0.35,
            DefectType::BitDepthMismatch { .. } => 0.25,
            DefectType::Upsampled { .. } => 0.2,
            DefectType::ResamplingDetected { .. } => 0.15, // Less penalty if no lossy detected
            DefectType::SpectralArtifacts { .. } => 0.15,
            DefectType::JointStereo { .. } => 0.1,
            DefectType::PreEcho { .. } => 0.2,
            DefectType::PhaseDiscontinuities { .. } => 0.1,
            DefectType::Clipping { .. } => 0.1,
            DefectType::InterSampleOvers { .. } => 0.05,
            DefectType::LowQuality { .. } => 0.15,
            DefectType::DitheringDetected { .. } => 0.10, // Informational
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
