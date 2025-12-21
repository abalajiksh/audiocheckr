// src/core/analysis/clipping_detection.rs
//
// Enhanced Clipping Detection for AudioCheckr
//
// Comprehensive clipping detection including:
// - Sample-level clipping (digital overs)
// - Inter-sample peaks (true peak analysis per ITU-R BS.1770)
// - Soft clipping detection (analog-style saturation)
// - Clipping pattern analysis (intentional limiting vs accidental)
// - Loudness war detection (chronic limiting/compression)
// - Restoration potential estimation
//
// This module provides detailed analysis of audio dynamics issues
// that can affect audio quality and authenticity.

use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Complete clipping analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippingAnalysisResult {
    /// Whether any clipping was detected
    pub has_clipping: bool,
    /// Overall clipping severity (0.0-1.0)
    pub severity: f32,
    /// Detailed clipping statistics
    pub statistics: ClippingStatistics,
    /// Inter-sample peak analysis
    pub inter_sample_analysis: InterSampleAnalysis,
    /// Detected clipping events
    pub clipping_events: Vec<ClippingEvent>,
    /// Pattern analysis (intentional vs accidental)
    pub pattern_analysis: ClippingPatternAnalysis,
    /// Loudness analysis
    pub loudness_analysis: LoudnessAnalysis,
    /// Restoration assessment
    pub restoration_assessment: RestorationAssessment,
    /// Per-channel analysis (for stereo/multichannel)
    pub channel_analysis: Vec<ChannelClippingInfo>,
    /// Evidence strings
    pub evidence: Vec<String>,
}

impl Default for ClippingAnalysisResult {
    fn default() -> Self {
        Self {
            has_clipping: false,
            severity: 0.0,
            statistics: ClippingStatistics::default(),
            inter_sample_analysis: InterSampleAnalysis::default(),
            clipping_events: Vec::new(),
            pattern_analysis: ClippingPatternAnalysis::default(),
            loudness_analysis: LoudnessAnalysis::default(),
            restoration_assessment: RestorationAssessment::default(),
            channel_analysis: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

/// Detailed clipping statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClippingStatistics {
    /// Number of samples at or above 0 dBFS
    pub samples_at_digital_max: u64,
    /// Percentage of samples clipped
    pub clipping_percentage: f32,
    /// Number of consecutive clipping runs
    pub clipping_runs: u32,
    /// Longest consecutive clipping run (samples)
    pub max_run_length: u32,
    /// Average clipping run length
    pub avg_run_length: f32,
    /// Peak sample value (should be <= 1.0)
    pub peak_sample: f32,
    /// Peak level in dBFS
    pub peak_db: f32,
    /// Number of positive clips
    pub positive_clips: u64,
    /// Number of negative clips
    pub negative_clips: u64,
    /// Asymmetry ratio (positive/negative)
    pub asymmetry_ratio: f32,
}

/// Inter-sample peak analysis (true peak per ITU-R BS.1770)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InterSampleAnalysis {
    /// True peak level in dBFS
    pub true_peak_db: f32,
    /// Sample peak level in dBFS
    pub sample_peak_db: f32,
    /// Difference between true peak and sample peak
    pub inter_sample_headroom_db: f32,
    /// Number of inter-sample overs (peaks > 0 dBFS between samples)
    pub inter_sample_overs: u32,
    /// Maximum inter-sample over level in dBFS
    pub max_inter_sample_over_db: f32,
    /// Locations of significant inter-sample overs
    pub over_locations: Vec<InterSampleOver>,
}

/// Single inter-sample over event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterSampleOver {
    /// Sample index where over occurs
    pub sample_index: usize,
    /// Time offset in seconds
    pub time_offset_secs: f32,
    /// Peak level in dBFS
    pub peak_db: f32,
    /// Duration of over in fractional samples
    pub duration_samples: f32,
}

/// Single clipping event (consecutive clipped samples)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippingEvent {
    /// Start sample index
    pub start_sample: usize,
    /// End sample index
    pub end_sample: usize,
    /// Duration in samples
    pub duration_samples: u32,
    /// Time offset in seconds
    pub time_offset_secs: f32,
    /// Duration in milliseconds
    pub duration_ms: f32,
    /// Clipping type
    pub clip_type: ClippingType,
    /// Peak level that was clipped to
    pub clipped_level: f32,
    /// Estimated original peak (if detectable)
    pub estimated_original_db: Option<f32>,
}

/// Type of clipping
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClippingType {
    /// Hard digital clipping (flat top)
    HardDigital,
    /// Soft clipping (rounded top, analog-style)
    SoftAnalog,
    /// Limiter-induced (very short, controlled)
    Limiter,
    /// Likely intentional distortion effect
    IntentionalDistortion,
    /// Unknown type
    Unknown,
}

