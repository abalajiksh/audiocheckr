//! MFCC (Mel-Frequency Cepstral Coefficients) Fingerprinting Module
//!
//! Computes MFCCs for audio fingerprinting and codec artifact detection.
//!
//! # Signal Processing Pipeline
//!
//! ```text
//! samples → pre-emphasis → frame → window → |FFT|² → mel filterbank → log → DCT → MFCCs
//!                                                                                    │
//!                                                          deltas ← Δ ← ΔΔ ──────────┘
//!                                                                    │
//!                                                              fingerprint
//! ```
//!
//! # Why MFCCs for AudioCheckr
//!
//! MFCCs capture the spectral envelope on a perceptually-motivated scale.
//! This is useful for several detection tasks:
//!
//! - **Codec fingerprinting**: Different lossy codecs (MP3, AAC, Opus, Vorbis)
//!   leave characteristic signatures in the cepstral domain — their psychoacoustic
//!   models shape the spectral envelope differently, and this shows up clearly
//!   in MFCCs even after re-encoding to lossless.
//!
//! - **Transcode detection**: A file that went through lossy encoding and back
//!   will have a smoother cepstral envelope than genuine lossless, because the
//!   codec's subband quantisation removes fine spectral detail that MFCCs capture.
//!
//! - **Audio matching**: MFCC fingerprints enable matching an unknown file against
//!   a reference database to identify the source recording, which can reveal
//!   whether a "lossless" file matches a known lossy release.
//!
//! # Mel Scale
//!
//! The mel scale approximates the human ear's frequency resolution:
//! linear below ~1 kHz, logarithmic above. The standard conversion is:
//!
//!   mel = 2595 · log₁₀(1 + f/700)
//!   f   = 700 · (10^(mel/2595) − 1)
//!
//! This means mel filterbank bins are narrow at low frequencies (where
//! the ear has fine resolution) and wide at high frequencies — exactly
//! matching critical band theory.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

// ════════════════════════════════════════════════════════════════════
// Configuration
// ════════════════════════════════════════════════════════════════════

/// MFCC analysis configuration.
#[derive(Debug, Clone)]
pub struct MfccConfig {
    /// Number of mel filterbank channels (typically 26–40).
    pub n_mels: usize,
    /// Number of cepstral coefficients to retain (typically 13–20).
    /// The 0th coefficient (energy) is included.
    pub n_mfcc: usize,
    /// FFT size in samples (must be power of 2).
    pub fft_size: usize,
    /// Hop size in samples between consecutive frames.
    pub hop_size: usize,
    /// Pre-emphasis coefficient (0.0 disables, 0.97 is standard).
    pub pre_emphasis: f64,
    /// Lower frequency bound for the mel filterbank (Hz).
    pub f_min: f64,
    /// Upper frequency bound for the mel filterbank (Hz).
    /// Set to 0.0 to use Nyquist.
    pub f_max: f64,
    /// Whether to compute delta (velocity) coefficients.
    pub compute_deltas: bool,
    /// Whether to compute delta-delta (acceleration) coefficients.
    pub compute_delta_deltas: bool,
    /// Whether to apply liftering (cepstral weighting) and its coefficient.
    /// 0 disables, 22 is a common value.
    pub lifter_coefficient: usize,
}

impl Default for MfccConfig {
    fn default() -> Self {
        Self {
            n_mels: 40,
            n_mfcc: 13,
            fft_size: 2048,
            hop_size: 512,
            pre_emphasis: 0.97,
            f_min: 20.0,
            f_max: 0.0, // will use Nyquist
            compute_deltas: true,
            compute_delta_deltas: true,
            lifter_coefficient: 22,
        }
    }
}

impl MfccConfig {
    /// Configuration tuned for codec artifact detection.
    /// Uses more mel bands and MFCCs to capture fine spectral detail.
    pub fn for_codec_detection() -> Self {
        Self {
            n_mels: 64,
            n_mfcc: 20,
            fft_size: 4096,
            hop_size: 1024,
            pre_emphasis: 0.97,
            f_min: 20.0,
            f_max: 0.0,
            compute_deltas: true,
            compute_delta_deltas: true,
            lifter_coefficient: 22,
        }
    }

