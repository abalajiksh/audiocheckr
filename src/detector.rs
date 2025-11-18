// src/detector.rs
use anyhow::Result;
use rustfft::{FftPlanner, num_complex::Complex};
use crate::decoder::AudioData;

#[derive(Debug)]
pub enum DefectType {
    Mp3Transcode { cutoff_hz: u32 },
    OggVorbisTranscode { cutoff_hz: u32 },
    AacTranscode { cutoff_hz: u32 },
    OpusTranscode { cutoff_hz: u32, mode: String },
    BitDepthMismatch { claimed: u32, actual: u32 },
    Upsampled { from: u32, to: u32 },
    SpectralArtifacts,
    LowQuality,
}

#[derive(Debug)]
pub struct QualityReport {
    pub sample_rate: u32,
    pub channels: usize,
    pub claimed_bit_depth: u32,
    pub actual_bit_depth: u32,
    pub duration_secs: f64,
    pub frequency_cutoff: f32,
    pub dynamic_range: f32,
    pub noise_floor: f32,
    pub peak_amplitude: f32,
    pub spectral_rolloff: f32,
    pub defects: Vec<DefectType>,
}

pub fn detect_quality_issues(
    audio: &AudioData,
    expected_bit_depth: u32,
    check_upsampling: bool,
) -> Result<QualityReport> {
    let mut defects = Vec::new();

    // Analyze frequency spectrum
    let (cutoff, rolloff, has_artifacts) = analyze_frequency_spectrum(audio)?;
    
    // Opus has specific cutoff frequencies based on bandwidth mode
    if cutoff < 8500.0 && cutoff > 7500.0 {
        // Opus wideband mode (WB) - 8 kHz cutoff
        defects.push(DefectType::OpusTranscode { 
            cutoff_hz: cutoff as u32,
            mode: "Wideband (8kHz)".to_string()
        });
    } else if cutoff < 12500.0 && cutoff > 11500.0 {
        // Opus super-wideband mode (SWB) - 12 kHz cutoff
        defects.push(DefectType::OpusTranscode { 
            cutoff_hz: cutoff as u32,
            mode: "Super-wideband (12kHz)".to_string()
        });
    } else if cutoff < 16500.0 && cutoff > 15000.0 {
        // Could be MP3 or low-bitrate Opus
        if detect_opus_artifacts(audio)? {
            defects.push(DefectType::OpusTranscode { 
                cutoff_hz: cutoff as u32,
                mode: "Low bitrate".to_string()
            });
        } else if cutoff < 15500.0 {
            defects.push(DefectType::Mp3Transcode { cutoff_hz: cutoff as u32 });
        } else if cutoff < 16000.0 {
            defects.push(DefectType::OggVorbisTranscode { cutoff_hz: cutoff as u32 });
        }
    } else if cutoff < 18000.0 {
        // AAC typically has higher cutoff but still below full spectrum
        defects.push(DefectType::AacTranscode { cutoff_hz: cutoff as u32 });
    } else if cutoff < 20500.0 && cutoff > 19000.0 {
        // Opus fullband mode (FB) - ~20 kHz cutoff
        if detect_opus_artifacts(audio)? {
            defects.push(DefectType::OpusTranscode { 
                cutoff_hz: cutoff as u32,
                mode: "Fullband (20kHz)".to_string()
            });
        }
    }

    if has_artifacts {
        defects.push(DefectType::SpectralArtifacts);
    }

    // Analyze dynamic range and bit depth
    let (dynamic_range, noise_floor, peak_amp) = analyze_dynamic_range(audio);
    let actual_bit_depth = estimate_bit_depth(dynamic_range);

    if actual_bit_depth < expected_bit_depth {
        defects.push(DefectType::BitDepthMismatch {
            claimed: audio.bit_depth,
            actual: actual_bit_depth,
        });
    }

    // Check for upsampling
    if check_upsampling {
        if let Some(original_rate) = detect_upsampling(audio, cutoff) {
            defects.push(DefectType::Upsampled {
                from: original_rate,
                to: audio.sample_rate,
            });
        }
    }

    Ok(QualityReport {
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        claimed_bit_depth: audio.bit_depth,
        actual_bit_depth,
        duration_secs: audio.duration_secs,
        frequency_cutoff: cutoff,
        dynamic_range,
        noise_floor,
        peak_amplitude: peak_amp,
        spectral_rolloff: rolloff,
        defects,
    })
}

