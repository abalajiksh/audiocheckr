//! Statistical and spectral analysis functions

/// Compute moving average
pub fn moving_average(data: &[f32], window_size: usize) -> Vec<f32> {
    if data.len() < window_size || window_size == 0 {
        return data.to_vec();
    }
    
    let mut result = Vec::with_capacity(data.len());
    let mut sum: f32 = data[..window_size].iter().sum();
    
    // First window_size/2 values: use partial window
    for i in 0..window_size / 2 {
        let partial_sum: f32 = data[..=i + window_size / 2].iter().sum();
        result.push(partial_sum / (i + window_size / 2 + 1) as f32);
    }
    
    // Middle values: full window
    for i in window_size / 2..data.len() - window_size / 2 {
        if i > window_size / 2 {
            sum = sum - data[i - window_size / 2 - 1] + data[i + window_size / 2];
        }
        result.push(sum / window_size as f32);
    }
    
    // Last window_size/2 values: use partial window
    for i in data.len() - window_size / 2..data.len() {
        let partial_sum: f32 = data[i - window_size / 2..].iter().sum();
        result.push(partial_sum / (data.len() - i + window_size / 2) as f32);
    }
    
    // Ensure output length matches input
    result.truncate(data.len());
    while result.len() < data.len() {
        result.push(*data.last().unwrap_or(&0.0));
    }
    
    result
}