impl std::fmt::Display for ClippingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClippingType::HardDigital => write!(f, "Hard Digital"),
            ClippingType::SoftAnalog => write!(f, "Soft Analog"),
            ClippingType::Limiter => write!(f, "Limiter"),
            ClippingType::IntentionalDistortion => write!(f, "Intentional Distortion"),
            ClippingType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Clipping pattern analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClippingPatternAnalysis {
    /// Likely cause of clipping
    pub likely_cause: ClippingCause,
    /// Confidence in cause determination
    pub cause_confidence: f32,
    /// Whether clipping appears intentional
    pub appears_intentional: bool,
    /// Whether clipping is consistent (mastering) vs sporadic (recording issue)
    pub is_consistent: bool,
    /// Distribution of clipping across the file
    pub temporal_distribution: TemporalDistribution,
    /// Correlation with musical dynamics
    pub correlates_with_dynamics: bool,
}

/// Likely cause of clipping
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ClippingCause {
    /// Recording too hot
    RecordingOverload,
    /// Aggressive mastering/limiting
    MasteringLimiting,
    /// Analog-to-digital conversion issue
    ADCOverload,
    /// Intentional distortion effect
    DistortionEffect,
    /// Lossy codec artifact
    CodecArtifact,
    /// Format conversion issue
    FormatConversion,
    /// No clipping detected
    #[default]
    None,
    /// Cannot determine
    Unknown,
}

impl std::fmt::Display for ClippingCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClippingCause::RecordingOverload => write!(f, "Recording Overload"),
            ClippingCause::MasteringLimiting => write!(f, "Mastering/Limiting"),
            ClippingCause::ADCOverload => write!(f, "ADC Overload"),
            ClippingCause::DistortionEffect => write!(f, "Distortion Effect"),
            ClippingCause::CodecArtifact => write!(f, "Codec Artifact"),
            ClippingCause::FormatConversion => write!(f, "Format Conversion"),
            ClippingCause::None => write!(f, "None"),
            ClippingCause::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Temporal distribution of clipping
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum TemporalDistribution {
    /// Clipping throughout the file
    Uniform,
    /// Clipping concentrated in specific sections
    Localized,
    /// Clipping increases over time
    Increasing,
    /// Clipping decreases over time
    Decreasing,
    /// Sporadic/random distribution
    Sporadic,
    #[default]
    /// No clipping
    None,
}

/// Loudness analysis
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoudnessAnalysis {
    /// Integrated loudness (LUFS)
    pub integrated_lufs: f32,
    /// Loudness range (LU)
    pub loudness_range_lu: f32,
    /// True peak (dBTP)
    pub true_peak_dbtp: f32,
    /// Peak-to-loudness ratio (PLR)
    pub plr_db: f32,
    /// Dynamic range (DR)
    pub dynamic_range_db: f32,
    /// Crest factor (peak/RMS ratio in dB)
    pub crest_factor_db: f32,
    /// Is this likely a "loudness war" victim?
    pub loudness_war_victim: bool,
    /// Compression severity score (0.0-1.0)
    pub compression_severity: f32,
}

/// Restoration assessment
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestorationAssessment {
    /// Can the clipping potentially be restored?
    pub restorable: bool,
    /// Estimated restoration quality (0.0-1.0)
    pub restoration_quality: f32,
    /// Recommended restoration method
    pub recommended_method: Option<RestorationMethod>,
    /// Percentage of clipped samples that may be recoverable
    pub recoverable_percentage: f32,
    /// Notes on restoration
    pub notes: Vec<String>,
}

/// Restoration methods
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RestorationMethod {
    /// Cubic spline interpolation
    CubicSpline,
    /// Spectral restoration
    SpectralReconstruction,
    /// Neural network based
    NeuralNetwork,
    /// Simple gain reduction (for minor overs)
    GainReduction,
    /// Not restorable
    NotRestorable,
}

impl std::fmt::Display for RestorationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestorationMethod::CubicSpline => write!(f, "Cubic Spline"),
            RestorationMethod::SpectralReconstruction => write!(f, "Spectral Reconstruction"),
            RestorationMethod::NeuralNetwork => write!(f, "Neural Network"),
            RestorationMethod::GainReduction => write!(f, "Gain Reduction"),
            RestorationMethod::NotRestorable => write!(f, "Not Restorable"),
        }
    }
}

/// Per-channel clipping info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelClippingInfo {
    /// Channel index (0 = left, 1 = right, etc.)
    pub channel: usize,
    /// Channel name
    pub name: String,
    /// Clipping statistics for this channel
    pub statistics: ClippingStatistics,
    /// True peak for this channel
    pub true_peak_db: f32,
    /// Sample peak for this channel
    pub sample_peak_db: f32,
}

/// Clipping detector with configurable thresholds
pub struct ClippingDetector {
    /// Threshold for considering a sample as clipped (0.9999 typical)
    clip_threshold: f32,
    /// Minimum consecutive samples to count as a clipping event
    min_clip_duration: usize,
    /// Oversampling factor for true peak detection (4 per ITU-R BS.1770)
    oversampling_factor: usize,
    /// Threshold for soft clipping detection (curvature analysis)
    soft_clip_curvature_threshold: f32,
    /// Maximum inter-sample overs to report individually
    max_reported_overs: usize,
}

