// src/cli/output.rs
//
// Output formatting for CLI

use colorful::Colorful;
use crate::core::{QualityReport, DetectedDefect, DefectType};

/// Print human-readable report
pub fn print_report(report: &QualityReport, verbose: bool) {
    println!("  Sample Rate: {} Hz", report.sample_rate);
    println!("  Bit Depth: {} bit (claimed: {})", 
        report.actual_bit_depth, report.claimed_bit_depth);
    println!("  Channels: {}", report.channels);
    println!("  Duration: {:.2}s", report.duration_secs);
    println!("  Frequency Cutoff: {:.0} Hz", report.frequency_cutoff);
    println!("  Dynamic Range: {:.1} dB", report.dynamic_range);

    let score_str = format!("{:.0}%", report.quality_score * 100.0);
    let quality_label = if report.is_likely_lossless {
        format!("Quality Score: {} (likely lossless)", score_str).green()
    } else {
        format!("Quality Score: {} (likely transcoded)", score_str).yellow()
    };
    println!("  {}", quality_label);

    if report.defects.is_empty() {
        println!("  Status: {}", "✓ CLEAN - No issues detected".green());
    } else {
        println!("  Status: {}", "✗ ISSUES DETECTED".red());
        for defect in &report.defects {
            println!("    • {}", format_defect(defect).yellow());
            
            if verbose {
                for evidence in &defect.evidence {
                    let evidence_clone = evidence.clone();
                    println!("      - {}", evidence_clone.dark_gray());
                }
            }
        }
    }
    
    // Verbose output for enhanced analysis
    if verbose {
        // Dithering analysis details
        if let Some(ref dither) = report.dither_analysis {
            println!("\n  {} Dithering Analysis:", "▸".cyan());
            println!("    Algorithm: {}", dither.algorithm);
            println!("    Scale: {}", dither.scale);
            println!("    Effective bit depth: {} (container: {})", 
                dither.effective_bit_depth, dither.container_bit_depth);
            println!("    Is bit-reduced: {}", if dither.is_bit_reduced { "Yes" } else { "No" });
            println!("    Noise floor: {:.1} dBFS", dither.noise_floor_db);
            println!("    Algorithm confidence: {:.0}%", dither.algorithm_confidence * 100.0);
            
            // Noise spectrum details
            println!("    Noise spectrum:");
            println!("      Spectral tilt: {:.1} dB/octave", dither.noise_spectrum.spectral_tilt);
            println!("      Low band ratio: {:.1}%", dither.noise_spectrum.low_band_ratio * 100.0);
            println!("      Mid band ratio: {:.1}%", dither.noise_spectrum.mid_band_ratio * 100.0);
            println!("      High band ratio: {:.1}%", dither.noise_spectrum.high_band_ratio * 100.0);
            if let Some(peak) = dither.noise_spectrum.shaping_peak_hz {
                println!("      Shaping peak: {:.0} Hz", peak);
            }
        }
        
        // Resampling analysis details
        if let Some(ref resample) = report.resample_analysis {
            println!("\n  {} Resampling Analysis:", "▸".cyan());
            println!("    Is resampled: {}", if resample.is_resampled { "Yes" } else { "No" });
            if resample.is_resampled {
                println!("    Current rate: {} Hz", resample.current_sample_rate);
                if let Some(orig) = resample.original_sample_rate {
                    println!("    Original rate: {} Hz", orig);
                }
                println!("    Direction: {:?}", resample.direction);
                println!("    Engine: {}", resample.engine);
                println!("    Quality tier: {}", resample.quality);
                println!("    Detection confidence: {:.0}%", resample.confidence * 100.0);
            }
            println!("    Filter characteristics:");
            println!("      Cutoff ratio: {:.1}%", resample.filter_cutoff_ratio * 100.0);
            println!("      Transition band: {:.0} Hz", resample.transition_band_hz);
            println!("      Stopband attenuation: {:.1} dB", resample.stopband_attenuation_db);
            println!("      Passband ripple: {:.2} dB", resample.passband_ripple_db);
            if resample.has_nyquist_null {
                if let Some(freq) = resample.null_frequency_hz {
                    println!("      Nyquist null at: {:.0} Hz", freq);
                }
            }
        }
    }
}

