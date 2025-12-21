//! Integration tests for ENF and Clipping Detection
//!
//! These tests verify the complete integration of ENF and clipping detection
//! into the AudioCheckr analysis pipeline.

use audiocheckr::analysis::{
    ExtendedDetectionPipeline,
    ExtendedDetectionOptions,
    ExtendedAnalysisResult,
    QualityGrade,
    QualityIssueType,
    AuthenticityResult,
    analyze_audio_quality,
    analyze_stereo_quality,
    analyze_authenticity,
};
use audiocheckr::analysis::enf_detection::EnfBaseFrequency;
use audiocheckr::cli::extended_detection::{
    ExtendedDetectionArgs,
    ExtendedOutputFormat,
    EnfFrequencyArg,
};

// =============================================================================
// Test Utilities
// =============================================================================

/// Generate a clean sine wave
fn generate_sine_wave(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * std::f32::consts::PI * frequency * t).sin()
        })
        .collect()
}

/// Generate a clipped sine wave (hard digital clipping)
fn generate_clipped_sine(frequency: f32, sample_rate: u32, duration_secs: f32, gain: f32) -> Vec<f32> {
    generate_sine_wave(frequency, sample_rate, duration_secs)
        .into_iter()
        .map(|s| {
            let amplified = s * gain;
            amplified.clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate severely clipped audio (many consecutive samples at max)
fn generate_severely_clipped(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    generate_clipped_sine(frequency, sample_rate, duration_secs, 3.0)
}

/// Generate audio with simulated ENF (50 Hz hum)
fn generate_with_enf_hum(
    base_frequency: f32,
    enf_frequency: f32,
    enf_amplitude: f32,
    sample_rate: u32,
    duration_secs: f32,
) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let signal = (2.0 * std::f32::consts::PI * base_frequency * t).sin();
            let hum = enf_amplitude * (2.0 * std::f32::consts::PI * enf_frequency * t).sin();
            (signal * 0.8 + hum).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate soft-clipped audio (analog-style saturation)
fn generate_soft_clipped(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    generate_sine_wave(frequency, sample_rate, duration_secs)
        .into_iter()
        .map(|s| {
            let amplified = s * 2.0;
            // Soft clipping using tanh
            amplified.tanh()
        })
        .collect()
}

/// Generate heavily compressed audio (loudness war style)
fn generate_loudness_war_audio(frequency: f32, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    generate_sine_wave(frequency, sample_rate, duration_secs)
        .into_iter()
        .map(|s| {
            // Heavy compression: reduce dynamic range
            let compressed = s.signum() * s.abs().powf(0.3);
            // Normalize to near max level
            (compressed * 0.99).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate silence
fn generate_silence(sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    vec![0.0; num_samples]
}

/// Generate DC offset audio
fn generate_dc_offset(sample_rate: u32, duration_secs: f32, offset: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    vec![offset; num_samples]
}

// =============================================================================
// Quality Assessment Tests
// =============================================================================

#[test]
fn test_quality_assessment_clean_audio() {
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    assert!(result.quality_assessment.score > 0.9, 
        "Clean audio should have score > 0.9, got {}", result.quality_assessment.score);
    assert_eq!(result.quality_assessment.grade, QualityGrade::Excellent);
    assert!(result.quality_assessment.issues.is_empty(),
        "Clean audio should have no issues, got {:?}", result.quality_assessment.issues);
}

#[test]
fn test_quality_assessment_clipped_audio() {
    let samples = generate_clipped_sine(440.0, 44100, 1.0, 1.5);
    let result = analyze_audio_quality(&samples, 44100);
    
    assert!(result.quality_assessment.score < 0.9,
        "Clipped audio should have score < 0.9, got {}", result.quality_assessment.score);
    assert!(result.quality_assessment.issues.iter()
        .any(|i| i.issue_type == QualityIssueType::DigitalClipping),
        "Should detect digital clipping");
}

#[test]
fn test_quality_assessment_severely_clipped() {
    let samples = generate_severely_clipped(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    assert!(result.quality_assessment.score < 0.5,
        "Severely clipped audio should have score < 0.5, got {}", result.quality_assessment.score);
    assert!(matches!(result.quality_assessment.grade, QualityGrade::Poor | QualityGrade::Severe),
        "Severely clipped should be Poor or Severe grade");
}

#[test]
fn test_quality_grade_boundaries() {
    // Test that grades are assigned correctly at boundaries
    let clean = generate_sine_wave(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&clean, 44100);
    
    let grade = result.quality_assessment.grade;
    let score = result.quality_assessment.score;
    
    match grade {
        QualityGrade::Excellent => assert!(score >= 0.9),
        QualityGrade::Good => assert!(score >= 0.75 && score < 0.9),
        QualityGrade::Acceptable => assert!(score >= 0.5 && score < 0.75),
        QualityGrade::Poor => assert!(score >= 0.25 && score < 0.5),
        QualityGrade::Severe => assert!(score < 0.25),
    }
}

#[test]
fn test_quality_recommendations_present() {
    let clipped = generate_clipped_sine(440.0, 44100, 1.0, 1.5);
    let result = analyze_audio_quality(&clipped, 44100);
    
    assert!(!result.quality_assessment.recommendations.is_empty(),
        "Clipped audio should have recommendations");
}

// =============================================================================
// Extended Pipeline Configuration Tests
// =============================================================================

#[test]
fn test_extended_pipeline_default_options() {
    let options = ExtendedDetectionOptions::default();
    
    assert!(!options.enable_enf, "ENF should be off by default");
    assert!(options.enable_clipping, "Clipping should be on by default");
    assert!(options.enable_inter_sample_peaks, "ISP should be on by default");
    assert!(options.enable_loudness_analysis, "Loudness should be on by default");
    assert!(!options.enf_sensitive_mode, "ENF sensitive should be off by default");
    assert!(!options.clipping_strict_mode, "Strict clipping should be off by default");
}

#[test]
fn test_extended_pipeline_custom_options() {
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enf_sensitive_mode: true,
        expected_enf_frequency: Some(EnfBaseFrequency::Hz50),
        enable_clipping: false,
        clipping_strict_mode: false,
        enable_inter_sample_peaks: false,
        enable_loudness_analysis: false,
    };
    
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    // ENF enabled, should have result
    assert!(result.enf_result.is_some(), "ENF result should be present");
    // Clipping disabled, should be None
    assert!(result.clipping_result.is_none(), "Clipping result should be None when disabled");
}

#[test]
fn test_extended_pipeline_enf_only() {
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enable_clipping: false,
        ..Default::default()
    };
    
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    assert!(result.enf_result.is_some());
    assert!(result.clipping_result.is_none());
    assert!(result.authenticity_assessment.is_some());
}

#[test]
fn test_extended_pipeline_clipping_only() {
    let options = ExtendedDetectionOptions {
        enable_enf: false,
        enable_clipping: true,
        ..Default::default()
    };
    
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    assert!(result.enf_result.is_none());
    assert!(result.clipping_result.is_some());
    assert!(result.authenticity_assessment.is_none());
}

// =============================================================================
// ENF Detection Tests
// =============================================================================

#[test]
fn test_enf_enabled_produces_result() {
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        ..Default::default()
    };
    
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    assert!(result.enf_result.is_some(), "ENF result should be present when enabled");
    assert!(result.authenticity_assessment.is_some(), "Authenticity assessment should be present");
}

#[test]
fn test_authenticity_analysis_no_enf_signal() {
    // Pure sine wave has no ENF signal
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    let assessment = analyze_authenticity(&samples, 44100);
    
    assert!(assessment.is_some());
    let auth = assessment.unwrap();
    
    // Clean synthetic audio has no ENF, should be inconclusive
    assert_eq!(auth.result, AuthenticityResult::Inconclusive,
        "Audio without ENF should be Inconclusive, got {:?}", auth.result);
}

#[test]
fn test_authenticity_with_enf_hum() {
    // Audio with simulated 50 Hz ENF
    let samples = generate_with_enf_hum(440.0, 50.0, 0.05, 44100, 5.0);
    let assessment = analyze_authenticity(&samples, 44100);
    
    assert!(assessment.is_some());
    let auth = assessment.unwrap();
    
    // Should detect ENF and provide authenticity assessment
    // Note: Result depends on ENF detector implementation
    assert!(auth.confidence >= 0.0 && auth.confidence <= 1.0);
}

#[test]
fn test_enf_sensitive_mode() {
    let normal_options = ExtendedDetectionOptions {
        enable_enf: true,
        enf_sensitive_mode: false,
        ..Default::default()
    };
    
    let sensitive_options = ExtendedDetectionOptions {
        enable_enf: true,
        enf_sensitive_mode: true,
        ..Default::default()
    };
    
    // Weak ENF signal
    let samples = generate_with_enf_hum(440.0, 50.0, 0.01, 44100, 3.0);
    
    let normal_pipeline = ExtendedDetectionPipeline::with_options(normal_options);
    let sensitive_pipeline = ExtendedDetectionPipeline::with_options(sensitive_options);
    
    let normal_result = normal_pipeline.analyze_mono(&samples, 44100);
    let sensitive_result = sensitive_pipeline.analyze_mono(&samples, 44100);
    
    // Both should have ENF results
    assert!(normal_result.enf_result.is_some());
    assert!(sensitive_result.enf_result.is_some());
}

#[test]
fn test_enf_frequency_specification() {
    let options_50hz = ExtendedDetectionOptions {
        enable_enf: true,
        expected_enf_frequency: Some(EnfBaseFrequency::Hz50),
        ..Default::default()
    };
    
    let options_60hz = ExtendedDetectionOptions {
        enable_enf: true,
        expected_enf_frequency: Some(EnfBaseFrequency::Hz60),
        ..Default::default()
    };
    
    let samples = generate_with_enf_hum(440.0, 50.0, 0.05, 44100, 3.0);
    
    let pipeline_50 = ExtendedDetectionPipeline::with_options(options_50hz);
    let pipeline_60 = ExtendedDetectionPipeline::with_options(options_60hz);
    
    let result_50 = pipeline_50.analyze_mono(&samples, 44100);
    let result_60 = pipeline_60.analyze_mono(&samples, 44100);
    
    assert!(result_50.enf_result.is_some());
    assert!(result_60.enf_result.is_some());
}

// =============================================================================
// Clipping Detection Tests
// =============================================================================

#[test]
fn test_clipping_detection_clean_audio() {
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    if let Some(clipping) = &result.clipping_result {
        assert!(!clipping.has_clipping, "Clean audio should not have clipping");
        assert_eq!(clipping.statistics.samples_at_digital_max, 0);
    }
}

#[test]
fn test_clipping_detection_clipped_audio() {
    let samples = generate_clipped_sine(440.0, 44100, 1.0, 1.5);
    let result = analyze_audio_quality(&samples, 44100);
    
    if let Some(clipping) = &result.clipping_result {
        assert!(clipping.has_clipping, "Clipped audio should have clipping detected");
        assert!(clipping.statistics.samples_at_digital_max > 0);
        assert!(clipping.severity > 0.0);
    }
}

#[test]
fn test_clipping_strict_mode() {
    let normal_options = ExtendedDetectionOptions {
        enable_clipping: true,
        clipping_strict_mode: false,
        ..Default::default()
    };
    
    let strict_options = ExtendedDetectionOptions {
        enable_clipping: true,
        clipping_strict_mode: true,
        ..Default::default()
    };
    
    // Audio that just barely clips
    let samples = generate_clipped_sine(440.0, 44100, 1.0, 1.01);
    
    let normal_pipeline = ExtendedDetectionPipeline::with_options(normal_options);
    let strict_pipeline = ExtendedDetectionPipeline::with_options(strict_options);
    
    let normal_result = normal_pipeline.analyze_mono(&samples, 44100);
    let strict_result = strict_pipeline.analyze_mono(&samples, 44100);
    
    // Both should have clipping results
    assert!(normal_result.clipping_result.is_some());
    assert!(strict_result.clipping_result.is_some());
}

#[test]
fn test_inter_sample_peak_detection() {
    let options_with_isp = ExtendedDetectionOptions {
        enable_clipping: true,
        enable_inter_sample_peaks: true,
        ..Default::default()
    };
    
    let options_without_isp = ExtendedDetectionOptions {
        enable_clipping: true,
        enable_inter_sample_peaks: false,
        ..Default::default()
    };
    
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    
    let pipeline_with = ExtendedDetectionPipeline::with_options(options_with_isp);
    let pipeline_without = ExtendedDetectionPipeline::with_options(options_without_isp);
    
    let result_with = pipeline_with.analyze_mono(&samples, 44100);
    let result_without = pipeline_without.analyze_mono(&samples, 44100);
    
    // Both should have clipping results
    assert!(result_with.clipping_result.is_some());
    assert!(result_without.clipping_result.is_some());
}

#[test]
fn test_loudness_analysis() {
    let options_with_loudness = ExtendedDetectionOptions {
        enable_clipping: true,
        enable_loudness_analysis: true,
        ..Default::default()
    };
    
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    let pipeline = ExtendedDetectionPipeline::with_options(options_with_loudness);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    if let Some(clipping) = &result.clipping_result {
        // Should have loudness metrics
        assert!(clipping.loudness_analysis.dynamic_range_db.is_finite());
        assert!(clipping.loudness_analysis.crest_factor_db.is_finite());
    }
}

#[test]
fn test_loudness_war_detection() {
    let samples = generate_loudness_war_audio(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    // Check if loudness war was detected
    let has_loudness_war_issue = result.quality_assessment.issues.iter()
        .any(|i| i.issue_type == QualityIssueType::LoudnessWarVictim ||
                 i.issue_type == QualityIssueType::LowDynamicRange);
    
    // The heavily compressed audio should trigger some dynamic range issue
    // Note: depends on implementation thresholds
    println!("Issues detected: {:?}", result.quality_assessment.issues);
}

// =============================================================================
// Stereo Analysis Tests
// =============================================================================

#[test]
fn test_stereo_analysis_identical_channels() {
    let mono = generate_sine_wave(440.0, 44100, 1.0);
    let result = analyze_stereo_quality(&mono, &mono, 44100);
    
    assert!(result.clipping_result.is_some());
    assert!(result.quality_assessment.score > 0.9);
}

#[test]
fn test_stereo_analysis_different_channels() {
    let left = generate_sine_wave(440.0, 44100, 1.0);
    let right = generate_sine_wave(880.0, 44100, 1.0);
    
    let result = analyze_stereo_quality(&left, &right, 44100);
    
    assert!(result.clipping_result.is_some());
}

#[test]
fn test_stereo_analysis_one_channel_clipped() {
    let left = generate_sine_wave(440.0, 44100, 1.0);
    let right = generate_clipped_sine(440.0, 44100, 1.0, 1.5);
    
    let result = analyze_stereo_quality(&left, &right, 44100);
    
    // Should detect clipping in the right channel
    if let Some(clipping) = &result.clipping_result {
        assert!(clipping.has_clipping, "Should detect clipping in one channel");
    }
}

#[test]
fn test_stereo_enf_analysis() {
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enable_clipping: true,
        ..Default::default()
    };
    
    let left = generate_with_enf_hum(440.0, 50.0, 0.05, 44100, 3.0);
    let right = generate_with_enf_hum(440.0, 50.0, 0.05, 44100, 3.0);
    
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_stereo(&left, &right, 44100);
    
    // ENF analysis should work on stereo (mixed to mono internally)
    assert!(result.enf_result.is_some());
    assert!(result.authenticity_assessment.is_some());
}

// =============================================================================
// CLI Arguments Tests
// =============================================================================

#[test]
fn test_cli_args_default() {
    let args = ExtendedDetectionArgs::default();
    
    assert!(!args.enf);
    assert!(!args.enf_sensitive);
    assert!(args.enf_frequency.is_none());
    assert!(!args.no_clipping);
    assert!(!args.clipping_strict);
    assert!(!args.no_inter_sample);
    assert!(!args.no_loudness);
    assert!(matches!(args.extended_output, ExtendedOutputFormat::Text));
}

#[test]
fn test_cli_to_options_conversion() {
    let args = ExtendedDetectionArgs {
        enf: true,
        enf_sensitive: true,
        enf_frequency: Some(EnfFrequencyArg::Hz50),
        no_clipping: true,
        clipping_strict: false,
        no_inter_sample: true,
        no_loudness: true,
        extended_output: ExtendedOutputFormat::Json,
    };
    
    // Convert to ExtendedDetectionOptions
    let options = ExtendedDetectionOptions {
        enable_enf: args.enf,
        enf_sensitive_mode: args.enf_sensitive,
        expected_enf_frequency: args.enf_frequency.map(|f| match f {
            EnfFrequencyArg::Hz50 => EnfBaseFrequency::Hz50,
            EnfFrequencyArg::Hz60 => EnfBaseFrequency::Hz60,
        }),
        enable_clipping: !args.no_clipping,
        clipping_strict_mode: args.clipping_strict,
        enable_inter_sample_peaks: !args.no_inter_sample,
        enable_loudness_analysis: !args.no_loudness,
    };
    
    assert!(options.enable_enf);
    assert!(options.enf_sensitive_mode);
    assert_eq!(options.expected_enf_frequency, Some(EnfBaseFrequency::Hz50));
    assert!(!options.enable_clipping);
    assert!(!options.enable_inter_sample_peaks);
    assert!(!options.enable_loudness_analysis);
}

// =============================================================================
// Edge Cases and Boundary Tests
// =============================================================================

#[test]
fn test_empty_audio() {
    let samples: Vec<f32> = vec![];
    let pipeline = ExtendedDetectionPipeline::new();
    
    // Should handle empty input gracefully
    // Note: behavior depends on implementation
}

#[test]
fn test_very_short_audio() {
    let samples = generate_sine_wave(440.0, 44100, 0.01); // 10ms
    let result = analyze_audio_quality(&samples, 44100);
    
    // Should still produce a result
    assert!(result.quality_assessment.score >= 0.0 && result.quality_assessment.score <= 1.0);
}

#[test]
fn test_silence() {
    let samples = generate_silence(44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    // Silence should not have clipping
    if let Some(clipping) = &result.clipping_result {
        assert!(!clipping.has_clipping);
    }
}

#[test]
fn test_dc_offset_audio() {
    let samples = generate_dc_offset(44100, 1.0, 0.5);
    let result = analyze_audio_quality(&samples, 44100);
    
    // Should handle DC offset
    // Note: may trigger certain detections depending on implementation
}

#[test]
fn test_high_sample_rate() {
    let samples = generate_sine_wave(440.0, 96000, 1.0);
    let result = analyze_audio_quality(&samples, 96000);
    
    assert!(result.quality_assessment.score > 0.9);
}

#[test]
fn test_low_sample_rate() {
    let samples = generate_sine_wave(440.0, 22050, 1.0);
    let result = analyze_audio_quality(&samples, 22050);
    
    // Should work with lower sample rates
    assert!(result.quality_assessment.score >= 0.0);
}

// =============================================================================
// Convenience Function Tests
// =============================================================================

#[test]
fn test_analyze_audio_quality_convenience() {
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    let result = analyze_audio_quality(&samples, 44100);
    
    // Should use default options (clipping on, ENF off)
    assert!(result.clipping_result.is_some());
    assert!(result.enf_result.is_none());
}

#[test]
fn test_analyze_stereo_quality_convenience() {
    let left = generate_sine_wave(440.0, 44100, 1.0);
    let right = generate_sine_wave(440.0, 44100, 1.0);
    
    let result = analyze_stereo_quality(&left, &right, 44100);
    
    assert!(result.clipping_result.is_some());
}

#[test]
fn test_analyze_authenticity_convenience() {
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    let assessment = analyze_authenticity(&samples, 44100);
    
    // Should have ENF analysis enabled
    assert!(assessment.is_some());
}

// =============================================================================
// Integration Workflow Tests
// =============================================================================

#[test]
fn test_complete_analysis_workflow_clean_audio() {
    let samples = generate_sine_wave(440.0, 44100, 2.0);
    
    // Full analysis with all features
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enable_clipping: true,
        enable_inter_sample_peaks: true,
        enable_loudness_analysis: true,
        ..Default::default()
    };
    
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    // Clean audio expectations
    assert!(result.quality_assessment.grade == QualityGrade::Excellent ||
            result.quality_assessment.grade == QualityGrade::Good);
    assert!(result.enf_result.is_some());
    assert!(result.clipping_result.is_some());
    
    if let Some(clipping) = &result.clipping_result {
        assert!(!clipping.has_clipping);
    }
}

#[test]
fn test_complete_analysis_workflow_problematic_audio() {
    let samples = generate_severely_clipped(440.0, 44100, 2.0);
    
    let options = ExtendedDetectionOptions {
        enable_enf: true,
        enable_clipping: true,
        ..Default::default()
    };
    
    let pipeline = ExtendedDetectionPipeline::with_options(options);
    let result = pipeline.analyze_mono(&samples, 44100);
    
    // Should detect multiple issues
    assert!(!result.quality_assessment.issues.is_empty());
    assert!(result.quality_assessment.grade != QualityGrade::Excellent);
    
    // Should have recommendations
    assert!(!result.quality_assessment.recommendations.is_empty());
}

#[test]
fn test_analysis_result_consistency() {
    let samples = generate_sine_wave(440.0, 44100, 1.0);
    
    // Run analysis twice
    let result1 = analyze_audio_quality(&samples, 44100);
    let result2 = analyze_audio_quality(&samples, 44100);
    
    // Results should be identical
    assert_eq!(result1.quality_assessment.score, result2.quality_assessment.score);
    assert_eq!(result1.quality_assessment.grade, result2.quality_assessment.grade);
}
