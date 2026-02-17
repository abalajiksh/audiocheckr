//! Dynamic Range Analysis Module
//!
//! Implements three complementary dynamic range measurements:
//!
//! 1. **Crest Factor** — Simple peak-to-RMS ratio. Fast but naive;
//!    a single transient spike dominates the result.
//!
//! 2. **EBU R128 / PLR (Peak-to-Loudness Ratio)** — ITU-R BS.1770 compliant.
//!    K-weighted frequency response with absolute (-70 LUFS) and relative (-10 LU)
//!    gating. The modern broadcast standard; correlates well with perceived loudness.
//!
//! 3. **TT Dynamic Range Meter** — Pleasurize Music Foundation algorithm.
//!    Industry standard for music. Splits audio into 3-second blocks, computes
//!    per-block RMS, and reports peak minus the 20th-percentile loudest RMS.
//!    More robust than crest factor for music analysis.
//!
//! # Interpretation
//!
//! | DR Score | Interpretation                               |
//! |----------|----------------------------------------------|
//! | < 7      | Heavily compressed (loudness war victim)      |
//! | 7–13     | Typical commercial release                    |
//! | 14–20    | Audiophile / classical mastering               |
//! | > 20     | Rare; unmastered or very dynamic recordings   |

use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

/// Complete dynamic range analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicRangeResult {
    /// Number of audio channels analyzed
    pub channels: usize,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Total number of samples per channel
    pub total_samples: usize,

    // ── Crest Factor ────────────────────────────────────────────
    /// Crest factor per channel (dB)
    pub crest_factor_per_channel: Vec<f64>,
    /// Average crest factor across channels (dB)
    pub crest_factor_db: f64,

    // ── EBU R128 / PLR ─────────────────────────────────────────
    /// Integrated loudness per channel (LUFS, K-weighted & gated)
    pub integrated_loudness_per_channel: Vec<f64>,
    /// Average integrated loudness (LUFS)
    pub integrated_loudness_lufs: f64,
    /// Peak-to-Loudness Ratio (dB) — true peak minus integrated loudness
    pub plr_db: f64,
    /// True peak level (dBFS), from existing true_peak module or computed here
    pub true_peak_dbfs: f64,

    // ── TT DR Meter ─────────────────────────────────────────────
    /// TT DR score per channel (dB)
    pub tt_dr_per_channel: Vec<f64>,
    /// Average TT DR score (dB) — the headline number
    pub tt_dr_score: f64,
    /// Number of 3-second blocks analyzed per channel
    pub tt_block_count: usize,

    // ── Supplementary ───────────────────────────────────────────
    /// Global peak level per channel (dBFS)
    pub peak_dbfs_per_channel: Vec<f64>,
    /// Global RMS level per channel (dBFS)
    pub rms_dbfs_per_channel: Vec<f64>,
    /// Loudness war classification
    pub loudness_war_victim: bool,
    /// Human-readable verdict
    pub verdict: DynamicRangeVerdict,
}

/// Classification of dynamic range health.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DynamicRangeVerdict {
    /// DR > 20 — exceptional dynamics
    Exceptional,
    /// DR 14–20 — audiophile / well-mastered
    Excellent,
    /// DR 8–13 — typical commercial release
    Normal,
    /// DR 5–7 — heavily compressed
    Compressed,
    /// DR < 5 — brickwalled / loudness war casualty
    Brickwalled,
}

impl std::fmt::Display for DynamicRangeVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exceptional => write!(f, "Exceptional (DR > 20)"),
            Self::Excellent => write!(f, "Excellent (DR 14-20)"),
            Self::Normal => write!(f, "Normal (DR 8-13)"),
            Self::Compressed => write!(f, "Compressed (DR 5-7)"),
            Self::Brickwalled => write!(f, "Brickwalled (DR < 5)"),
        }
    }
}

/// Dynamic range analyzer.
pub struct DynamicRangeAnalyzer {
    sample_rate: u32,
    /// Block size in seconds for TT DR meter (default 3.0)
    tt_block_seconds: f64,
}

