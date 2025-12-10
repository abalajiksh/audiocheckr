// src/core/analysis/stereo.rs
//
// Stereo field analysis for detecting joint stereo and other artifacts

/// Stereo analysis results
#[derive(Debug, Clone, Default)]
pub struct StereoAnalysis {
    pub stereo_width: f32,
    pub correlation: f32,
    pub mid_side_ratio: f32,
    pub is_joint_stereo: bool,
    pub joint_stereo_confidence: f32,
    pub phase_correlation: f32,
    pub evidence: Vec<String>,
}

/// Analyze stereo characteristics
pub fn analyze_stereo(left: &[f32], right: &[f32], _sample_rate: u32) -> StereoAnalysis {
    if left.len() != right.len() || left.is_empty() {
        return StereoAnalysis::default();
    }
    
    let n = left.len() as f32;
    
    // Calculate correlation
    let mut sum_l = 0.0f32;
    let mut sum_r = 0.0f32;
    let mut sum_ll = 0.0f32;
    let mut sum_rr = 0.0f32;
    let mut sum_lr = 0.0f32;
    
    for (&l, &r) in left.iter().zip(right.iter()) {
        sum_l += l;
        sum_r += r;
        sum_ll += l * l;
        sum_rr += r * r;
        sum_lr += l * r;
    }
    
    let mean_l = sum_l / n;
    let mean_r = sum_r / n;
    
    let var_l = sum_ll / n - mean_l * mean_l;
    let var_r = sum_rr / n - mean_r * mean_r;
    let cov_lr = sum_lr / n - mean_l * mean_r;
    
    let correlation = if var_l > 1e-10 && var_r > 1e-10 {
        cov_lr / (var_l.sqrt() * var_r.sqrt())
    } else {
        1.0  // Mono or near-silent
    };
    
    // Calculate stereo width (1.0 = full stereo, 0.0 = mono)
    let stereo_width = (1.0 - correlation.abs()).sqrt();
    
    // Calculate mid-side ratio
    let mut mid_energy = 0.0f32;
    let mut side_energy = 0.0f32;
    
    for (&l, &r) in left.iter().zip(right.iter()) {
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5;
        mid_energy += mid * mid;
        side_energy += side * side;
    }
    
    let mid_side_ratio = if mid_energy > 1e-10 {
        side_energy / mid_energy
    } else {
        0.0
    };
    
    // Detect joint stereo (high correlation with frequency-dependent width)
    // This is simplified - full implementation would analyze spectral correlation
    let is_joint_stereo = correlation > 0.95 && mid_side_ratio < 0.1;
    let joint_stereo_confidence = if is_joint_stereo { 0.7 } else { 0.0 };
    
    let mut evidence = Vec::new();
    if is_joint_stereo {
        evidence.push(format!(
            "High channel correlation ({:.3}) with low side energy - possible joint stereo",
            correlation
        ));
    }
    
    StereoAnalysis {
        stereo_width,
        correlation,
        mid_side_ratio,
        is_joint_stereo,
        joint_stereo_confidence,
        phase_correlation: correlation,  // Simplified
        evidence,
    }
}