    /// Configuration tuned for audio fingerprinting / matching.
    /// Compact representation for fast comparison.
    pub fn for_fingerprinting() -> Self {
        Self {
            n_mels: 40,
            n_mfcc: 13,
            fft_size: 2048,
            hop_size: 512,
            pre_emphasis: 0.97,
            f_min: 300.0,  // skip very low frequencies for robustness
            f_max: 8000.0, // most perceptual content is below 8 kHz
            compute_deltas: true,
            compute_delta_deltas: false,
            lifter_coefficient: 22,
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Results
// ════════════════════════════════════════════════════════════════════

/// Complete MFCC analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfccResult {
    /// MFCC matrix: `[n_frames][n_mfcc]`.
    pub mfcc: Vec<Vec<f64>>,
    /// Delta (first derivative) coefficients, if computed.
    pub deltas: Option<Vec<Vec<f64>>>,
    /// Delta-delta (second derivative) coefficients, if computed.
    pub delta_deltas: Option<Vec<Vec<f64>>>,
    /// Number of frames.
    pub n_frames: usize,
    /// Number of MFCC coefficients per frame.
    pub n_mfcc: usize,
    /// Sample rate used for analysis.
    pub sample_rate: u32,
    /// Statistical summary across all frames.
    pub stats: MfccStats,
    /// Compact fingerprint derived from MFCCs.
    pub fingerprint: MfccFingerprint,
}

/// Per-coefficient statistics across all frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfccStats {
    /// Mean of each MFCC coefficient across frames.
    pub mean: Vec<f64>,
    /// Standard deviation of each coefficient.
    pub std_dev: Vec<f64>,
    /// Skewness of each coefficient.
    pub skewness: Vec<f64>,
    /// Kurtosis of each coefficient (excess kurtosis, i.e. Gaussian = 0).
    pub kurtosis: Vec<f64>,
    /// Mean of delta coefficients (if computed).
    pub delta_mean: Option<Vec<f64>>,
    /// Std dev of delta coefficients.
    pub delta_std: Option<Vec<f64>>,
}

/// Compact audio fingerprint derived from MFCC statistics.
///
/// This is a fixed-size representation regardless of audio duration,
/// suitable for database storage and fast comparison via cosine similarity
/// or Euclidean distance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfccFingerprint {
    /// Feature vector: concatenation of MFCC means, std devs, skewness,
    /// kurtosis, and optionally delta statistics.
    /// For 13 MFCCs with deltas: 13×4 + 13×2 = 78 dimensions.
    pub features: Vec<f64>,
    /// Dimensionality of the feature vector.
    pub dimension: usize,
    /// Duration of analyzed audio in seconds.
    pub duration_secs: f64,
    /// Number of frames used to compute the fingerprint.
    pub n_frames: usize,
}