impl DynamicRangeAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            tt_block_seconds: 3.0,
        }
    }

    /// Override the TT DR block duration (default 3.0 seconds).
    pub fn with_block_seconds(mut self, seconds: f64) -> Self {
        self.tt_block_seconds = seconds;
        self
    }

    /// Analyze dynamic range using all three methods.
    ///
    /// `channels`: slice of sample slices, one per channel.
    /// Samples are expected as f64 in [-1.0, 1.0] range.
    pub fn analyze(&self, channels: &[&[f64]]) -> DynamicRangeResult {
        let n_channels = channels.len();
        assert!(n_channels > 0, "Must provide at least one channel");

        let total_samples = channels[0].len();

        // ── Per-channel metrics ─────────────────────────────────
        let mut crest_per_ch = Vec::with_capacity(n_channels);
        let mut peak_dbfs_per_ch = Vec::with_capacity(n_channels);
        let mut rms_dbfs_per_ch = Vec::with_capacity(n_channels);
        let mut lufs_per_ch = Vec::with_capacity(n_channels);
        let mut tt_dr_per_ch = Vec::with_capacity(n_channels);
        let mut tt_block_count = 0;

        for &samples in channels {
            let (peak, rms) = peak_and_rms(samples);
            let peak_db = to_dbfs(peak);
            let rms_db = to_dbfs(rms);
            let crest = peak_db - rms_db;

            crest_per_ch.push(crest);
            peak_dbfs_per_ch.push(peak_db);
            rms_dbfs_per_ch.push(rms_db);

            // EBU R128 integrated loudness (K-weighted, gated)
            let lufs = self.compute_integrated_loudness(samples);
            lufs_per_ch.push(lufs);

            // TT DR meter
            let (tt_dr, blocks) = self.compute_tt_dr(samples);
            tt_dr_per_ch.push(tt_dr);
            tt_block_count = blocks;
        }

        // ── Averages ────────────────────────────────────────────
        let crest_avg = mean(&crest_per_ch);
        let lufs_avg = mean(&lufs_per_ch);
        let tt_dr_avg = mean(&tt_dr_per_ch);

        // True peak: use the maximum across channels
        let true_peak_dbfs = peak_dbfs_per_ch
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        // PLR = true peak - integrated loudness
        let plr = true_peak_dbfs - lufs_avg;

        // Verdict based on TT DR score (the industry standard number)
        let verdict = classify_dr(tt_dr_avg);
        let loudness_war_victim = matches!(
            verdict,
            DynamicRangeVerdict::Compressed | DynamicRangeVerdict::Brickwalled
        );

        DynamicRangeResult {
            channels: n_channels,
            sample_rate: self.sample_rate,
            total_samples,
            crest_factor_per_channel: crest_per_ch,
            crest_factor_db: crest_avg,
            integrated_loudness_per_channel: lufs_per_ch,
            integrated_loudness_lufs: lufs_avg,
            plr_db: plr,
            true_peak_dbfs,
            tt_dr_per_channel: tt_dr_per_ch,
            tt_dr_score: tt_dr_avg,
            tt_block_count,
            peak_dbfs_per_channel: peak_dbfs_per_ch,
            rms_dbfs_per_channel: rms_dbfs_per_ch,
            loudness_war_victim,
            verdict,
        }
    }

    // ════════════════════════════════════════════════════════════════
    // EBU R128 / ITU-R BS.1770 Integrated Loudness
    // ════════════════════════════════════════════════════════════════

    /// Compute integrated loudness for a single channel (LUFS-like).
    ///
    /// Full BS.1770 is multi-channel with channel weighting (LFE, surround).
    /// For stereo/mono, we apply K-weighting and gating per-channel, then
    /// the caller averages. This is compliant for mono and the per-channel
    /// component of stereo measurement.
    fn compute_integrated_loudness(&self, samples: &[f64]) -> f64 {
        // Step 1: K-weighting filter
        let k_weighted = self.apply_k_weighting(samples);

        // Step 2: 400ms blocks with 75% overlap
        let block_samples = (0.4 * self.sample_rate as f64) as usize;
        let hop = block_samples / 4; // 75% overlap → 25% hop

        if k_weighted.len() < block_samples {
            // Too short for gated measurement; fall back to simple RMS
            let rms = rms_of(&k_weighted);
            return to_lufs(rms);
        }

        let mut block_powers: Vec<f64> = Vec::new();

        let mut start = 0;
        while start + block_samples <= k_weighted.len() {
            let block = &k_weighted[start..start + block_samples];
            let power = mean_square(block);
            block_powers.push(power);
            start += hop;
        }

        // Step 3: Absolute gate at -70 LUFS
        let abs_gate_power = 10.0_f64.powf((-70.0 + 0.691) / 10.0);
        let above_abs: Vec<f64> = block_powers
            .iter()
            .copied()
            .filter(|&p| p > abs_gate_power)
            .collect();

        if above_abs.is_empty() {
            return -70.0; // silence
        }

        // Step 4: Relative gate at -10 LU below ungated average
        let ungated_mean = mean(&above_abs);
        let rel_gate_power = ungated_mean * 10.0_f64.powf(-10.0 / 10.0);

        let above_rel: Vec<f64> = above_abs
            .iter()
            .copied()
            .filter(|&p| p > rel_gate_power)
            .collect();

        if above_rel.is_empty() {
            return -70.0;
        }

        let gated_mean = mean(&above_rel);
        to_lufs_from_power(gated_mean)
    }

    /// Apply K-weighting filter (two cascaded biquads) per ITU-R BS.1770.
    ///
    /// Stage 1: Pre-filter (shelving) — boosts high frequencies to model
    ///          head-related transfer function.
    /// Stage 2: High-pass (RLB weighting) — rolls off below ~100 Hz.
    ///
    /// Coefficients are sample-rate dependent. We compute them analytically
    /// rather than using fixed 48kHz tables.
    fn apply_k_weighting(&self, samples: &[f64]) -> Vec<f64> {
        let fs = self.sample_rate as f64;

        // ── Stage 1: Pre-filter (high shelf) ───────────────────
        let (b1, a1) = k_weight_prefilter_coefficients(fs);

        // ── Stage 2: High-pass (RLB) ───────────────────────────
        let (b2, a2) = k_weight_highpass_coefficients(fs);

        // Apply both biquads in sequence
        let stage1 = biquad_filter(samples, &b1, &a1);
        biquad_filter(&stage1, &b2, &a2)
    }

    // ════════════════════════════════════════════════════════════════
    // TT Dynamic Range Meter
    // ════════════════════════════════════════════════════════════════

    /// Compute TT DR score for a single channel.
    ///
    /// Returns (dr_score_db, number_of_blocks).
    fn compute_tt_dr(&self, samples: &[f64]) -> (f64, usize) {
        let block_size = (self.tt_block_seconds * self.sample_rate as f64) as usize;

        if samples.len() < block_size {
            // Whole file is one block
            let peak = peak_of(samples);
            let rms = rms_of(samples);
            let dr = to_dbfs(peak) - to_dbfs(rms);
            return (dr, 1);
        }

        let n_blocks = samples.len() / block_size;

        let mut rms_values: Vec<f64> = Vec::with_capacity(n_blocks);
        let mut peak_values: Vec<f64> = Vec::with_capacity(n_blocks);

        for i in 0..n_blocks {
            let start = i * block_size;
            let end = start + block_size;
            let block = &samples[start..end];

            let rms = rms_of(block);
            let peak = peak_of(block);

            // Exclude near-silent blocks (< -60 dBFS RMS)
            if rms > 1e-3 {
                // ~-60 dBFS
                rms_values.push(rms);
                peak_values.push(peak);
            }
        }

        if rms_values.is_empty() {
            return (0.0, n_blocks);
        }

        // Sort RMS values descending to find the loudest blocks
        let mut indexed: Vec<(usize, f64)> = rms_values.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Take top 20% of loudest blocks
        let n_top = (rms_values.len() as f64 * 0.2).ceil() as usize;
        let n_top = n_top.max(1).min(rms_values.len());

        let top_indices: Vec<usize> = indexed[..n_top].iter().map(|(i, _)| *i).collect();

        // Average peak and RMS of the top 20% blocks
        let avg_peak: f64 = top_indices.iter().map(|&i| peak_values[i]).sum::<f64>() / n_top as f64;
        let avg_rms: f64 = top_indices.iter().map(|&i| rms_values[i]).sum::<f64>() / n_top as f64;

        let dr = to_dbfs(avg_peak) - to_dbfs(avg_rms);

        (dr, n_blocks)
    }
}

