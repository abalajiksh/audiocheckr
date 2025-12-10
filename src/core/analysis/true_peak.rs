// src/core/analysis/true_peak.rs
//
// True peak measurement per ITU-R BS.1770

/// True peak analysis result
#[derive(Debug, Clone, Default)]
pub struct TruePeakAnalysis {
    pub sample_peak_db: f32,
    pub true_peak_db: f32,
    pub inter_sample_overs: usize,
    pub max_over_db: f32,
    pub loudness: Option<LoudnessInfo>,
}

/// Loudness measurement info
#[derive(Debug, Clone, Default)]
pub struct LoudnessInfo {
    pub integrated_lufs: f32,
    pub momentary_max_lufs: f32,
    pub short_term_max_lufs: f32,
    pub loudness_range_lu: f32,
}

/// Per-channel true peak
#[derive(Debug, Clone, Default)]
pub struct ChannelTruePeak {
    pub channel: usize,
    pub sample_peak_db: f32,
    pub true_peak_db: f32,
}

/// Analyze true peak of mono signal
pub fn analyze_true_peak(samples: &[f32], _sample_rate: u32) -> TruePeakAnalysis {
    if samples.is_empty() {
        return TruePeakAnalysis::default();
    }
    
    // Sample peak
    let sample_peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let sample_peak_db = if sample_peak > 1e-10 {
        20.0 * sample_peak.log10()
    } else {
        -100.0
    };
    
    // True peak via 4x oversampling
    let oversampled = oversample_4x(samples);
    let true_peak = oversampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let true_peak_db = if true_peak > 1e-10 {
        20.0 * true_peak.log10()
    } else {
        -100.0
    };
    
    // Count inter-sample overs
    let mut inter_sample_overs = 0;
    let mut max_over_db = 0.0f32;
    
    for (i, &peak) in oversampled.iter().enumerate() {
        if peak.abs() > 1.0 {
            // Check if this is an inter-sample peak (not on original sample boundary)
            if i % 4 != 0 {
                inter_sample_overs += 1;
                let over_db = 20.0 * peak.abs().log10();
                if over_db > max_over_db {
                    max_over_db = over_db;
                }
            }
        }
    }
    
    TruePeakAnalysis {
        sample_peak_db,
        true_peak_db,
        inter_sample_overs,
        max_over_db,
        loudness: None,  // Full loudness measurement would require more processing
    }
}

/// Analyze true peak for stereo
pub fn analyze_true_peak_stereo(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<ChannelTruePeak> {
    vec![
        {
            let result = analyze_true_peak(left, sample_rate);
            ChannelTruePeak {
                channel: 0,
                sample_peak_db: result.sample_peak_db,
                true_peak_db: result.true_peak_db,
            }
        },
        {
            let result = analyze_true_peak(right, sample_rate);
            ChannelTruePeak {
                channel: 1,
                sample_peak_db: result.sample_peak_db,
                true_peak_db: result.true_peak_db,
            }
        },
    ]
}

/// Simple 4x oversampling for true peak measurement
fn oversample_4x(samples: &[f32]) -> Vec<f32> {
    use std::f32::consts::PI;
    
    let filter_len = 16;
    let mut output = vec![0.0f32; samples.len() * 4];
    
    // Place original samples
    for (i, &sample) in samples.iter().enumerate() {
        output[i * 4] = sample;
    }
    
    // Interpolate intermediate samples
    for i in 0..samples.len() {
        for j in 1..4 {
            let frac = j as f32 / 4.0;
            let out_idx = i * 4 + j;
            
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
            output[out_idx] = if weight_sum > 0.0 { sum / weight_sum * 4.0 } else { 0.0 };
        }
    }
    
    output
}