impl MfccFingerprint {
    /// Compute cosine similarity with another fingerprint.
    /// Returns value in [-1.0, 1.0], where 1.0 = identical.
    pub fn cosine_similarity(&self, other: &MfccFingerprint) -> f64 {
        if self.features.len() != other.features.len() {
            return 0.0;
        }

        let dot: f64 = self
            .features
            .iter()
            .zip(other.features.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm_a: f64 = self.features.iter().map(|x| x * x).sum::<f64>().sqrt();
        let norm_b: f64 = other.features.iter().map(|x| x * x).sum::<f64>().sqrt();

        if norm_a < 1e-15 || norm_b < 1e-15 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    /// Compute Euclidean distance to another fingerprint.
    pub fn euclidean_distance(&self, other: &MfccFingerprint) -> f64 {
        if self.features.len() != other.features.len() {
            return f64::INFINITY;
        }

        self.features
            .iter()
            .zip(other.features.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

// ════════════════════════════════════════════════════════════════════
// Analyzer
// ════════════════════════════════════════════════════════════════════

/// MFCC analyzer.
///
/// Construct once per sample rate, call `analyze()` on multiple buffers.
/// The mel filterbank and DCT matrices are precomputed at construction.
pub struct MfccAnalyzer {
    config: MfccConfig,
    sample_rate: u32,
    /// Precomputed mel filterbank: `[n_mels][fft_size/2 + 1]`.
    mel_filterbank: Vec<Vec<f64>>,
    /// Precomputed Hann window.
    window: Vec<f64>,
    /// Precomputed DCT-II matrix: `[n_mfcc][n_mels]`.
    dct_matrix: Vec<Vec<f64>>,
    /// Precomputed liftering weights.
    lifter_weights: Vec<f64>,
}

impl MfccAnalyzer {
    /// Create a new analyzer for the given sample rate.
    pub fn new(sample_rate: u32, config: MfccConfig) -> Self {
        let f_max = if config.f_max <= 0.0 {
            sample_rate as f64 / 2.0
        } else {
            config.f_max.min(sample_rate as f64 / 2.0)
        };

        let mel_filterbank = build_mel_filterbank(
            config.n_mels,
            config.fft_size,
            sample_rate as f64,
            config.f_min,
            f_max,
        );

        let window = hann_window(config.fft_size);
        let dct_matrix = build_dct_matrix(config.n_mfcc, config.n_mels);

        let lifter_weights = if config.lifter_coefficient > 0 {
            build_lifter_weights(config.n_mfcc, config.lifter_coefficient)
        } else {
            vec![1.0; config.n_mfcc]
        };

        Self {
            config,
            sample_rate,
            mel_filterbank,
            window,
            dct_matrix,
            lifter_weights,
        }
    }

    /// Analyze audio samples and return MFCCs + fingerprint.
    ///
    /// Samples should be mono f64 in [-1.0, 1.0].
    pub fn analyze(&self, samples: &[f64]) -> MfccResult {
        if samples.is_empty() {
            return MfccResult {
                mfcc: vec![],
                deltas: None,
                delta_deltas: None,
                n_frames: 0,
                n_mfcc: self.config.n_mfcc,
                sample_rate: self.sample_rate,
                stats: MfccStats {
                    mean: vec![0.0; self.config.n_mfcc],
                    std_dev: vec![0.0; self.config.n_mfcc],
                    skewness: vec![0.0; self.config.n_mfcc],
                    kurtosis: vec![0.0; self.config.n_mfcc],
                    delta_mean: None,
                    delta_std: None,
                },
                fingerprint: MfccFingerprint {
                    features: vec![],
                    dimension: 0,
                    duration_secs: 0.0,
                    n_frames: 0,
                },
            };
        }

        // 1. Pre-emphasis
        let emphasized = if self.config.pre_emphasis > 0.0 {
            pre_emphasize(samples, self.config.pre_emphasis)
        } else {
            samples.to_vec()
        };

        // 2. Frame, window, FFT, mel filterbank, log, DCT
        let n_fft_bins = self.config.fft_size / 2 + 1;
        let n_frames = if emphasized.len() > self.config.fft_size {
            (emphasized.len() - self.config.fft_size) / self.config.hop_size + 1
        } else {
            1
        };

        let mut mfcc_matrix: Vec<Vec<f64>> = Vec::with_capacity(n_frames);

        for frame_idx in 0..n_frames {
            let start = frame_idx * self.config.hop_size;

            // Extract frame and apply window
            let mut frame = vec![0.0f64; self.config.fft_size];
            let copy_len = self
                .config
                .fft_size
                .min(emphasized.len().saturating_sub(start));
            for i in 0..copy_len {
                frame[i] = emphasized[start + i] * self.window[i];
            }

            // FFT → power spectrum
            let power_spectrum = compute_power_spectrum(&frame, n_fft_bins);

            // Apply mel filterbank
            let mel_energies = apply_filterbank(&power_spectrum, &self.mel_filterbank);

            // Log compression (floor to avoid log(0))
            let log_mel: Vec<f64> = mel_energies.iter().map(|&e| (e.max(1e-10)).ln()).collect();

            // DCT → MFCCs
            let mut mfcc_frame = apply_dct(&log_mel, &self.dct_matrix);

            // Liftering
            for (i, coeff) in mfcc_frame.iter_mut().enumerate() {
                if i < self.lifter_weights.len() {
                    *coeff *= self.lifter_weights[i];
                }
            }

            mfcc_matrix.push(mfcc_frame);
        }

        // 3. Compute deltas
        let deltas = if self.config.compute_deltas && n_frames > 1 {
            Some(compute_deltas(&mfcc_matrix, 2))
        } else {
            None
        };

        let delta_deltas = if self.config.compute_delta_deltas && n_frames > 1 {
            deltas.as_ref().map(|d| compute_deltas(d, 2))
        } else {
            None
        };

        // 4. Statistics
        let stats = compute_stats(&mfcc_matrix, &deltas);

        // 5. Fingerprint
        let fingerprint = build_fingerprint(
            &stats,
            n_frames,
            samples.len() as f64 / self.sample_rate as f64,
        );

        MfccResult {
            mfcc: mfcc_matrix,
            deltas,
            delta_deltas,
            n_frames,
            n_mfcc: self.config.n_mfcc,
            sample_rate: self.sample_rate,
            stats,
            fingerprint,
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// DSP Primitives
// ════════════════════════════════════════════════════════════════════

/// Pre-emphasis filter: y[n] = x[n] − α·x[n−1]
fn pre_emphasize(samples: &[f64], alpha: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(samples.len());
    out.push(samples[0]);
    for i in 1..samples.len() {
        out.push(samples[i] - alpha * samples[i - 1]);
    }
    out
}

/// Hann window coefficients.
fn hann_window(size: usize) -> Vec<f64> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (size - 1).max(1) as f64).cos()))
        .collect()
}

/// Compute power spectrum |FFT(x)|² using rustfft (already in project deps).
fn compute_power_spectrum(frame: &[f64], n_bins: usize) -> Vec<f64> {
    use rustfft::{num_complex::Complex, FftPlanner};

    let n = frame.len();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);

    let mut buffer: Vec<Complex<f64>> = frame.iter().map(|&x| Complex::new(x, 0.0)).collect();

    fft.process(&mut buffer);

    buffer[..n_bins]
        .iter()
        .map(|c| c.re * c.re + c.im * c.im)
        .collect()
}

// ════════════════════════════════════════════════════════════════════
// Mel Filterbank
// ════════════════════════════════════════════════════════════════════

/// Convert frequency in Hz to mel scale.
#[inline]
fn hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Convert mel scale to frequency in Hz.
#[inline]
fn mel_to_hz(mel: f64) -> f64 {
    700.0 * (10.0_f64.powf(mel / 2595.0) - 1.0)
}

/// Build triangular mel filterbank matrix.
///
/// Returns `[n_mels][n_fft_bins]` where n_fft_bins = fft_size/2 + 1.
/// Each row is a triangular filter centered on a mel-spaced frequency.
fn build_mel_filterbank(
    n_mels: usize,
    fft_size: usize,
    sample_rate: f64,
    f_min: f64,
    f_max: f64,
) -> Vec<Vec<f64>> {
    let n_bins = fft_size / 2 + 1;
    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);

    // n_mels + 2 points: includes lower and upper edges
    let mel_points: Vec<f64> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64)
        .collect();

    let hz_points: Vec<f64> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert Hz to FFT bin indices (floating point for sub-bin precision)
    let bin_points: Vec<f64> = hz_points
        .iter()
        .map(|&f| f * fft_size as f64 / sample_rate)
        .collect();

    let mut filterbank = vec![vec![0.0f64; n_bins]; n_mels];

    for m in 0..n_mels {
        let f_left = bin_points[m];
        let f_center = bin_points[m + 1];
        let f_right = bin_points[m + 2];

        for k in 0..n_bins {
            let k_f = k as f64;

            if k_f >= f_left && k_f <= f_center && (f_center - f_left) > 1e-10 {
                filterbank[m][k] = (k_f - f_left) / (f_center - f_left);
            } else if k_f > f_center && k_f <= f_right && (f_right - f_center) > 1e-10 {
                filterbank[m][k] = (f_right - k_f) / (f_right - f_center);
            }
        }
    }

    filterbank
}

/// Apply mel filterbank to power spectrum.
fn apply_filterbank(power_spectrum: &[f64], filterbank: &[Vec<f64>]) -> Vec<f64> {
    filterbank
        .iter()
        .map(|filter| {
            filter
                .iter()
                .zip(power_spectrum.iter())
                .map(|(&f, &p)| f * p)
                .sum()
        })
        .collect()
}

// ════════════════════════════════════════════════════════════════════
// DCT-II (Discrete Cosine Transform, Type II)
// ════════════════════════════════════════════════════════════════════

/// Build orthonormal DCT-II matrix: `[n_mfcc][n_mels]`.
///
/// DCT-II:  C[k][n] = cos(π·k·(2n+1) / (2·N))
///
/// This decorrelates the log mel energies, concentrating the most
/// important information in the first few coefficients.
fn build_dct_matrix(n_mfcc: usize, n_mels: usize) -> Vec<Vec<f64>> {
    let mut matrix = vec![vec![0.0f64; n_mels]; n_mfcc];

    for k in 0..n_mfcc {
        for n in 0..n_mels {
            matrix[k][n] = (PI * k as f64 * (2.0 * n as f64 + 1.0) / (2.0 * n_mels as f64)).cos();
        }
    }

    // Orthonormal scaling
    let scale_0 = (1.0 / n_mels as f64).sqrt();
    let scale_k = (2.0 / n_mels as f64).sqrt();

    for n in 0..n_mels {
        matrix[0][n] *= scale_0;
    }
    for k in 1..n_mfcc {
        for n in 0..n_mels {
            matrix[k][n] *= scale_k;
        }
    }

    matrix
}

/// Apply DCT to log-mel energies.
fn apply_dct(log_mel: &[f64], dct_matrix: &[Vec<f64>]) -> Vec<f64> {
    dct_matrix
        .iter()
        .map(|row| row.iter().zip(log_mel.iter()).map(|(&c, &e)| c * e).sum())
        .collect()
}

/// Build sinusoidal liftering weights.
///
/// Liftering de-emphasizes higher-order cepstral coefficients which
/// are more sensitive to noise:
///
///   w[n] = 1 + (L/2) · sin(π·n/L)
fn build_lifter_weights(n_mfcc: usize, l: usize) -> Vec<f64> {
    (0..n_mfcc)
        .map(|n| 1.0 + (l as f64 / 2.0) * (PI * n as f64 / l as f64).sin())
        .collect()
}

// ════════════════════════════════════════════════════════════════════
// Delta Computation
// ════════════════════════════════════════════════════════════════════

/// Compute delta (derivative) coefficients using the standard regression formula.
///
/// Δc[t] = Σ_{n=1}^{N} n · (c[t+n] − c[t−n]) / (2 · Σ_{n=1}^{N} n²)
///
/// This is the standard approach (HTK/Kaldi). N=2 (context of ±2 frames).
fn compute_deltas(features: &[Vec<f64>], n: usize) -> Vec<Vec<f64>> {
    let n_frames = features.len();
    if n_frames == 0 {
        return vec![];
    }
    let n_coeffs = features[0].len();
    let denominator: f64 = 2.0 * (1..=n).map(|i| (i * i) as f64).sum::<f64>();

    if denominator < 1e-15 {
        return features.iter().map(|f| vec![0.0; f.len()]).collect();
    }

    let mut deltas = Vec::with_capacity(n_frames);

    for t in 0..n_frames {
        let mut delta_frame = vec![0.0f64; n_coeffs];

        for lag in 1..=n {
            let t_plus = (t + lag).min(n_frames - 1);
            let t_minus = if t >= lag { t - lag } else { 0 };

            for c in 0..n_coeffs {
                delta_frame[c] += lag as f64 * (features[t_plus][c] - features[t_minus][c]);
            }
        }

        for c in 0..n_coeffs {
            delta_frame[c] /= denominator;
        }

        deltas.push(delta_frame);
    }

    deltas
}

// ════════════════════════════════════════════════════════════════════
// Statistics & Fingerprint
// ════════════════════════════════════════════════════════════════════

/// Compute per-coefficient statistics across all frames.
fn compute_stats(mfcc: &[Vec<f64>], deltas: &Option<Vec<Vec<f64>>>) -> MfccStats {
    let n_frames = mfcc.len();
    if n_frames == 0 {
        return MfccStats {
            mean: vec![],
            std_dev: vec![],
            skewness: vec![],
            kurtosis: vec![],
            delta_mean: None,
            delta_std: None,
        };
    }
    let n_coeffs = mfcc[0].len();
    let nf = n_frames as f64;

    // Mean
    let mean: Vec<f64> = (0..n_coeffs)
        .map(|c| mfcc.iter().map(|frame| frame[c]).sum::<f64>() / nf)
        .collect();

    // Variance, skewness, kurtosis accumulators
    let mut variance = vec![0.0f64; n_coeffs];
    let mut skew_acc = vec![0.0f64; n_coeffs];
    let mut kurt_acc = vec![0.0f64; n_coeffs];

    for frame in mfcc {
        for c in 0..n_coeffs {
            let d = frame[c] - mean[c];
            let d2 = d * d;
            variance[c] += d2;
            skew_acc[c] += d * d2;
            kurt_acc[c] += d2 * d2;
        }
    }

    let std_dev: Vec<f64> = variance.iter().map(|&v| (v / nf).sqrt()).collect();

    let skewness: Vec<f64> = (0..n_coeffs)
        .map(|c| {
            let s3 = std_dev[c].powi(3);
            if s3 < 1e-15 {
                0.0
            } else {
                (skew_acc[c] / nf) / s3
            }
        })
        .collect();

    let kurtosis: Vec<f64> = (0..n_coeffs)
        .map(|c| {
            let s4 = std_dev[c].powi(4);
            if s4 < 1e-15 {
                0.0
            } else {
                (kurt_acc[c] / nf) / s4 - 3.0
            }
        })
        .collect();

    // Delta statistics
    let (delta_mean, delta_std) = if let Some(d) = deltas {
        if d.is_empty() {
            (None, None)
        } else {
            let n_d = d[0].len();
            let dm: Vec<f64> = (0..n_d)
                .map(|c| d.iter().map(|frame| frame[c]).sum::<f64>() / nf)
                .collect();
            let ds: Vec<f64> = (0..n_d)
                .map(|c| {
                    let m = dm[c];
                    (d.iter().map(|frame| (frame[c] - m).powi(2)).sum::<f64>() / nf).sqrt()
                })
                .collect();
            (Some(dm), Some(ds))
        }
    } else {
        (None, None)
    };

    MfccStats {
        mean,
        std_dev,
        skewness,
        kurtosis,
        delta_mean,
        delta_std,
    }
}

/// Build a compact fingerprint from MFCC statistics.
///
/// Feature vector layout:
///   [mfcc_mean | mfcc_std | mfcc_skew | mfcc_kurt | delta_mean | delta_std]
fn build_fingerprint(stats: &MfccStats, n_frames: usize, duration_secs: f64) -> MfccFingerprint {
    let mut features = Vec::new();

    features.extend_from_slice(&stats.mean);
    features.extend_from_slice(&stats.std_dev);
    features.extend_from_slice(&stats.skewness);
    features.extend_from_slice(&stats.kurtosis);

    if let Some(ref dm) = stats.delta_mean {
        features.extend_from_slice(dm);
    }
    if let Some(ref ds) = stats.delta_std {
        features.extend_from_slice(ds);
    }

    let dimension = features.len();

    MfccFingerprint {
        features,
        dimension,
        duration_secs,
        n_frames,
    }
}

// ════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(freq: f64, amplitude: f64, sample_rate: u32, duration_secs: f64) -> Vec<f64> {
        let n = (sample_rate as f64 * duration_secs) as usize;
        (0..n)
            .map(|i| amplitude * (2.0 * PI * freq * i as f64 / sample_rate as f64).sin())
            .collect()
    }

    #[test]
    fn test_hz_mel_roundtrip() {
        for &f in &[100.0, 440.0, 1000.0, 4000.0, 8000.0, 16000.0] {
            let f_back = mel_to_hz(hz_to_mel(f));
            assert!((f - f_back).abs() < 0.01, "Roundtrip failed for {} Hz", f);
        }
    }

    #[test]
    fn test_mel_filterbank_shape() {
        let fb = build_mel_filterbank(40, 2048, 44100.0, 20.0, 22050.0);
        assert_eq!(fb.len(), 40);
        assert_eq!(fb[0].len(), 1025);
        for (i, filter) in fb.iter().enumerate() {
            assert!(
                filter.iter().sum::<f64>() > 0.0,
                "Filter {} has zero energy",
                i
            );
        }
    }

    #[test]
    fn test_dct_matrix_dc_row() {
        let dct = build_dct_matrix(13, 40);
        assert_eq!(dct.len(), 13);
        assert_eq!(dct[0].len(), 40);
        let first_val = dct[0][0];
        for &v in &dct[0] {
            assert!(
                (v - first_val).abs() < 1e-10,
                "DCT row 0 should be constant"
            );
        }
    }

    #[test]
    fn test_mfcc_basic() {
        let samples = sine_wave(440.0, 0.5, 44100, 1.0);
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let result = analyzer.analyze(&samples);
        assert!(result.n_frames > 0);
        assert_eq!(result.n_mfcc, 13);
        assert_eq!(result.mfcc[0].len(), 13);
        assert!(result.deltas.is_some());
        assert!(result.delta_deltas.is_some());
    }

    #[test]
    fn test_fingerprint_dimension() {
        let samples = sine_wave(440.0, 0.5, 44100, 2.0);
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let result = analyzer.analyze(&samples);
        // 13 coeffs × 4 stats + 13 delta_mean + 13 delta_std = 78
        assert_eq!(result.fingerprint.dimension, 78);
    }

    #[test]
    fn test_fingerprint_self_similarity() {
        let samples = sine_wave(440.0, 0.5, 44100, 2.0);
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let fp = analyzer.analyze(&samples).fingerprint;
        assert!((fp.cosine_similarity(&fp) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_different_signals_differ() {
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let tone = sine_wave(440.0, 0.5, 44100, 2.0);
        let noise: Vec<f64> = (0..88200)
            .map(|i| ((i as f64 * 0.123456).sin() * 7.919).sin() * 0.3)
            .collect();
        let sim = analyzer
            .analyze(&tone)
            .fingerprint
            .cosine_similarity(&analyzer.analyze(&noise).fingerprint);
        assert!(sim < 0.95, "Different signals should differ, got {}", sim);
    }

    #[test]
    fn test_empty_input() {
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let result = analyzer.analyze(&[]);
        assert_eq!(result.n_frames, 0);
    }

    #[test]
    fn test_short_input() {
        let samples = sine_wave(440.0, 0.5, 44100, 0.01);
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let result = analyzer.analyze(&samples);
        assert!(result.n_frames >= 1);
    }

    #[test]
    fn test_codec_detection_config() {
        let config = MfccConfig::for_codec_detection();
        assert_eq!(config.n_mels, 64);
        assert_eq!(config.n_mfcc, 20);
    }

    #[test]
    fn test_euclidean_distance_self() {
        let samples = sine_wave(440.0, 0.5, 44100, 2.0);
        let analyzer = MfccAnalyzer::new(44100, MfccConfig::default());
        let fp = analyzer.analyze(&samples).fingerprint;
        assert!(fp.euclidean_distance(&fp) < 1e-10);
    }
}
