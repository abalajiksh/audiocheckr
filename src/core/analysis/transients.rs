// src/core/analysis/transients.rs
//
// Transient and pre-echo analysis for detecting lossy compression artifacts

/// Information about a detected transient
#[derive(Debug, Clone)]
pub struct TransientInfo {
    pub position_samples: usize,
    pub position_secs: f64,
    pub amplitude: f32,
    pub rise_time_ms: f32,
    pub has_pre_echo: bool,
    pub pre_echo_level: f32,
}

/// Pre-echo analysis results
#[derive(Debug, Clone, Default)]
pub struct PreEchoAnalysis {
    pub transient_count: usize,
    pub pre_echo_count: usize,
    pub pre_echo_score: f32,
    pub average_pre_echo_level: f32,
    pub transients: Vec<TransientInfo>,
    pub evidence: Vec<String>,
}

/// Frame boundary analysis results
#[derive(Debug, Clone, Default)]
pub struct FrameBoundaryAnalysis {
    pub frame_size: usize,
    pub boundary_artifacts: usize,
    pub average_discontinuity: f32,
    pub confidence: f32,
}

/// Analyze pre-echo in transients
pub fn analyze_pre_echo(samples: &[f32], sample_rate: u32) -> PreEchoAnalysis {
    let mut analysis = PreEchoAnalysis::default();
    
    if samples.len() < 4096 {
        return analysis;
    }
    
    // Find transients (sudden amplitude increases)
    let envelope = compute_envelope(samples);
    let transients = find_transients(&envelope, sample_rate);
    
    analysis.transient_count = transients.len();
    
    // Check each transient for pre-echo
    let pre_echo_window_ms = 20.0;  // Look 20ms before transient
    let pre_echo_samples = (pre_echo_window_ms * sample_rate as f32 / 1000.0) as usize;
    
    for pos in &transients {
        if *pos > pre_echo_samples {
            let pre_region = &samples[pos - pre_echo_samples..*pos];
            let transient_region = &samples[*pos..(*pos + pre_echo_samples).min(samples.len())];
            
            let pre_energy: f32 = pre_region.iter().map(|s| s * s).sum::<f32>() / pre_region.len() as f32;
            let trans_energy: f32 = transient_region.iter().map(|s| s * s).sum::<f32>() / transient_region.len() as f32;
            
            // Pre-echo is detected when there's energy before the transient
            // that correlates with the transient shape
            let pre_echo_ratio = if trans_energy > 1e-10 {
                pre_energy / trans_energy
            } else {
                0.0
            };
            
            let has_pre_echo = pre_echo_ratio > 0.01 && pre_echo_ratio < 0.3;
            
            let transient_info = TransientInfo {
                position_samples: *pos,
                position_secs: *pos as f64 / sample_rate as f64,
                amplitude: transient_region.iter().map(|s| s.abs()).fold(0.0f32, f32::max),
                rise_time_ms: 5.0,  // Simplified
                has_pre_echo,
                pre_echo_level: pre_echo_ratio,
            };
            
            if has_pre_echo {
                analysis.pre_echo_count += 1;
            }
            
            analysis.transients.push(transient_info);
        }
    }
    
    // Calculate overall score
    if analysis.transient_count > 0 {
        analysis.pre_echo_score = analysis.pre_echo_count as f32 / analysis.transient_count as f32;
        
        let avg_level: f32 = analysis.transients.iter()
            .filter(|t| t.has_pre_echo)
            .map(|t| t.pre_echo_level)
            .sum::<f32>() / analysis.pre_echo_count.max(1) as f32;
        
        analysis.average_pre_echo_level = avg_level;
        
        if analysis.pre_echo_score > 0.3 {
            analysis.evidence.push(format!(
                "{}/{} transients show pre-echo (score: {:.2})",
                analysis.pre_echo_count,
                analysis.transient_count,
                analysis.pre_echo_score
            ));
        }
    }
    
    analysis
}

/// Analyze frame boundaries for codec artifacts
pub fn analyze_frame_boundaries(samples: &[f32], frame_sizes: &[usize]) -> FrameBoundaryAnalysis {
    let mut best_result = FrameBoundaryAnalysis::default();
    
    for &frame_size in frame_sizes {
        let mut discontinuities = 0;
        let mut total_disc = 0.0f32;
        
        for i in (frame_size..samples.len()).step_by(frame_size) {
            if i > 0 && i < samples.len() {
                let disc = (samples[i] - samples[i - 1]).abs();
                if disc > 0.01 {
                    discontinuities += 1;
                    total_disc += disc;
                }
            }
        }
        
        let num_boundaries = samples.len() / frame_size;
        let avg_disc = if discontinuities > 0 {
            total_disc / discontinuities as f32
        } else {
            0.0
        };
        
        let disc_rate = discontinuities as f32 / num_boundaries as f32;
        
        if disc_rate > best_result.confidence {
            best_result = FrameBoundaryAnalysis {
                frame_size,
                boundary_artifacts: discontinuities,
                average_discontinuity: avg_disc,
                confidence: disc_rate,
            };
        }
    }
    
    best_result
}

fn compute_envelope(samples: &[f32]) -> Vec<f32> {
    let attack = 0.01f32;
    let release = 0.0001f32;
    
    let mut envelope = Vec::with_capacity(samples.len());
    let mut current = 0.0f32;
    
    for &sample in samples {
        let abs_sample = sample.abs();
        if abs_sample > current {
            current += attack * (abs_sample - current);
        } else {
            current += release * (abs_sample - current);
        }
        envelope.push(current);
    }
    
    envelope
}

fn find_transients(envelope: &[f32], sample_rate: u32) -> Vec<usize> {
    let mut transients = Vec::new();
    let min_distance = sample_rate as usize / 10;  // 100ms minimum between transients
    let threshold = 0.1f32;
    
    let mut last_transient = 0;
    
    for i in 1..envelope.len() {
        let diff = envelope[i] - envelope[i - 1];
        if diff > threshold && i - last_transient > min_distance {
            transients.push(i);
            last_transient = i;
        }
    }
    
    transients
}