fn analyze_frequency_spectrum(audio: &AudioData) -> Result<(f32, f32, bool)> {
    let fft_size = 8192;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Take middle section of audio to avoid edge effects
    let start = audio.samples.len() / 2;
    let end = (start + fft_size * audio.channels).min(audio.samples.len());
    
    if end - start < fft_size {
        return Ok((audio.sample_rate as f32 / 2.0, audio.sample_rate as f32 / 2.0, false));
    }

    // Extract mono channel for analysis
    let mut signal: Vec<Complex<f32>> = audio.samples[start..end]
        .chunks(audio.channels)
        .take(fft_size)
        .map(|chunk| Complex::new(chunk[0], 0.0))
        .collect();

    // Apply Hann window
    for (i, sample) in signal.iter_mut().enumerate() {
        let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
        *sample *= window;
    }

    fft.process(&mut signal);

    // Calculate magnitude spectrum
    let magnitudes: Vec<f32> = signal[..fft_size / 2]
        .iter()
        .map(|c| (c.re * c.re + c.im * c.im).sqrt())
        .collect();

    // Find frequency cutoff (where energy drops significantly)
    let cutoff = find_frequency_cutoff(&magnitudes, audio.sample_rate);
    
    // Find spectral rolloff (95% energy point)
    let rolloff = find_spectral_rolloff(&magnitudes, audio.sample_rate);
    
    // Detect artifacts (irregularities in spectrum)
    let has_artifacts = detect_spectral_artifacts(&magnitudes);

    Ok((cutoff, rolloff, has_artifacts))
}

fn find_frequency_cutoff(magnitudes: &[f32], sample_rate: u32) -> f32 {
    let threshold = magnitudes.iter().cloned().fold(0.0f32, f32::max) * 0.01; // 1% of peak
    
    // Search from high to low frequency
    for i in (0..magnitudes.len()).rev() {
        if magnitudes[i] > threshold {
            let freq = i as f32 * sample_rate as f32 / (2.0 * magnitudes.len() as f32);
            return freq;
        }
    }
    
    sample_rate as f32 / 2.0
}

fn find_spectral_rolloff(magnitudes: &[f32], sample_rate: u32) -> f32 {
    let total_energy: f32 = magnitudes.iter().map(|m| m * m).sum();
    let threshold = total_energy * 0.95;
    
    let mut cumulative = 0.0;
    for (i, &mag) in magnitudes.iter().enumerate() {
        cumulative += mag * mag;
        if cumulative >= threshold {
            return i as f32 * sample_rate as f32 / (2.0 * magnitudes.len() as f32);
        }
    }
    
    sample_rate as f32 / 2.0
}

fn detect_spectral_artifacts(magnitudes: &[f32]) -> bool {
    // Look for sudden drops or "holes" in the spectrum
    let mut artifact_count = 0;
    let window_size = 10;
    
    for i in window_size..magnitudes.len() - window_size {
        let before: f32 = magnitudes[i - window_size..i].iter().sum::<f32>() / window_size as f32;
        let current = magnitudes[i];
        let after: f32 = magnitudes[i + 1..i + window_size + 1].iter().sum::<f32>() / window_size as f32;
        
        let avg = (before + after) / 2.0;
        if avg > 0.0 && current < avg * 0.3 {
            artifact_count += 1;
        }
    }
    
    artifact_count > 5
}