// ════════════════════════════════════════════════════════════════════
// K-Weighting Filter Coefficient Computation
// ════════════════════════════════════════════════════════════════════

/// Pre-filter (high-shelf) coefficients for K-weighting.
fn k_weight_prefilter_coefficients(fs: f64) -> ([f64; 3], [f64; 3]) {
    // Reference coefficients at 48kHz from ITU-R BS.1770-4
    if (fs - 48000.0).abs() < 1.0 {
        return (
            [1.53512485958697, -2.69169618940638, 1.19839281085285],
            [1.0, -1.69065929318241, 0.73248077421585],
        );
    }

    let fc = 1681.974450955533;
    let g_db = 3.999843853973347;
    let q = 0.7071752369554196;

    let a = 10.0_f64.powf(g_db / 40.0);
    let w0 = 2.0 * PI * fc / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha;

    ([b0 / a0, b1 / a0, b2 / a0], [1.0, a1 / a0, a2 / a0])
}

/// High-pass (RLB weighting) coefficients for K-weighting.
fn k_weight_highpass_coefficients(fs: f64) -> ([f64; 3], [f64; 3]) {
    if (fs - 48000.0).abs() < 1.0 {
        return (
            [1.0, -2.0, 1.0],
            [1.0, -1.99004745483398, 0.99007225036621],
        );
    }

    let fc = 38.13547087602444;
    let w0 = 2.0 * PI * fc / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * 0.7071067811865476);

    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    ([b0 / a0, b1 / a0, b2 / a0], [1.0, a1 / a0, a2 / a0])
}