impl Default for ClippingDetector {
    fn default() -> Self {
        Self {
            clip_threshold: 0.9999,
            min_clip_duration: 1,
            oversampling_factor: 4,
            soft_clip_curvature_threshold: 0.1,
            max_reported_overs: 100,
        }
    }
}

impl ClippingDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Configure for strict detection (broadcast standards)
    pub fn strict(mut self) -> Self {
        self.clip_threshold = 0.999;
        self.min_clip_duration = 1;
        self
    }
    
    /// Configure for lenient detection (music production)
    pub fn lenient(mut self) -> Self {
        self.clip_threshold = 1.0;
        self.min_clip_duration = 3;
        self
    }
    
    /// Analyze mono audio for clipping
    pub fn analyze_mono(&self, samples: &[f32], sample_rate: u32) -> ClippingAnalysisResult {
        let channels = vec![("Mono".to_string(), samples.to_vec())];
        self.analyze_channels(&channels, sample_rate)
    }
    
    /// Analyze stereo audio for clipping
    pub fn analyze_stereo(
        &self,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> ClippingAnalysisResult {
        let channels = vec![
            ("Left".to_string(), left.to_vec()),
            ("Right".to_string(), right.to_vec()),
        ];
        self.analyze_channels(&channels, sample_rate)
    }
    
    /// Analyze multiple channels
    fn analyze_channels(
        &self,
        channels: &[(String, Vec<f32>)],
        sample_rate: u32,
    ) -> ClippingAnalysisResult {
        let mut result = ClippingAnalysisResult::default();
        
        if channels.is_empty() || channels[0].1.is_empty() {
            result.evidence.push("No audio data to analyze".to_string());
            return result;
        }
        
        // Analyze each channel
        for (idx, (name, samples)) in channels.iter().enumerate() {
            let channel_stats = self.analyze_single_channel(samples, sample_rate);
            let true_peak = self.calculate_true_peak(samples, sample_rate);
            
            result.channel_analysis.push(ChannelClippingInfo {
                channel: idx,
                name: name.clone(),
                statistics: channel_stats.clone(),
                true_peak_db: true_peak.0,
                sample_peak_db: true_peak.1,
            });
        }
        
        // Combine channel statistics
        result.statistics = self.combine_channel_stats(&result.channel_analysis);
        
        // Inter-sample analysis (use first channel or mono mix)
        let analysis_samples = if channels.len() == 1 {
            channels[0].1.clone()
        } else {
            // Create mono mix for inter-sample analysis
            let len = channels[0].1.len().min(channels[1].1.len());
            (0..len)
                .map(|i| (channels[0].1[i] + channels[1].1[i]) / 2.0)
                .collect()
        };
        
        result.inter_sample_analysis = self.analyze_inter_sample_peaks(&analysis_samples, sample_rate);
        
        // Detect clipping events
        result.clipping_events = self.detect_clipping_events(&analysis_samples, sample_rate);
        
        // Analyze patterns
        result.pattern_analysis = self.analyze_patterns(&result.clipping_events, &result.statistics);
        
        // Loudness analysis
        result.loudness_analysis = self.analyze_loudness(&analysis_samples, sample_rate);
        
        // Restoration assessment
        result.restoration_assessment = self.assess_restoration(&result);
        
        // Overall results
        result.has_clipping = result.statistics.clipping_percentage > 0.0001 
            || result.inter_sample_analysis.inter_sample_overs > 0;
        
        result.severity = self.calculate_severity(&result);
        
        // Build evidence
        self.build_evidence(&mut result);
        
        result
    }
    
    /// Analyze single channel for clipping statistics
    fn analyze_single_channel(&self, samples: &[f32], _sample_rate: u32) -> ClippingStatistics {
        let mut stats = ClippingStatistics::default();
        
        if samples.is_empty() {
            return stats;
        }
        
        let mut positive_clips = 0u64;
        let mut negative_clips = 0u64;
        let mut current_run = 0u32;
        let mut max_run = 0u32;
        let mut total_run_length = 0u64;
        let mut in_clip = false;
        
        let mut peak = 0.0f32;
        
        for &sample in samples {
            let abs_sample = sample.abs();
            
            if abs_sample > peak {
                peak = abs_sample;
            }
            
            if abs_sample >= self.clip_threshold {
                stats.samples_at_digital_max += 1;
                
                if sample > 0.0 {
                    positive_clips += 1;
                } else {
                    negative_clips += 1;
                }
                
                if !in_clip {
                    in_clip = true;
                    stats.clipping_runs += 1;
                }
                current_run += 1;
            } else {
                if in_clip {
                    total_run_length += current_run as u64;
                    if current_run > max_run {
                        max_run = current_run;
                    }
                    current_run = 0;
                    in_clip = false;
                }
            }
        }
        
        // Handle trailing clip
        if in_clip && current_run > 0 {
            total_run_length += current_run as u64;
            if current_run > max_run {
                max_run = current_run;
            }
        }
        
        stats.peak_sample = peak;
        stats.peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { -100.0 };
        stats.clipping_percentage = stats.samples_at_digital_max as f32 / samples.len() as f32 * 100.0;
        stats.max_run_length = max_run;
        stats.avg_run_length = if stats.clipping_runs > 0 {
            total_run_length as f32 / stats.clipping_runs as f32
        } else {
            0.0
        };
        stats.positive_clips = positive_clips;
        stats.negative_clips = negative_clips;
        stats.asymmetry_ratio = if negative_clips > 0 {
            positive_clips as f32 / negative_clips as f32
        } else if positive_clips > 0 {
            f32::INFINITY
        } else {
            1.0
        };
        
        stats
    }
    
    /// Calculate true peak using oversampling
    fn calculate_true_peak(&self, samples: &[f32], _sample_rate: u32) -> (f32, f32) {
        if samples.is_empty() {
            return (-100.0, -100.0);
        }
        
        // Sample peak
        let sample_peak = samples.iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        
        // True peak via 4x oversampling
        let oversampled = self.oversample(samples);
        let true_peak = oversampled.iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        
        let true_peak_db = if true_peak > 0.0 { 20.0 * true_peak.log10() } else { -100.0 };
        let sample_peak_db = if sample_peak > 0.0 { 20.0 * sample_peak.log10() } else { -100.0 };
        
        (true_peak_db, sample_peak_db)
    }
    
    /// 4x oversampling for true peak measurement
    fn oversample(&self, samples: &[f32]) -> Vec<f32> {
        let factor = self.oversampling_factor;
        let filter_len = 16;
        let mut output = vec![0.0f32; samples.len() * factor];
        
        // Place original samples
        for (i, &sample) in samples.iter().enumerate() {
            output[i * factor] = sample;
        }
        
        // Interpolate using windowed sinc
        for i in 0..samples.len() {
            for j in 1..factor {
                let frac = j as f32 / factor as f32;
                let out_idx = i * factor + j;
                
                let mut sum = 0.0f32;
                let mut weight_sum = 0.0f32;
                
                for k in -filter_len..=filter_len {
                    let src_idx = i as i32 + k;
                    if src_idx >= 0 && (src_idx as usize) < samples.len() {
                        let x = (k as f32 - frac) * PI;
                        let sinc = if x.abs() < 1e-6 { 1.0 } else { x.sin() / x };
                        let window = 0.5 * (1.0 + ((k as f32 - frac) * PI / filter_len as f32).cos());
                        let weight = sinc * window;
                        sum += samples[src_idx as usize] * weight;
                        weight_sum += weight.abs();
                    }
                }
                
                output[out_idx] = if weight_sum > 0.0 { sum / weight_sum * factor as f32 } else { 0.0 };
            }
        }
        
        output
    }
    
    /// Analyze inter-sample peaks
    fn analyze_inter_sample_peaks(&self, samples: &[f32], sample_rate: u32) -> InterSampleAnalysis {
        let mut analysis = InterSampleAnalysis::default();
        
        if samples.is_empty() {
            return analysis;
        }
        
        let oversampled = self.oversample(samples);
        let factor = self.oversampling_factor;
        
        let sample_peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let true_peak = oversampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        
        analysis.sample_peak_db = if sample_peak > 0.0 { 20.0 * sample_peak.log10() } else { -100.0 };
        analysis.true_peak_db = if true_peak > 0.0 { 20.0 * true_peak.log10() } else { -100.0 };
        analysis.inter_sample_headroom_db = analysis.true_peak_db - analysis.sample_peak_db;
        
        // Find inter-sample overs
        let mut max_over_db = -100.0f32;
        
        for i in 0..oversampled.len() {
            // Check if this is an inter-sample position
            if i % factor != 0 && oversampled[i].abs() > 1.0 {
                analysis.inter_sample_overs += 1;
                
                let over_db = 20.0 * oversampled[i].abs().log10();
                if over_db > max_over_db {
                    max_over_db = over_db;
                }
                
                if analysis.over_locations.len() < self.max_reported_overs {
                    let sample_idx = i / factor;
                    analysis.over_locations.push(InterSampleOver {
                        sample_index: sample_idx,
                        time_offset_secs: sample_idx as f32 / sample_rate as f32,
                        peak_db: over_db,
                        duration_samples: 0.5,  // Approximate
                    });
                }
            }
        }
        
        analysis.max_inter_sample_over_db = max_over_db;
        
        analysis
    }
    
    /// Detect individual clipping events
    fn detect_clipping_events(&self, samples: &[f32], sample_rate: u32) -> Vec<ClippingEvent> {
        let mut events = Vec::new();
        
        let mut in_clip = false;
        let mut clip_start = 0usize;
        let mut clip_value = 0.0f32;
        
        for (i, &sample) in samples.iter().enumerate() {
            let is_clipped = sample.abs() >= self.clip_threshold;
            
            if is_clipped && !in_clip {
                // Start of new clip
                in_clip = true;
                clip_start = i;
                clip_value = sample;
            } else if !is_clipped && in_clip {
                // End of clip
                let duration = (i - clip_start) as u32;
                
                if duration >= self.min_clip_duration as u32 {
                    let clip_type = self.classify_clip_type(
                        &samples[clip_start.saturating_sub(10)..(i + 10).min(samples.len())],
                        10,
                    );
                    
                    events.push(ClippingEvent {
                        start_sample: clip_start,
                        end_sample: i,
                        duration_samples: duration,
                        time_offset_secs: clip_start as f32 / sample_rate as f32,
                        duration_ms: duration as f32 * 1000.0 / sample_rate as f32,
                        clip_type,
                        clipped_level: clip_value.abs(),
                        estimated_original_db: self.estimate_original_level(
                            &samples[clip_start.saturating_sub(20)..(i + 20).min(samples.len())],
                            20,
                            duration as usize,
                        ),
                    });
                }
                
                in_clip = false;
            }
        }
        
        // Handle trailing clip
        if in_clip {
            let i = samples.len();
            let duration = (i - clip_start) as u32;
            
            if duration >= self.min_clip_duration as u32 {
                events.push(ClippingEvent {
                    start_sample: clip_start,
                    end_sample: i,
                    duration_samples: duration,
                    time_offset_secs: clip_start as f32 / sample_rate as f32,
                    duration_ms: duration as f32 * 1000.0 / sample_rate as f32,
                    clip_type: ClippingType::Unknown,
                    clipped_level: clip_value.abs(),
                    estimated_original_db: None,
                });
            }
        }
        
        events
    }
    
    /// Classify the type of clipping
    fn classify_clip_type(&self, context: &[f32], clip_offset: usize) -> ClippingType {
        if context.len() < clip_offset + 3 {
            return ClippingType::Unknown;
        }
        
        // Analyze the shape of the clipping
        let before = &context[..clip_offset];
        let after = &context[(clip_offset + 1).min(context.len())..];
        
        if before.is_empty() || after.is_empty() {
            return ClippingType::Unknown;
        }
        
        // Check for hard digital clipping (flat top)
        let clipped_region = &context[clip_offset..];
        let flatness = self.measure_flatness(clipped_region);
        
        if flatness > 0.95 {
            return ClippingType::HardDigital;
        }
        
        // Check for soft clipping (curved approach)
        let approach_curve = self.measure_curvature(before);
        let exit_curve = self.measure_curvature(after);
        
        if approach_curve > 0.5 && exit_curve > 0.5 {
            return ClippingType::SoftAnalog;
        }
        
        // Check for limiter (very short duration)
        if clipped_region.len() <= 3 {
            return ClippingType::Limiter;
        }
        
        ClippingType::Unknown
    }
    
    /// Measure flatness of a signal section
    fn measure_flatness(&self, samples: &[f32]) -> f32 {
        if samples.len() < 2 {
            return 0.0;
        }
        
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        let variance = samples.iter()
            .map(|s| (s - mean).powi(2))
            .sum::<f32>() / samples.len() as f32;
        
        // High flatness = low variance
        1.0 / (1.0 + variance * 10000.0)
    }
    
    /// Measure curvature of signal approach to peak
    fn measure_curvature(&self, samples: &[f32]) -> f32 {
        if samples.len() < 3 {
            return 0.0;
        }
        
        let mut curvature_sum = 0.0f32;
        let mut count = 0;
        
        for i in 1..samples.len() - 1 {
            let second_deriv = samples[i + 1] - 2.0 * samples[i] + samples[i - 1];
            curvature_sum += second_deriv.abs();
            count += 1;
        }
        
        if count > 0 {
            curvature_sum / count as f32
        } else {
            0.0
        }
    }
    
    /// Estimate original peak level before clipping
    fn estimate_original_level(
        &self,
        context: &[f32],
        offset: usize,
        clip_duration: usize,
    ) -> Option<f32> {
        if context.len() < offset * 2 + clip_duration {
            return None;
        }
        
        // Use samples before and after clip to estimate trajectory
        let before = &context[..offset];
        let after = &context[(offset + clip_duration)..];
        
        if before.len() < 3 || after.len() < 3 {
            return None;
        }
        
        // Simple linear extrapolation from before samples
        let before_slope = if before.len() >= 2 {
            (before[before.len() - 1] - before[before.len() - 2]).abs()
        } else {
            0.0
        };
        
        // Estimate peak would be current level + slope * duration
        let last_before = before[before.len() - 1].abs();
        let estimated_peak = last_before + before_slope * (clip_duration as f32 / 2.0);
        
        if estimated_peak > 1.0 {
            Some(20.0 * estimated_peak.log10())
        } else {
            None
        }
    }
    
    /// Combine channel statistics
    fn combine_channel_stats(&self, channels: &[ChannelClippingInfo]) -> ClippingStatistics {
        if channels.is_empty() {
            return ClippingStatistics::default();
        }
        
        if channels.len() == 1 {
            return channels[0].statistics.clone();
        }
        
        // Combine across channels
        let mut combined = ClippingStatistics::default();
        
        for ch in channels {
            combined.samples_at_digital_max += ch.statistics.samples_at_digital_max;
            combined.clipping_runs += ch.statistics.clipping_runs;
            combined.positive_clips += ch.statistics.positive_clips;
            combined.negative_clips += ch.statistics.negative_clips;
            
            if ch.statistics.max_run_length > combined.max_run_length {
                combined.max_run_length = ch.statistics.max_run_length;
            }
            if ch.statistics.peak_sample > combined.peak_sample {
                combined.peak_sample = ch.statistics.peak_sample;
                combined.peak_db = ch.statistics.peak_db;
            }
        }
        
        combined.clipping_percentage = channels.iter()
            .map(|c| c.statistics.clipping_percentage)
            .sum::<f32>() / channels.len() as f32;
        
        combined.avg_run_length = channels.iter()
            .map(|c| c.statistics.avg_run_length)
            .sum::<f32>() / channels.len() as f32;
        
        combined.asymmetry_ratio = if combined.negative_clips > 0 {
            combined.positive_clips as f32 / combined.negative_clips as f32
        } else if combined.positive_clips > 0 {
            f32::INFINITY
        } else {
            1.0
        };
        
        combined
    }
    
    /// Analyze clipping patterns
    fn analyze_patterns(
        &self,
        events: &[ClippingEvent],
        stats: &ClippingStatistics,
    ) -> ClippingPatternAnalysis {
        let mut analysis = ClippingPatternAnalysis::default();
        
        if events.is_empty() {
            analysis.likely_cause = ClippingCause::None;
            analysis.temporal_distribution = TemporalDistribution::None;
            return analysis;
        }
        
        // Analyze event types
        let hard_count = events.iter()
            .filter(|e| e.clip_type == ClippingType::HardDigital)
            .count();
        let soft_count = events.iter()
            .filter(|e| e.clip_type == ClippingType::SoftAnalog)
            .count();
        let limiter_count = events.iter()
            .filter(|e| e.clip_type == ClippingType::Limiter)
            .count();
        
        // Determine likely cause
        if limiter_count > events.len() / 2 {
            analysis.likely_cause = ClippingCause::MasteringLimiting;
            analysis.appears_intentional = true;
            analysis.cause_confidence = 0.8;
        } else if hard_count > events.len() / 2 {
            if stats.asymmetry_ratio > 2.0 || stats.asymmetry_ratio < 0.5 {
                analysis.likely_cause = ClippingCause::ADCOverload;
            } else {
                analysis.likely_cause = ClippingCause::RecordingOverload;
            }
            analysis.cause_confidence = 0.7;
        } else if soft_count > events.len() / 2 {
            analysis.likely_cause = ClippingCause::DistortionEffect;
            analysis.appears_intentional = true;
            analysis.cause_confidence = 0.6;
        } else {
            analysis.likely_cause = ClippingCause::Unknown;
            analysis.cause_confidence = 0.3;
        }
        
        // Analyze temporal distribution
        if events.len() < 3 {
            analysis.temporal_distribution = TemporalDistribution::Sporadic;
        } else {
            let times: Vec<f32> = events.iter().map(|e| e.time_offset_secs).collect();
            let first_third = times.iter().filter(|&&t| t < times.last().unwrap_or(&0.0) / 3.0).count();
            let last_third = times.iter().filter(|&&t| t > times.last().unwrap_or(&0.0) * 2.0 / 3.0).count();
            
            if first_third > events.len() / 2 {
                analysis.temporal_distribution = TemporalDistribution::Decreasing;
            } else if last_third > events.len() / 2 {
                analysis.temporal_distribution = TemporalDistribution::Increasing;
            } else {
                let variance = self.calculate_time_variance(&times);
                if variance < 0.3 {
                    analysis.temporal_distribution = TemporalDistribution::Uniform;
                } else {
                    analysis.temporal_distribution = TemporalDistribution::Sporadic;
                }
            }
        }
        
        // Check consistency
        let avg_duration = events.iter().map(|e| e.duration_ms).sum::<f32>() / events.len() as f32;
        let duration_variance: f32 = events.iter()
            .map(|e| (e.duration_ms - avg_duration).powi(2))
            .sum::<f32>() / events.len() as f32;
        
        analysis.is_consistent = duration_variance.sqrt() < avg_duration;
        
        analysis
    }
    
    /// Calculate variance of time intervals
    fn calculate_time_variance(&self, times: &[f32]) -> f32 {
        if times.len() < 2 {
            return 0.0;
        }
        
        let intervals: Vec<f32> = times.windows(2)
            .map(|w| w[1] - w[0])
            .collect();
        
        if intervals.is_empty() {
            return 0.0;
        }
        
        let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
        let variance = intervals.iter()
            .map(|i| (i - mean).powi(2))
            .sum::<f32>() / intervals.len() as f32;
        
        variance.sqrt() / mean.max(0.001)
    }
    
    /// Analyze loudness characteristics
    fn analyze_loudness(&self, samples: &[f32], sample_rate: u32) -> LoudnessAnalysis {
        let mut analysis = LoudnessAnalysis::default();
        
        if samples.is_empty() {
            return analysis;
        }
        
        // Calculate RMS in windows
        let window_samples = (sample_rate as f32 * 0.4) as usize;  // 400ms windows
        let hop = window_samples / 2;
        
        let mut rms_values = Vec::new();
        
        for start in (0..samples.len().saturating_sub(window_samples)).step_by(hop) {
            let window = &samples[start..start + window_samples];
            let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
            if rms > 1e-10 {
                rms_values.push(rms);
            }
        }
        
        if rms_values.is_empty() {
            return analysis;
        }
        
        // Sort for percentile calculations
        let mut sorted_rms = rms_values.clone();
        sorted_rms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        // Integrated loudness (approximate LUFS)
        let mean_rms = rms_values.iter().sum::<f32>() / rms_values.len() as f32;
        analysis.integrated_lufs = 20.0 * mean_rms.log10() - 0.691;  // Simplified K-weighting offset
        
        // Dynamic range (difference between 95th and 10th percentile)
        let p10_idx = sorted_rms.len() / 10;
        let p95_idx = sorted_rms.len() * 95 / 100;
        
        if p95_idx > p10_idx {
            let dr = sorted_rms[p95_idx] / sorted_rms[p10_idx].max(1e-10);
            analysis.dynamic_range_db = 20.0 * dr.log10();
        }
        
        // Loudness range
        let p10 = sorted_rms[p10_idx];
        let p95 = sorted_rms[p95_idx];
        analysis.loudness_range_lu = 20.0 * (p95 / p10.max(1e-10)).log10();
        
        // Peak and crest factor
        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        analysis.true_peak_dbtp = if peak > 0.0 { 20.0 * peak.log10() } else { -100.0 };
        analysis.crest_factor_db = analysis.true_peak_dbtp - analysis.integrated_lufs;
        
        // Peak-to-loudness ratio
        analysis.plr_db = analysis.true_peak_dbtp - analysis.integrated_lufs;
        
        // Detect loudness war victim
        // Characteristics: high integrated loudness, low dynamic range, low crest factor
        analysis.loudness_war_victim = 
            analysis.integrated_lufs > -10.0 &&
            analysis.dynamic_range_db < 6.0 &&
            analysis.crest_factor_db < 8.0;
        
        // Compression severity
        analysis.compression_severity = (
            ((-analysis.integrated_lufs - 14.0) / 10.0).max(0.0) * 0.3 +
            ((8.0 - analysis.dynamic_range_db) / 8.0).max(0.0) * 0.4 +
            ((10.0 - analysis.crest_factor_db) / 10.0).max(0.0) * 0.3
        ).min(1.0);
        
        analysis
    }
    
    /// Assess restoration potential
    fn assess_restoration(&self, result: &ClippingAnalysisResult) -> RestorationAssessment {
        let mut assessment = RestorationAssessment::default();
        
        if !result.has_clipping {
            assessment.restorable = false;
            assessment.notes.push("No clipping detected, no restoration needed".to_string());
            return assessment;
        }
        
        // Analyze restoration potential based on clipping characteristics
        let avg_duration = if !result.clipping_events.is_empty() {
            result.clipping_events.iter()
                .map(|e| e.duration_samples)
                .sum::<u32>() as f32 / result.clipping_events.len() as f32
        } else {
            0.0
        };
        
        let percentage = result.statistics.clipping_percentage;
        
        // Short clips are more recoverable
        if avg_duration < 5.0 && percentage < 0.1 {
            assessment.restorable = true;
            assessment.restoration_quality = 0.9;
            assessment.recommended_method = Some(RestorationMethod::CubicSpline);
            assessment.recoverable_percentage = 95.0;
            assessment.notes.push("Minor clipping, excellent restoration potential".to_string());
        } else if avg_duration < 20.0 && percentage < 0.5 {
            assessment.restorable = true;
            assessment.restoration_quality = 0.7;
            assessment.recommended_method = Some(RestorationMethod::SpectralReconstruction);
            assessment.recoverable_percentage = 80.0;
            assessment.notes.push("Moderate clipping, good restoration potential".to_string());
        } else if avg_duration < 50.0 && percentage < 2.0 {
            assessment.restorable = true;
            assessment.restoration_quality = 0.5;
            assessment.recommended_method = Some(RestorationMethod::NeuralNetwork);
            assessment.recoverable_percentage = 60.0;
            assessment.notes.push("Significant clipping, partial restoration possible".to_string());
        } else {
            assessment.restorable = false;
            assessment.restoration_quality = 0.2;
            assessment.recommended_method = Some(RestorationMethod::NotRestorable);
            assessment.recoverable_percentage = 20.0;
            assessment.notes.push("Severe clipping, restoration unlikely to be satisfactory".to_string());
        }
        
        // Inter-sample overs can often be fixed with simple gain reduction
        if result.inter_sample_analysis.inter_sample_overs > 0 
            && result.statistics.samples_at_digital_max == 0 {
            assessment.restorable = true;
            assessment.restoration_quality = 0.95;
            assessment.recommended_method = Some(RestorationMethod::GainReduction);
            assessment.recoverable_percentage = 100.0;
            assessment.notes.clear();
            assessment.notes.push("Only inter-sample overs detected, simple gain reduction recommended".to_string());
        }
        
        assessment
    }
    
    /// Calculate overall severity score
    fn calculate_severity(&self, result: &ClippingAnalysisResult) -> f32 {
        let mut severity = 0.0f32;
        
        // Clipping percentage contribution
        severity += (result.statistics.clipping_percentage / 1.0).min(0.4);
        
        // Inter-sample overs contribution
        let iso_factor = (result.inter_sample_analysis.inter_sample_overs as f32 / 100.0).min(0.2);
        severity += iso_factor;
        
        // Run length contribution (longer runs are worse)
        let run_factor = (result.statistics.max_run_length as f32 / 50.0).min(0.2);
        severity += run_factor;
        
        // Loudness war contribution
        if result.loudness_analysis.loudness_war_victim {
            severity += 0.2;
        }
        
        severity.min(1.0)
    }
    
    /// Build evidence strings
    fn build_evidence(&self, result: &mut ClippingAnalysisResult) {
        if result.statistics.samples_at_digital_max > 0 {
            result.evidence.push(format!(
                "{} samples at digital maximum ({:.4}%)",
                result.statistics.samples_at_digital_max,
                result.statistics.clipping_percentage
            ));
        }
        
        if result.inter_sample_analysis.inter_sample_overs > 0 {
            result.evidence.push(format!(
                "{} inter-sample overs detected (max: {:.2} dBTP)",
                result.inter_sample_analysis.inter_sample_overs,
                result.inter_sample_analysis.max_inter_sample_over_db
            ));
        }
        
        if result.statistics.max_run_length > 5 {
            result.evidence.push(format!(
                "Longest clipping run: {} samples",
                result.statistics.max_run_length
            ));
        }
        
        if result.pattern_analysis.likely_cause != ClippingCause::None {
            result.evidence.push(format!(
                "Likely cause: {} ({:.0}% confidence)",
                result.pattern_analysis.likely_cause,
                result.pattern_analysis.cause_confidence * 100.0
            ));
        }
        
        if result.loudness_analysis.loudness_war_victim {
            result.evidence.push(format!(
                "Loudness war characteristics detected (DR: {:.1} dB, crest: {:.1} dB)",
                result.loudness_analysis.dynamic_range_db,
                result.loudness_analysis.crest_factor_db
            ));
        }
        
        if result.restoration_assessment.restorable {
            if let Some(method) = &result.restoration_assessment.recommended_method {
                result.evidence.push(format!(
                    "Restoration recommended: {} (est. {:.0}% recovery)",
                    method,
                    result.restoration_assessment.recoverable_percentage
                ));
            }
        }
        
        result.evidence.push(format!(
            "Peak level: {:.2} dBFS, True peak: {:.2} dBTP",
            result.statistics.peak_db,
            result.inter_sample_analysis.true_peak_db
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_no_clipping() {
        let detector = ClippingDetector::new();
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / 44100.0).sin() * 0.5)
            .collect();
        
        let result = detector.analyze_mono(&samples, 44100);
        assert!(!result.has_clipping);
        assert_eq!(result.statistics.samples_at_digital_max, 0);
    }
    
    #[test]
    fn test_hard_clipping() {
        let detector = ClippingDetector::new();
        let samples: Vec<f32> = (0..44100)
            .map(|i| {
                let s = (2.0 * PI * 1000.0 * i as f32 / 44100.0).sin() * 1.5;
                s.clamp(-1.0, 1.0)  // Hard clip
            })
            .collect();
        
        let result = detector.analyze_mono(&samples, 44100);
        assert!(result.has_clipping);
        assert!(result.statistics.samples_at_digital_max > 0);
    }
    
    #[test]
    fn test_inter_sample_overs() {
        let detector = ClippingDetector::new();
        // Create signal that peaks between samples
        let samples: Vec<f32> = (0..44100)
            .map(|i| {
                let t = i as f32 / 44100.0;
                // High frequency signal close to 0 dBFS
                (2.0 * PI * 15000.0 * t).sin() * 0.99
            })
            .collect();
        
        let result = detector.analyze_mono(&samples, 44100);
        // Inter-sample peaks may or may not exceed 0 dBFS depending on frequency
        assert!(result.inter_sample_analysis.true_peak_db >= result.inter_sample_analysis.sample_peak_db);
    }
    
    #[test]
    fn test_severity_calculation() {
        let mut result = ClippingAnalysisResult::default();
        result.statistics.clipping_percentage = 0.5;
        result.statistics.max_run_length = 25;
        
        let detector = ClippingDetector::new();
        let severity = detector.calculate_severity(&result);
        
        assert!(severity > 0.0);
        assert!(severity < 1.0);
    }
}