/// Compute median of a slice
pub fn median(data: &mut [f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    
    data.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    
    let mid = data.len() / 2;
    if data.len() % 2 == 0 {
        (data[mid - 1] + data[mid]) / 2.0
    } else {
        data[mid]
    }
}

/// Compute RMS (Root Mean Square)
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Compute peak amplitude
pub fn peak_amplitude(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

/// Convert amplitude to dB (relative to 1.0)
pub fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude > 1e-10 {
        20.0 * amplitude.log10()
    } else {
        -200.0
    }
}

/// Convert dB to amplitude
pub fn db_to_amplitude(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Compute envelope using Hilbert transform approximation
pub fn compute_envelope(samples: &[f32], smooth_samples: usize) -> Vec<f32> {
    // Simple peak-following envelope
    let mut envelope = Vec::with_capacity(samples.len());
    
    let attack = 0.01;  // Fast attack
    let release = 0.0001;  // Slow release
    
    let mut current = 0.0f32;
    
    for &sample in samples {
        let abs_sample = sample.abs();
        if abs_sample > current {
            current = current + attack * (abs_sample - current);
        } else {
            current = current + release * (abs_sample - current);
        }
        envelope.push(current);
    }
    
    // Smooth the envelope
    if smooth_samples > 0 {
        moving_average(&envelope, smooth_samples)
    } else {
        envelope
    }
}

/// Find transient positions (sudden amplitude increases)
pub fn find_transients(samples: &[f32], threshold_db: f32, min_distance: usize) -> Vec<usize> {
    let envelope = compute_envelope(samples, 64);
    let envelope_db: Vec<f32> = envelope.iter()
        .map(|&e| amplitude_to_db(e))
        .collect();
    
    let mut transients = Vec::new();
    let mut last_transient = 0;
    
    // Look for sudden increases in envelope
    let analysis_hop = 32;
    for i in (analysis_hop..envelope_db.len() - analysis_hop).step_by(analysis_hop) {
        let before = envelope_db[i - analysis_hop..i].iter()
            .fold(f32::MIN, |a, &b| a.max(b));
        let after = envelope_db[i..i + analysis_hop].iter()
            .fold(f32::MIN, |a, &b| a.max(b));
        
        let increase = after - before;
        
        if increase > threshold_db && i - last_transient > min_distance {
            transients.push(i);
            last_transient = i;
        }
    }
    
    transients
}

/// Zero-crossing rate
pub fn zero_crossing_rate(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    
    let crossings: usize = samples.windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    
    crossings as f32 / (samples.len() - 1) as f32
}

/// Compute autocorrelation
pub fn autocorrelation(samples: &[f32], max_lag: usize) -> Vec<f32> {
    let n = samples.len();
    let max_lag = max_lag.min(n - 1);
    
    // Normalize by energy
    let energy: f32 = samples.iter().map(|s| s * s).sum();
    if energy < 1e-10 {
        return vec![0.0; max_lag + 1];
    }
    
    (0..=max_lag)
        .map(|lag| {
            let sum: f32 = samples[..n - lag].iter()
                .zip(&samples[lag..])
                .map(|(a, b)| a * b)
                .sum();
            sum / energy
        })
        .collect()
}

/// Compute spectral centroid (brightness measure)
pub fn spectral_centroid(magnitudes: &[f32], sample_rate: u32) -> f32 {
    let total_energy: f32 = magnitudes.iter().sum();
    if total_energy < 1e-10 {
        return 0.0;
    }
    
    let weighted_sum: f32 = magnitudes.iter()
        .enumerate()
        .map(|(i, &m)| {
            let freq = i as f32 * sample_rate as f32 / (2.0 * magnitudes.len() as f32);
            freq * m
        })
        .sum();
    
    weighted_sum / total_energy
}

/// Compute spectral spread (bandwidth)
pub fn spectral_spread(magnitudes: &[f32], sample_rate: u32) -> f32 {
    let centroid = spectral_centroid(magnitudes, sample_rate);
    let total_energy: f32 = magnitudes.iter().sum();
    
    if total_energy < 1e-10 {
        return 0.0;
    }
    
    let variance: f32 = magnitudes.iter()
        .enumerate()
        .map(|(i, &m)| {
            let freq = i as f32 * sample_rate as f32 / (2.0 * magnitudes.len() as f32);
            let diff = freq - centroid;
            diff * diff * m
        })
        .sum();
    
    (variance / total_energy).sqrt()
}

/// Compute spectral flatness (Wiener entropy)
/// Returns 1.0 for white noise, approaches 0.0 for tonal signals
pub fn spectral_flatness(magnitudes: &[f32]) -> f32 {
    let n = magnitudes.len() as f32;
    
    // Geometric mean (via log)
    let log_sum: f32 = magnitudes.iter()
        .map(|&m| (m + 1e-10).ln())
        .sum();
    let geometric_mean = (log_sum / n).exp();
    
    // Arithmetic mean
    let arithmetic_mean = magnitudes.iter().sum::<f32>() / n;
    
    if arithmetic_mean < 1e-10 {
        return 0.0;
    }
    
    geometric_mean / arithmetic_mean
}

/// Compute spectral rolloff (frequency below which X% of energy is contained)
pub fn spectral_rolloff(magnitudes: &[f32], sample_rate: u32, percentile: f32) -> f32 {
    let total_energy: f32 = magnitudes.iter().map(|m| m * m).sum();
    let threshold = total_energy * percentile;
    
    let mut cumulative = 0.0f32;
    
    for (i, &mag) in magnitudes.iter().enumerate() {
        cumulative += mag * mag;
        if cumulative >= threshold {
            return i as f32 * sample_rate as f32 / (2.0 * magnitudes.len() as f32);
        }
    }
    
    sample_rate as f32 / 2.0
}

/// Compute spectral flux (frame-to-frame spectral change)
pub fn spectral_flux(prev_spectrum: &[f32], curr_spectrum: &[f32]) -> f32 {
    if prev_spectrum.len() != curr_spectrum.len() {
        return 0.0;
    }
    
    // Rectified spectral flux (only positive changes)
    prev_spectrum.iter()
        .zip(curr_spectrum)
        .map(|(&prev, &curr)| {
            let diff = curr - prev;
            if diff > 0.0 { diff * diff } else { 0.0 }
        })
        .sum::<f32>()
        .sqrt()
}

/// Compute spectral contrast in frequency bands
pub fn spectral_contrast(magnitudes: &[f32], num_bands: usize) -> Vec<f32> {
    let band_size = magnitudes.len() / num_bands;
    
    (0..num_bands)
        .map(|band| {
            let start = band * band_size;
            let end = ((band + 1) * band_size).min(magnitudes.len());
            let band_mags = &magnitudes[start..end];
            
            if band_mags.is_empty() {
                return 0.0;
            }
            
            let mut sorted: Vec<f32> = band_mags.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            
            // Top 20% (peaks) vs bottom 20% (valleys)
            let n = sorted.len();
            let top_start = (n * 80) / 100;
            let bottom_end = (n * 20) / 100;
            
            let peaks: f32 = sorted[top_start..].iter().sum::<f32>() / (n - top_start) as f32;
            let valleys: f32 = sorted[..bottom_end.max(1)].iter().sum::<f32>() / bottom_end.max(1) as f32;
            
            if valleys > 1e-10 {
                amplitude_to_db(peaks) - amplitude_to_db(valleys)
            } else {
                amplitude_to_db(peaks) + 60.0  // Large contrast
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms() {
        let samples = vec![1.0, -1.0, 1.0, -1.0];
        assert!((rms(&samples) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_spectral_flatness_tonal() {
        // Mostly zeros with one peak = low flatness
        let mut mags = vec![0.001; 100];
        mags[50] = 1.0;
        let flatness = spectral_flatness(&mags);
        assert!(flatness < 0.1);
    }

    #[test]
    fn test_spectral_flatness_noise() {
        // All equal = high flatness
        let mags = vec![1.0; 100];
        let flatness = spectral_flatness(&mags);
        assert!(flatness > 0.99);
    }
}
