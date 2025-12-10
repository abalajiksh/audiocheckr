//! Signal filtering and resampling utilities

use std::f32::consts::PI;

/// Apply pre-emphasis filter (boosts high frequencies)
pub fn pre_emphasis(samples: &[f32], coefficient: f32) -> Vec<f32> {
    if samples.is_empty() {
        return vec![];
    }
    
    let mut output = Vec::with_capacity(samples.len());
    output.push(samples[0]);
    
    for i in 1..samples.len() {
        output.push(samples[i] - coefficient * samples[i - 1]);
    }
    
    output
}

/// Apply de-emphasis filter (inverse of pre-emphasis)
pub fn de_emphasis(samples: &[f32], coefficient: f32) -> Vec<f32> {
    if samples.is_empty() {
        return vec![];
    }
    
    let mut output = Vec::with_capacity(samples.len());
    output.push(samples[0]);
    
    for i in 1..samples.len() {
        output.push(samples[i] + coefficient * output[i - 1]);
    }
    
    output
}

/// Simple sinc interpolation for upsampling
pub fn upsample_sinc(samples: &[f32], factor: usize) -> Vec<f32> {
    if factor <= 1 {
        return samples.to_vec();
    }
    
    let output_len = samples.len() * factor;
    let mut output = vec![0.0f32; output_len];
    
    // Sinc filter parameters
    let filter_len = 32;
    
    for (i, &sample) in samples.iter().enumerate() {
        output[i * factor] = sample;
    }
    
    // Interpolate intermediate samples
    for i in 0..output_len {
        if i % factor == 0 {
            continue;  // Original sample
        }
        
        let fractional_pos = i as f32 / factor as f32;
        let base_idx = fractional_pos.floor() as i32;
        let frac = fractional_pos - base_idx as f32;
        
        let mut sum = 0.0f32;
        let mut weight_sum = 0.0f32;
        
        for j in -filter_len..=filter_len {
            let idx = base_idx + j;
            if idx >= 0 && (idx as usize) < samples.len() {
                let x = (j as f32 - frac) * PI;
                let sinc = if x.abs() < 1e-6 { 1.0 } else { x.sin() / x };
                
                // Apply window
                let window_x = (j as f32 - frac) / filter_len as f32;
                let window = if window_x.abs() <= 1.0 {
                    0.5 * (1.0 + (PI * window_x).cos())
                } else {
                    0.0
                };
                
                let weight = sinc * window;
                sum += samples[idx as usize] * weight;
                weight_sum += weight.abs();
            }
        }
        
        output[i] = if weight_sum > 0.0 { sum / weight_sum * factor as f32 } else { 0.0 };
    }
    
    output
}

/// Simple downsampling with anti-aliasing
pub fn downsample_simple(samples: &[f32], factor: usize) -> Vec<f32> {
    if factor <= 1 {
        return samples.to_vec();
    }
    
    // Apply simple averaging as anti-aliasing
    samples.chunks(factor)
        .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
        .collect()
}