/// Print JSON output
pub fn print_json(report: &QualityReport) -> anyhow::Result<()> {
    let json_output = serde_json::json!({
        "sample_rate": report.sample_rate,
        "channels": report.channels,
        "claimed_bit_depth": report.claimed_bit_depth,
        "actual_bit_depth": report.actual_bit_depth,
        "duration_secs": report.duration_secs,
        "frequency_cutoff": report.frequency_cutoff,
        "quality_score": report.quality_score,
        "is_likely_lossless": report.is_likely_lossless,
        "defects": report.defects.iter().map(|d| {
            serde_json::json!({
                "type": format_defect_type(&d.defect_type),
                "confidence": d.confidence,
                "evidence": d.evidence,
                "details": format_defect_details(&d.defect_type),
            })
        }).collect::<Vec<_>>(),
        "dither_analysis": report.dither_analysis.as_ref().map(|d| {
            serde_json::json!({
                "algorithm": format!("{}", d.algorithm),
                "scale": format!("{}", d.scale),
                "effective_bit_depth": d.effective_bit_depth,
                "container_bit_depth": d.container_bit_depth,
                "is_bit_reduced": d.is_bit_reduced,
                "noise_floor_db": d.noise_floor_db,
                "algorithm_confidence": d.algorithm_confidence,
                "noise_spectrum": {
                    "spectral_tilt": d.noise_spectrum.spectral_tilt,
                    "low_band_ratio": d.noise_spectrum.low_band_ratio,
                    "mid_band_ratio": d.noise_spectrum.mid_band_ratio,
                    "high_band_ratio": d.noise_spectrum.high_band_ratio,
                    "shaping_peak_hz": d.noise_spectrum.shaping_peak_hz,
                }
            })
        }),
        "resample_analysis": report.resample_analysis.as_ref().map(|r| {
            serde_json::json!({
                "is_resampled": r.is_resampled,
                "confidence": r.confidence,
                "current_sample_rate": r.current_sample_rate,
                "original_sample_rate": r.original_sample_rate,
                "direction": format!("{:?}", r.direction),
                "engine": format!("{}", r.engine),
                "quality": format!("{}", r.quality),
                "filter_cutoff_ratio": r.filter_cutoff_ratio,
                "transition_band_hz": r.transition_band_hz,
                "stopband_attenuation_db": r.stopband_attenuation_db,
                "passband_ripple_db": r.passband_ripple_db,
                "has_nyquist_null": r.has_nyquist_null,
                "null_frequency_hz": r.null_frequency_hz,
            })
        }),
    });
    println!("{}", serde_json::to_string_pretty(&json_output)?);
    Ok(())
}

fn format_defect(defect: &DetectedDefect) -> String {
    let conf_str = format!(" ({:.0}% confidence)", defect.confidence * 100.0);
    
    match &defect.defect_type {
        DefectType::Mp3Transcode { cutoff_hz, estimated_bitrate } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" (~{}kbps)", b))
                .unwrap_or_default();
            format!("MP3 transcode{} - cutoff at {} Hz{}", bitrate_str, cutoff_hz, conf_str)
        },
        DefectType::OggVorbisTranscode { cutoff_hz, estimated_quality } => {
            let quality_str = estimated_quality
                .map(|q| format!(" (~q{:.0})", q))
                .unwrap_or_default();
            format!("Ogg Vorbis transcode{} - cutoff at {} Hz{}", quality_str, cutoff_hz, conf_str)
        },
        DefectType::AacTranscode { cutoff_hz, estimated_bitrate } => {
            let bitrate_str = estimated_bitrate
                .map(|b| format!(" (~{}kbps)", b))
                .unwrap_or_default();
            format!("AAC transcode{} - cutoff at {} Hz{}", bitrate_str, cutoff_hz, conf_str)
        },
        DefectType::OpusTranscode { cutoff_hz, mode } => {
            format!("Opus transcode ({}) - cutoff at {} Hz{}", mode, cutoff_hz, conf_str)
        },
        DefectType::BitDepthMismatch { claimed, actual, .. } => {
            format!("Bit depth mismatch: claims {}-bit, actually {}-bit{}", claimed, actual, conf_str)
        },
        DefectType::Upsampled { from, to, .. } => {
            format!("Upsampled from {} Hz to {} Hz{}", from, to, conf_str)
        },
        DefectType::SpectralArtifacts { artifact_score } => {
            format!("Spectral artifacts detected (score: {:.2}){}", artifact_score, conf_str)
        },
        DefectType::JointStereo { .. } => {
            format!("Joint stereo encoding detected{}", conf_str)
        },
        DefectType::PreEcho { score } => {
            format!("Pre-echo artifacts (score: {:.2}){}", score, conf_str)
        },
        DefectType::PhaseDiscontinuities { score } => {
            format!("Phase discontinuities (score: {:.2}){}", score, conf_str)
        },
        DefectType::Clipping { percentage } => {
            format!("Clipping detected ({:.3}% of samples){}", percentage * 100.0, conf_str)
        },
        DefectType::InterSampleOvers { count, max_level_db } => {
            format!("{} inter-sample overs (max: {:.1} dB){}", count, max_level_db, conf_str)
        },
        DefectType::LowQuality { description } => {
            format!("Low quality: {}{}", description, conf_str)
        },
        DefectType::DitheringDetected { algorithm, scale, effective_bits, container_bits } => {
            format!(
                "Bit depth reduction detected: {}-bit → {}-bit with {} dithering (scale: {}){}",
                container_bits, effective_bits, algorithm, scale, conf_str
            )
        },
        DefectType::ResamplingDetected { original_rate, current_rate, engine, quality } => {
            format!(
                "Resampling detected: {} Hz → {} Hz using {} ({} quality){}",
                original_rate, current_rate, engine, quality, conf_str
            )
        },
    }
}