fn analyze_dynamic_range(audio: &AudioData) -> (f32, f32, f32) {
    let samples = &audio.samples;
    
    // Find peak amplitude
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { -120.0 };
    
    // Estimate noise floor (average of lowest 5% of samples)
    let mut sorted: Vec<f32> = samples.iter().map(|s| s.abs()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise_samples = &sorted[..sorted.len() / 20];
    let noise_floor = noise_samples.iter().sum::<f32>() / noise_samples.len() as f32;
    let noise_db = if noise_floor > 0.0 { 20.0 * noise_floor.log10() } else { -120.0 };
    
    let dynamic_range = peak_db - noise_db;
    
    (dynamic_range, noise_db, peak_db)
}

fn estimate_bit_depth(dynamic_range: f32) -> u32 {
    // Theoretical: 16-bit = 96 dB, 24-bit = 144 dB
    // Allow some tolerance
    if dynamic_range > 120.0 {
        24
    } else if dynamic_range > 80.0 {
        16
    } else {
        8
    }
}

fn detect_upsampling(audio: &AudioData, cutoff_freq: f32) -> Option<u32> {
    let sample_rate = audio.sample_rate;
    let nyquist = sample_rate as f32 / 2.0;
    
    // Common sample rate pairs
    let rate_pairs = vec![
        (44100, 88200),
        (44100, 96000),
        (44100, 176400),
        (44100, 192000),
        (48000, 96000),
        (48000, 192000),
        (96000, 192000),
    ];
    
    for (original, upsampled) in rate_pairs {
        if sample_rate == upsampled {
            let original_nyquist = original as f32 / 2.0;
            // If cutoff is near the original Nyquist, likely upsampled
            if cutoff_freq < original_nyquist * 1.1 {
                return Some(original);
            }
        }
    }
    
    None
}


// Add new function to src/detector.rs to detect Opus-specific artifacts
fn detect_opus_artifacts(audio: &AudioData) -> Result<bool> {
    let fft_size = 8192;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Analyze multiple sections
    let num_sections = 5;
    let section_size = audio.samples.len() / (num_sections * audio.channels);
    let mut opus_indicators = 0;

    for section in 0..num_sections {
        let start = section * section_size * audio.channels;
        let end = (start + fft_size * audio.channels).min(audio.samples.len());
        
        if end - start < fft_size * audio.channels {
            continue;
        }

        let mut signal: Vec<Complex<f32>> = audio.samples[start..end]
            .chunks(audio.channels)
            .take(fft_size)
            .map(|chunk| Complex::new(chunk[0], 0.0))
            .collect();

        // Apply Hann window
        for (i, sample) in signal.iter_mut().enumerate() {
            let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
            *sample *= window;
        }

        fft.process(&mut signal);

        let magnitudes: Vec<f32> = signal[..fft_size / 2]
            .iter()
            .map(|c| (c.re * c.re + c.im * c.im).sqrt())
            .collect();

        // Opus characteristics:
        // 1. Spectral folding creates mirror patterns
        // 2. Band extension shows smoothed/synthetic high frequencies
        // 3. Distinct energy distribution at bandwidth boundaries
        
        if detect_spectral_folding(&magnitudes, audio.sample_rate) {
            opus_indicators += 1;
        }
        
        if detect_bandwidth_boundary(&magnitudes, audio.sample_rate) {
            opus_indicators += 1;
        }
    }

    // If multiple sections show Opus characteristics, likely transcoded
    Ok(opus_indicators > num_sections / 2)
}

fn detect_spectral_folding(magnitudes: &[f32], sample_rate: u32) -> bool {
    // Opus uses spectral folding for bandwidth extension
    // Look for correlation between low and high frequency bands
    let nyquist_idx = magnitudes.len() - 1;
    let mid_idx = magnitudes.len() / 2;
    
    // Check for suspicious similarity in spectral envelope
    let low_band: Vec<f32> = magnitudes[mid_idx/2..mid_idx].to_vec();
    let high_band: Vec<f32> = magnitudes[mid_idx..mid_idx + low_band.len()].to_vec();
    
    // Calculate correlation
    let correlation = calculate_correlation(&low_band, &high_band);
    
    // High correlation suggests folding (Opus artifact)
    correlation > 0.6
}

fn detect_bandwidth_boundary(magnitudes: &[f32], sample_rate: u32) -> bool {
    // Opus has distinct energy drops at bandwidth boundaries
    // Check for sharp transitions at 8kHz, 12kHz, or 20kHz
    let boundaries = vec![8000.0, 12000.0, 20000.0];
    
    for boundary in boundaries {
        let bin = (boundary * magnitudes.len() as f32 / (sample_rate as f32 / 2.0)) as usize;
        
        if bin >= 5 && bin < magnitudes.len() - 5 {
            let before: f32 = magnitudes[bin-5..bin].iter().sum::<f32>() / 5.0;
            let after: f32 = magnitudes[bin..bin+5].iter().sum::<f32>() / 5.0;
            
            // Sharp energy drop characteristic of Opus bandwidth boundary
            if before > 0.0 && after / before < 0.3 {
                return true;
            }
        }
    }
    
    false
}

fn calculate_correlation(signal1: &[f32], signal2: &[f32]) -> f32 {
    let len = signal1.len().min(signal2.len());
    if len == 0 {
        return 0.0;
    }
    
    let mean1: f32 = signal1[..len].iter().sum::<f32>() / len as f32;
    let mean2: f32 = signal2[..len].iter().sum::<f32>() / len as f32;
    
    let mut numerator = 0.0;
    let mut denom1 = 0.0;
    let mut denom2 = 0.0;
    
    for i in 0..len {
        let diff1 = signal1[i] - mean1;
        let diff2 = signal2[i] - mean2;
        numerator += diff1 * diff2;
        denom1 += diff1 * diff1;
        denom2 += diff2 * diff2;
    }
    
    let denominator = (denom1 * denom2).sqrt();
    if denominator > 0.0 {
        numerator / denominator
    } else {
        0.0
    }
}