/// Apply a biquad (2nd-order IIR) filter using Direct Form II transposed.
fn biquad_filter(input: &[f64], b: &[f64; 3], a: &[f64; 3]) -> Vec<f64> {
    let mut output = Vec::with_capacity(input.len());
    let mut z1 = 0.0_f64;
    let mut z2 = 0.0_f64;

    for &x in input {
        let y = b[0] * x + z1;
        z1 = b[1] * x - a[1] * y + z2;
        z2 = b[2] * x - a[2] * y;
        output.push(y);
    }

    output
}

// ════════════════════════════════════════════════════════════════════
// Helper Functions
// ════════════════════════════════════════════════════════════════════

fn peak_of(samples: &[f64]) -> f64 {
    samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f64, f64::max)
}

fn rms_of(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64).sqrt()
}

fn mean_square(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64
}

fn peak_and_rms(samples: &[f64]) -> (f64, f64) {
    (peak_of(samples), rms_of(samples))
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn to_dbfs(amplitude: f64) -> f64 {
    if amplitude <= 0.0 {
        return -120.0;
    }
    20.0 * amplitude.log10()
}

fn to_lufs(rms: f64) -> f64 {
    if rms <= 0.0 {
        return -70.0;
    }
    -0.691 + 10.0 * (rms * rms).log10()
}

fn to_lufs_from_power(power: f64) -> f64 {
    if power <= 0.0 {
        return -70.0;
    }
    -0.691 + 10.0 * power.log10()
}

fn classify_dr(tt_dr: f64) -> DynamicRangeVerdict {
    if tt_dr > 20.0 {
        DynamicRangeVerdict::Exceptional
    } else if tt_dr >= 14.0 {
        DynamicRangeVerdict::Excellent
    } else if tt_dr >= 8.0 {
        DynamicRangeVerdict::Normal
    } else if tt_dr >= 5.0 {
        DynamicRangeVerdict::Compressed
    } else {
        DynamicRangeVerdict::Brickwalled
    }
}

// ════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(freq: f64, amplitude: f64, sample_rate: u32, duration_secs: f64) -> Vec<f64> {
        let n_samples = (sample_rate as f64 * duration_secs) as usize;
        (0..n_samples)
            .map(|i| amplitude * (2.0 * PI * freq * i as f64 / sample_rate as f64).sin())
            .collect()
    }

    #[test]
    fn test_crest_factor_sine() {
        let samples = sine_wave(1000.0, 0.5, 48000, 5.0);
        let analyzer = DynamicRangeAnalyzer::new(48000);
        let result = analyzer.analyze(&[&samples]);
        assert!(
            (result.crest_factor_db - 3.01).abs() < 0.1,
            "Sine crest factor should be ~3.01 dB, got {:.2}",
            result.crest_factor_db
        );
    }

    #[test]
    fn test_tt_dr_high_dynamic_range() {
        let sr = 48000_u32;
        let mut samples = Vec::new();
        for i in 0..10 {
            let amp = if i % 2 == 0 { 0.9 } else { 0.01 };
            let block = sine_wave(440.0, amp, sr, 3.0);
            samples.extend_from_slice(&block);
        }
        let analyzer = DynamicRangeAnalyzer::new(sr);
        let result = analyzer.analyze(&[&samples]);
        assert!(
            result.tt_dr_score > 10.0,
            "Alternating loud/quiet should give DR > 10, got {:.1}",
            result.tt_dr_score
        );
    }

    #[test]
    fn test_tt_dr_brickwalled() {
        let samples = sine_wave(440.0, 0.99, 48000, 15.0);
        let analyzer = DynamicRangeAnalyzer::new(48000);
        let result = analyzer.analyze(&[&samples]);
        assert!(
            result.tt_dr_score < 5.0,
            "Constant amplitude should give low DR, got {:.1}",
            result.tt_dr_score
        );
    }

    #[test]
    fn test_lufs_silence() {
        let samples = vec![0.0; 48000 * 5];
        let analyzer = DynamicRangeAnalyzer::new(48000);
        let result = analyzer.analyze(&[&samples]);
        assert!(
            result.integrated_loudness_lufs <= -70.0,
            "Silence should be <= -70 LUFS, got {:.1}",
            result.integrated_loudness_lufs
        );
    }

    #[test]
    fn test_stereo_analysis() {
        let left = sine_wave(440.0, 0.8, 48000, 5.0);
        let right = sine_wave(440.0, 0.4, 48000, 5.0);
        let analyzer = DynamicRangeAnalyzer::new(48000);
        let result = analyzer.analyze(&[&left, &right]);
        assert_eq!(result.channels, 2);
        assert_eq!(result.crest_factor_per_channel.len(), 2);
        assert_eq!(result.tt_dr_per_channel.len(), 2);
        assert!(
            (result.crest_factor_per_channel[0] - result.crest_factor_per_channel[1]).abs() < 0.5
        );
    }

    #[test]
    fn test_verdict_classification() {
        assert_eq!(classify_dr(25.0), DynamicRangeVerdict::Exceptional);
        assert_eq!(classify_dr(16.0), DynamicRangeVerdict::Excellent);
        assert_eq!(classify_dr(10.0), DynamicRangeVerdict::Normal);
        assert_eq!(classify_dr(6.0), DynamicRangeVerdict::Compressed);
        assert_eq!(classify_dr(3.0), DynamicRangeVerdict::Brickwalled);
    }

    #[test]
    fn test_k_weighting_boosts_high_freq() {
        let sr = 48000;
        let low = sine_wave(100.0, 0.5, sr, 1.0);
        let high = sine_wave(4000.0, 0.5, sr, 1.0);
        let analyzer = DynamicRangeAnalyzer::new(sr);
        let low_filtered = analyzer.apply_k_weighting(&low);
        let high_filtered = analyzer.apply_k_weighting(&high);
        let low_gain = rms_of(&low_filtered) / rms_of(&low);
        let high_gain = rms_of(&high_filtered) / rms_of(&high);
        assert!(
            high_gain > low_gain,
            "K-weighting should boost high freq relative to low: low_gain={:.3}, high_gain={:.3}",
            low_gain,
            high_gain
        );
    }

    #[test]
    fn test_biquad_stability() {
        let impulse: Vec<f64> = std::iter::once(1.0)
            .chain(std::iter::repeat(0.0).take(999))
            .collect();
        let (b, a) = k_weight_prefilter_coefficients(48000.0);
        let output = biquad_filter(&impulse, &b, &a);
        let max_val = output.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        assert!(max_val < 10.0, "Biquad output should be bounded, got max {}", max_val);
        let tail_energy: f64 = output[900..].iter().map(|x| x * x).sum();
        assert!(tail_energy < 1e-6, "Biquad tail should decay");
    }
}