fn format_defect_type(defect: &DefectType) -> String {
    match defect {
        DefectType::Mp3Transcode { .. } => "MP3 Transcode".to_string(),
        DefectType::OggVorbisTranscode { .. } => "Ogg Vorbis Transcode".to_string(),
        DefectType::AacTranscode { .. } => "AAC Transcode".to_string(),
        DefectType::OpusTranscode { .. } => "Opus Transcode".to_string(),
        DefectType::BitDepthMismatch { .. } => "Bit Depth Mismatch".to_string(),
        DefectType::Upsampled { .. } => "Upsampled".to_string(),
        DefectType::SpectralArtifacts { .. } => "Spectral Artifacts".to_string(),
        DefectType::JointStereo { .. } => "Joint Stereo".to_string(),
        DefectType::PreEcho { .. } => "Pre-Echo".to_string(),
        DefectType::PhaseDiscontinuities { .. } => "Phase Discontinuities".to_string(),
        DefectType::Clipping { .. } => "Clipping".to_string(),
        DefectType::InterSampleOvers { .. } => "Inter-Sample Overs".to_string(),
        DefectType::LowQuality { .. } => "Low Quality".to_string(),
        DefectType::DitheringDetected { .. } => "Dithering Detected".to_string(),
        DefectType::ResamplingDetected { .. } => "Resampling Detected".to_string(),
    }
}

fn format_defect_details(defect: &DefectType) -> serde_json::Value {
    match defect {
        DefectType::Mp3Transcode { cutoff_hz, estimated_bitrate } => {
            serde_json::json!({
                "cutoff_hz": cutoff_hz,
                "estimated_bitrate": estimated_bitrate,
            })
        },
        DefectType::OggVorbisTranscode { cutoff_hz, estimated_quality } => {
            serde_json::json!({
                "cutoff_hz": cutoff_hz,
                "estimated_quality": estimated_quality,
            })
        },
        DefectType::AacTranscode { cutoff_hz, estimated_bitrate } => {
            serde_json::json!({
                "cutoff_hz": cutoff_hz,
                "estimated_bitrate": estimated_bitrate,
            })
        },
        DefectType::OpusTranscode { cutoff_hz, mode } => {
            serde_json::json!({
                "cutoff_hz": cutoff_hz,
                "mode": mode,
            })
        },
        DefectType::BitDepthMismatch { claimed, actual, method } => {
            serde_json::json!({
                "claimed": claimed,
                "actual": actual,
                "method": method,
            })
        },
        DefectType::Upsampled { from, to, method } => {
            serde_json::json!({
                "from": from,
                "to": to,
                "method": method,
            })
        },
        DefectType::DitheringDetected { algorithm, scale, effective_bits, container_bits } => {
            serde_json::json!({
                "algorithm": algorithm,
                "scale": scale,
                "effective_bits": effective_bits,
                "container_bits": container_bits,
            })
        },
        DefectType::ResamplingDetected { original_rate, current_rate, engine, quality } => {
            serde_json::json!({
                "original_rate": original_rate,
                "current_rate": current_rate,
                "engine": engine,
                "quality": quality,
            })
        },
        _ => serde_json::json!({}),
    }
}
