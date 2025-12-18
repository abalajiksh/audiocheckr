// src/core/analysis/dither_detection.rs
//
// Enhanced dithering detection module for AudioCheckr.
// Detects various dithering algorithms used in bit depth conversion:
// - Rectangular (RPDF) - flat noise distribution
// - Triangular (TPDF) - triangular noise distribution  
// - Triangular HP - high-pass filtered triangular
// - Lipshitz - noise shaping with Lipshitz curve
// - Shibata family - Sony's noise shaping (low/standard/high variants)
// - F-weighted - psychoacoustic noise shaping
// - Modified/Improved E-weighted - enhanced psychoacoustic shaping
//
// Also detects dither scale (amplitude) variations: 0.5, 0.75, 1.0, 1.25, 1.5, 2.0

use std::f64::consts::PI;
use std::collections::HashMap;
use rustfft::{FftPlanner, num_complex::Complex};

/// FFmpeg dithering algorithms that we can detect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DitherAlgorithm {
    /// No dithering detected (truncation or genuine bit depth)
    None,
    /// Rectangular PDF (RPDF) - uniform noise ±0.5 LSB
    Rectangular,
    /// Triangular PDF (TPDF) - triangular noise ±1 LSB
    Triangular,
    /// High-pass triangular - TPDF with high-pass filter
    TriangularHighPass,
    /// Lipshitz noise shaping
    Lipshitz,
    /// Standard Shibata noise shaping
    Shibata,
    /// Low-frequency optimized Shibata
    LowShibata,
    /// High-frequency optimized Shibata  
    HighShibata,
    /// F-weighted psychoacoustic noise shaping
    FWeighted,
    /// Modified E-weighted noise shaping
    ModifiedEWeighted,
    /// Improved E-weighted noise shaping
    ImprovedEWeighted,
    /// Unknown dithering pattern detected
    Unknown,
}

impl std::fmt::Display for DitherAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DitherAlgorithm::None => write!(f, "None"),
            DitherAlgorithm::Rectangular => write!(f, "Rectangular (RPDF)"),
            DitherAlgorithm::Triangular => write!(f, "Triangular (TPDF)"),
            DitherAlgorithm::TriangularHighPass => write!(f, "Triangular HP"),
            DitherAlgorithm::Lipshitz => write!(f, "Lipshitz"),
            DitherAlgorithm::Shibata => write!(f, "Shibata"),
            DitherAlgorithm::LowShibata => write!(f, "Low Shibata"),
            DitherAlgorithm::HighShibata => write!(f, "High Shibata"),
            DitherAlgorithm::FWeighted => write!(f, "F-weighted"),
            DitherAlgorithm::ModifiedEWeighted => write!(f, "Modified E-weighted"),
            DitherAlgorithm::ImprovedEWeighted => write!(f, "Improved E-weighted"),
            DitherAlgorithm::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Estimated dither scale (amplitude multiplier)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DitherScale {
    /// 0.5x normal amplitude
    Half,
    /// 0.75x normal amplitude
    ThreeQuarters,
    /// 1.0x standard amplitude
    Standard,
    /// 1.25x normal amplitude  
    OneTwentyFive,
    /// 1.5x normal amplitude
    OnePointFive,
    /// 2.0x double amplitude
    Double,
    /// Scale could not be determined
    Unknown,
}

impl DitherScale {
    pub fn from_multiplier(mult: f32) -> Self {
        if mult < 0.625 { DitherScale::Half }
        else if mult < 0.875 { DitherScale::ThreeQuarters }
        else if mult < 1.125 { DitherScale::Standard }
        else if mult < 1.375 { DitherScale::OneTwentyFive }
        else if mult < 1.75 { DitherScale::OnePointFive }
        else if mult < 2.25 { DitherScale::Double }
        else { DitherScale::Unknown }
    }
    
    pub fn to_multiplier(&self) -> f32 {
        match self {
            DitherScale::Half => 0.5,
            DitherScale::ThreeQuarters => 0.75,
            DitherScale::Standard => 1.0,
            DitherScale::OneTwentyFive => 1.25,
            DitherScale::OnePointFive => 1.5,
            DitherScale::Double => 2.0,
            DitherScale::Unknown => 1.0,
        }
    }
}

impl std::fmt::Display for DitherScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x", self.to_multiplier())
    }
}

/// Complete dithering analysis result
#[derive(Debug, Clone)]
pub struct DitherDetectionResult {
    /// Detected dithering algorithm
    pub algorithm: DitherAlgorithm,
    /// Confidence in algorithm detection (0.0-1.0)
    pub algorithm_confidence: f32,
    /// Estimated dither scale
    pub scale: DitherScale,
    /// Confidence in scale detection (0.0-1.0)
    pub scale_confidence: f32,
    /// Is this file likely a bit-reduced version (24→16)?
    pub is_bit_reduced: bool,
    /// Effective bit depth detected
    pub effective_bit_depth: u8,
    /// Container/claimed bit depth
    pub container_bit_depth: u8,
    /// Noise floor level in dBFS
    pub noise_floor_db: f32,
    /// Noise spectral characteristics
    pub noise_spectrum: NoiseSpectrumProfile,
    /// Detailed evidence for the detection
    pub evidence: Vec<String>,
}

/// Noise spectrum characteristics used for algorithm fingerprinting
#[derive(Debug, Clone)]
pub struct NoiseSpectrumProfile {
    /// Spectral tilt in dB/octave (positive = HF boost, negative = LF boost)
    pub spectral_tilt: f32,
    /// Energy in low band (0-4kHz) relative to total
    pub low_band_ratio: f32,
    /// Energy in mid band (4-12kHz) relative to total
    pub mid_band_ratio: f32,
    /// Energy in high band (12-22kHz) relative to total
    pub high_band_ratio: f32,
    /// PDF flatness (1.0 = perfectly flat/rectangular)
    pub pdf_flatness: f32,
    /// PDF triangularity correlation
    pub pdf_triangularity: f32,
    /// Peak frequency of noise shaping (Hz)
    pub shaping_peak_hz: Option<f32>,
    /// Estimated noise shaping order
    pub shaping_order: u8,
}

impl Default for NoiseSpectrumProfile {
    fn default() -> Self {
        Self {
            spectral_tilt: 0.0,
            low_band_ratio: 0.33,
            mid_band_ratio: 0.33,
            high_band_ratio: 0.33,
            pdf_flatness: 0.5,
            pdf_triangularity: 0.0,
            shaping_peak_hz: None,
            shaping_order: 0,
        }
    }
}

/// Main dithering detector
pub struct DitherDetector {
    fft_size: usize,
    num_segments: usize,
    sample_rate: u32,
}

impl Default for DitherDetector {
    fn default() -> Self {
        Self {
            fft_size: 8192,
            num_segments: 64,
            sample_rate: 44100,
        }
    }
}

impl DitherDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            ..Default::default()
        }
    }
    
    /// Analyze audio for dithering patterns
    /// samples: normalized f32 audio [-1.0, 1.0]
    /// container_bits: claimed bit depth from file metadata
    pub fn analyze(&self, samples: &[f32], container_bits: u8) -> DitherDetectionResult {
        let mut evidence = Vec::new();
        
        // Step 1: Analyze LSB patterns to detect effective bit depth
        let (effective_bits, lsb_stats) = self.detect_effective_bit_depth(samples, container_bits);
        evidence.push(format!(
            "Effective bit depth: {} (container: {})",
            effective_bits, container_bits
        ));
        
        let is_bit_reduced = effective_bits < container_bits && container_bits >= 24;
        
        if !is_bit_reduced {
            // Not a bit-reduced file, no dithering to detect
            return DitherDetectionResult {
                algorithm: DitherAlgorithm::None,
                algorithm_confidence: 0.9,
                scale: DitherScale::Unknown,
                scale_confidence: 0.0,
                is_bit_reduced: false,
                effective_bit_depth: effective_bits,
                container_bit_depth: container_bits,
                noise_floor_db: -96.0,
                noise_spectrum: NoiseSpectrumProfile::default(),
                evidence,
            };
        }
        
        // Step 2: Extract and analyze dither noise
        let noise_samples = self.extract_dither_noise(samples, effective_bits);
        
        // Step 3: Analyze noise spectrum
        let noise_spectrum = self.analyze_noise_spectrum(&noise_samples);
        evidence.push(format!(
            "Noise spectral tilt: {:.1} dB/octave",
            noise_spectrum.spectral_tilt
        ));
        
        // Step 4: Analyze PDF shape
        let pdf_analysis = self.analyze_noise_pdf(&noise_samples);
        evidence.push(format!(
            "PDF flatness: {:.2}, triangularity: {:.2}",
            pdf_analysis.flatness, pdf_analysis.triangularity
        ));
        
        // Step 5: Estimate dither scale
        let (scale, scale_conf) = self.estimate_dither_scale(&noise_samples, effective_bits);
        evidence.push(format!("Estimated dither scale: {}", scale));
        
        // Step 6: Classify dithering algorithm
        let (algorithm, algo_conf) = self.classify_algorithm(
            &noise_spectrum,
            &pdf_analysis,
            &lsb_stats,
        );
        evidence.push(format!("Detected algorithm: {} ({:.0}% confidence)", 
            algorithm, algo_conf * 100.0));
        
        // Step 7: Calculate noise floor
        let noise_floor_db = self.calculate_noise_floor(&noise_samples);
        evidence.push(format!("Dither noise floor: {:.1} dBFS", noise_floor_db));
        
        DitherDetectionResult {
            algorithm,
            algorithm_confidence: algo_conf,
            scale,
            scale_confidence: scale_conf,
            is_bit_reduced,
            effective_bit_depth: effective_bits,
            container_bit_depth: container_bits,
            noise_floor_db,
            noise_spectrum,
            evidence,
        }
    }
    
    /// Detect effective bit depth from LSB analysis
    fn detect_effective_bit_depth(&self, samples: &[f32], container_bits: u8) -> (u8, LsbStats) {
        let scale = (1u64 << (container_bits - 1)) as f64;
        let test_samples = samples.len().min(500000);
        
        // Analyze activity at each bit position
        let analyze_bits = 12.min(container_bits);
        let mut bit_activity = vec![0u64; analyze_bits as usize];
        let mut total_nonzero = 0u64;
        
        // Build histogram of lower 8 bits
        let mut lsb_histogram = vec![0u64; 256];
        
        for &sample in samples.iter().take(test_samples) {
            if sample.abs() < 1e-7 {
                continue; // Skip silence
            }
            
            let sample_int = ((sample as f64) * scale) as i64;
            
            for bit in 0..analyze_bits {
                if (sample_int.abs() >> bit) & 1 != 0 {
                    bit_activity[bit as usize] += 1;
                }
            }
            
            let lsb_8 = (sample_int.abs() & 0xFF) as usize;
            lsb_histogram[lsb_8] += 1;
            total_nonzero += 1;
        }
        
        if total_nonzero < 1000 {
            return (container_bits, LsbStats::default());
        }
        
        // Normalize activity
        let activity_ratios: Vec<f32> = bit_activity.iter()
            .map(|&a| a as f32 / total_nonzero as f32)
            .collect();
        
        // Find effective bit depth by checking where activity drops off
        let mut effective = container_bits;
        for (i, &ratio) in activity_ratios.iter().enumerate() {
            // If bit has < 1% activity, likely not used
            if ratio < 0.01 && i < 8 {
                effective = container_bits - (i as u8);
                break;
            }
        }
        
        // Check for 16-bit-in-24-bit pattern (8 bits of padding)
        let zeros_at_lsb = activity_ratios.iter().take(8).filter(|&&r| r < 0.02).count();
        if zeros_at_lsb >= 6 && container_bits == 24 {
            effective = 16;
        }
        
        // Calculate histogram statistics
        let total_hist: u64 = lsb_histogram.iter().sum();
        let unique_values = lsb_histogram.iter().filter(|&&c| c > 0).count();
        
        // Calculate entropy
        let entropy: f64 = lsb_histogram.iter()
            .filter(|&&c| c > 0)
            .map(|&c| {
                let p = c as f64 / total_hist as f64;
                -p * p.ln()
            })
            .sum();
        let max_entropy = (256.0_f64).ln();
        let normalized_entropy = (entropy / max_entropy) as f32;
        
        let lsb_stats = LsbStats {
            activity_ratios,
            unique_lsb_values: unique_values,
            entropy: normalized_entropy,
            histogram: lsb_histogram,
        };
        
        (effective, lsb_stats)
    }
    
    /// Extract the dither noise component from samples
    fn extract_dither_noise(&self, samples: &[f32], effective_bits: u8) -> Vec<f32> {
        let container_scale = (1i64 << 23) as f64; // 24-bit scale
        let effective_scale = (1i64 << (effective_bits - 1)) as f64;
        let quantize_factor = container_scale / effective_scale;
        
        let mut noise = Vec::with_capacity(samples.len());
        
        for &sample in samples {
            // Quantize to effective bit depth
            let scaled = (sample as f64) * container_scale;
            let quantized = (scaled / quantize_factor).round() * quantize_factor;
            
            // Noise is the difference
            let noise_sample = (scaled - quantized) / container_scale;
            noise.push(noise_sample as f32);
        }
        
        noise
    }
    
    /// Analyze the spectral characteristics of the dither noise
    fn analyze_noise_spectrum(&self, noise: &[f32]) -> NoiseSpectrumProfile {
        if noise.len() < self.fft_size * 2 {
            return NoiseSpectrumProfile::default();
        }
        
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        // Compute averaged spectrum
        let num_frames = (noise.len() / self.fft_size).min(self.num_segments);
        let mut spectrum_accum = vec![0.0f64; self.fft_size / 2];
        
        // Hann window
        let window: Vec<f32> = (0..self.fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI as f32 * i as f32 / self.fft_size as f32).cos()))
            .collect();
        
        for frame in 0..num_frames {
            let start = frame * self.fft_size;
            let mut buffer: Vec<Complex<f32>> = noise[start..start + self.fft_size]
                .iter()
                .enumerate()
                .map(|(i, &s)| Complex::new(s * window[i], 0.0))
                .collect();
            
            fft.process(&mut buffer);
            
            for (i, c) in buffer.iter().take(self.fft_size / 2).enumerate() {
                spectrum_accum[i] += (c.re * c.re + c.im * c.im).sqrt() as f64;
            }
        }
        
        // Average and convert to dB
        let spectrum_db: Vec<f32> = spectrum_accum.iter()
            .map(|&s| {
                let avg = s / num_frames as f64;
                if avg > 1e-10 { (20.0 * avg.log10()) as f32 } else { -120.0 }
            })
            .collect();
        
        // Calculate band energies
        let nyquist = self.sample_rate as f32 / 2.0;
        let bin_hz = nyquist / (self.fft_size / 2) as f32;
        
        let low_end = (4000.0 / bin_hz) as usize;
        let mid_end = (12000.0 / bin_hz) as usize;
        
        let low_energy: f32 = spectrum_accum[1..low_end.min(spectrum_accum.len())].iter().sum::<f64>() as f32;
        let mid_energy: f32 = spectrum_accum[low_end..mid_end.min(spectrum_accum.len())].iter().sum::<f64>() as f32;
        let high_energy: f32 = spectrum_accum[mid_end..].iter().sum::<f64>() as f32;
        let total_energy = low_energy + mid_energy + high_energy;
        
        // Calculate spectral tilt using linear regression on log-frequency
        let spectral_tilt = self.calculate_spectral_tilt(&spectrum_db, bin_hz);
        
        // Find noise shaping peak
        let shaping_peak = self.find_shaping_peak(&spectrum_db, bin_hz);
        
        NoiseSpectrumProfile {
            spectral_tilt,
            low_band_ratio: low_energy / total_energy.max(1e-10),
            mid_band_ratio: mid_energy / total_energy.max(1e-10),
            high_band_ratio: high_energy / total_energy.max(1e-10),
            pdf_flatness: 0.0, // Filled by PDF analysis
            pdf_triangularity: 0.0,
            shaping_peak_hz: shaping_peak,
            shaping_order: if spectral_tilt.abs() > 3.0 { 1 } else { 0 },
        }
    }
    
    /// Calculate spectral tilt in dB/octave
    fn calculate_spectral_tilt(&self, spectrum_db: &[f32], bin_hz: f32) -> f32 {
        // Use linear regression on log2(frequency) vs magnitude
        let mut sum_x = 0.0f64;
        let mut sum_y = 0.0f64;
        let mut sum_xy = 0.0f64;
        let mut sum_xx = 0.0f64;
        let mut count = 0.0f64;
        
        for (i, &mag) in spectrum_db.iter().enumerate().skip(4) {
            let freq = (i as f32 + 0.5) * bin_hz;
            if freq > 100.0 && freq < 20000.0 && mag > -100.0 {
                let x = (freq as f64).log2();
                let y = mag as f64;
                sum_x += x;
                sum_y += y;
                sum_xy += x * y;
                sum_xx += x * x;
                count += 1.0;
            }
        }
        
        if count < 10.0 {
            return 0.0;
        }
        
        // Slope = dB per doubling of frequency = dB/octave
        let slope = (count * sum_xy - sum_x * sum_y) / (count * sum_xx - sum_x * sum_x);
        slope as f32
    }
    
    /// Find peak frequency of noise shaping (if present)
    fn find_shaping_peak(&self, spectrum_db: &[f32], bin_hz: f32) -> Option<f32> {
        // Look for peak in 10-20kHz range typical of noise shaping
        let start_bin = (10000.0 / bin_hz) as usize;
        let end_bin = (20000.0 / bin_hz).min(spectrum_db.len() as f32) as usize;
        
        if start_bin >= end_bin {
            return None;
        }
        
        let mut max_val = f32::MIN;
        let mut max_bin = start_bin;
        
        for i in start_bin..end_bin {
            if spectrum_db[i] > max_val {
                max_val = spectrum_db[i];
                max_bin = i;
            }
        }
        
        // Check if peak is significantly above average
        let avg: f32 = spectrum_db[start_bin..end_bin].iter().sum::<f32>() 
            / (end_bin - start_bin) as f32;
        
        if max_val > avg + 6.0 {
            Some((max_bin as f32 + 0.5) * bin_hz)
        } else {
            None
        }
    }
    
    /// Analyze the probability distribution function of the noise
    fn analyze_noise_pdf(&self, noise: &[f32]) -> PdfAnalysis {
        // Histogram of noise values
        let num_bins = 256;
        let mut histogram = vec![0u64; num_bins];
        
        // Find noise range
        let max_noise = noise.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if max_noise < 1e-10 {
            return PdfAnalysis::default();
        }
        
        let bin_width = max_noise * 2.0 / num_bins as f32;
        
        for &n in noise {
            let bin = ((n + max_noise) / bin_width).floor() as usize;
            if bin < num_bins {
                histogram[bin] += 1;
            }
        }
        
        let total: u64 = histogram.iter().sum();
        if total == 0 {
            return PdfAnalysis::default();
        }
        
        // Calculate PDF flatness (variance from uniform)
        let expected = total as f64 / num_bins as f64;
        let variance: f64 = histogram.iter()
            .map(|&c| (c as f64 - expected).powi(2))
            .sum::<f64>() / num_bins as f64;
        let flatness = 1.0 / (1.0 + (variance.sqrt() / expected)) as f32;
        
        // Calculate triangularity (correlation with triangular PDF)
        let center = num_bins / 2;
        let mut triangular_corr = 0.0f64;
        let mut triangular_norm = 0.0f64;
        let mut actual_norm = 0.0f64;
        
        for (i, &count) in histogram.iter().enumerate() {
            // Ideal triangular PDF peaks at center
            let dist = (i as i32 - center as i32).abs() as f64;
            let triangular = 1.0 - dist / center as f64;
            
            let actual = count as f64 / total as f64;
            let mean = 1.0 / num_bins as f64;
            
            triangular_corr += (actual - mean) * (triangular - 0.5);
            triangular_norm += (triangular - 0.5).powi(2);
            actual_norm += (actual - mean).powi(2);
        }
        
        let triangularity = if triangular_norm > 0.0 && actual_norm > 0.0 {
            (triangular_corr / (triangular_norm.sqrt() * actual_norm.sqrt())).max(0.0) as f32
        } else {
            0.0
        };
        
        PdfAnalysis {
            flatness,
            triangularity,
            histogram,
            peak_count: histogram.iter().filter(|&&c| c > total / num_bins as u64).count(),
        }
    }
    
    /// Estimate the dither scale (amplitude multiplier)
    fn estimate_dither_scale(&self, noise: &[f32], effective_bits: u8) -> (DitherScale, f32) {
        // Expected standard dither amplitude for various PDFs
        // RPDF: ±0.5 LSB → std dev = 0.289 LSB
        // TPDF: ±1.0 LSB → std dev = 0.408 LSB
        
        let lsb = 1.0 / (1u64 << (effective_bits - 1)) as f32;
        
        // Calculate RMS of noise
        let rms = (noise.iter().map(|&n| n * n).sum::<f32>() / noise.len() as f32).sqrt();
        
        // Expected RMS for standard TPDF dither
        let expected_rms = lsb * 0.408;
        
        // Ratio gives us the scale
        let scale_estimate = rms / expected_rms.max(1e-10);
        
        let scale = DitherScale::from_multiplier(scale_estimate);
        let confidence = match scale {
            DitherScale::Unknown => 0.3,
            _ => 0.7 + 0.2 * (1.0 - (scale_estimate - scale.to_multiplier()).abs()),
        };
        
        (scale, confidence)
    }
    
    /// Classify the dithering algorithm based on all analysis
    fn classify_algorithm(
        &self,
        spectrum: &NoiseSpectrumProfile,
        pdf: &PdfAnalysis,
        lsb: &LsbStats,
    ) -> (DitherAlgorithm, f32) {
        // Scoring system for each algorithm
        let mut scores: HashMap<DitherAlgorithm, f32> = HashMap::new();
        
        // No noise shaping, flat PDF → Rectangular
        if spectrum.spectral_tilt.abs() < 1.5 && pdf.flatness > 0.7 {
            *scores.entry(DitherAlgorithm::Rectangular).or_default() += 
                pdf.flatness * 0.8 + (1.0 - spectrum.spectral_tilt.abs() / 10.0) * 0.5;
        }
        
        // No noise shaping, triangular PDF → Triangular
        if spectrum.spectral_tilt.abs() < 1.5 && pdf.triangularity > 0.5 {
            *scores.entry(DitherAlgorithm::Triangular).or_default() += 
                pdf.triangularity * 1.0 + (1.0 - spectrum.spectral_tilt.abs() / 10.0) * 0.3;
        }
        
        // HF boost with triangular PDF → Triangular HP
        if spectrum.spectral_tilt > 2.0 && spectrum.spectral_tilt < 8.0 && pdf.triangularity > 0.3 {
            *scores.entry(DitherAlgorithm::TriangularHighPass).or_default() += 
                spectrum.spectral_tilt / 10.0 * 0.8 + pdf.triangularity * 0.5;
        }
        
        // Strong HF boost → Noise shaping algorithms
        if spectrum.spectral_tilt > 4.0 {
            // Lipshitz has moderate shaping
            if spectrum.spectral_tilt > 4.0 && spectrum.spectral_tilt < 10.0 {
                *scores.entry(DitherAlgorithm::Lipshitz).or_default() += 
                    0.6 + (1.0 - (spectrum.spectral_tilt - 7.0).abs() / 5.0) * 0.4;
            }
            
            // Shibata has strong HF peak
            if spectrum.shaping_peak_hz.is_some() {
                let peak = spectrum.shaping_peak_hz.unwrap();
                
                // Standard Shibata: peak around 14-16kHz
                if peak > 13000.0 && peak < 17000.0 {
                    *scores.entry(DitherAlgorithm::Shibata).or_default() += 0.9;
                }
                
                // Low Shibata: peak around 10-13kHz
                if peak > 9000.0 && peak < 14000.0 {
                    *scores.entry(DitherAlgorithm::LowShibata).or_default() += 0.8;
                }
                
                // High Shibata: peak around 17-20kHz
                if peak > 16000.0 && peak < 21000.0 {
                    *scores.entry(DitherAlgorithm::HighShibata).or_default() += 0.8;
                }
            }
            
            // F-weighted: moderate HF boost, psychoacoustic curve
            if spectrum.spectral_tilt > 6.0 && spectrum.spectral_tilt < 15.0 {
                *scores.entry(DitherAlgorithm::FWeighted).or_default() +=
                    0.5 + (1.0 - (spectrum.spectral_tilt - 10.0).abs() / 8.0) * 0.4;
            }
            
            // E-weighted variants: strong shaping
            if spectrum.spectral_tilt > 10.0 {
                *scores.entry(DitherAlgorithm::ModifiedEWeighted).or_default() += 0.6;
                *scores.entry(DitherAlgorithm::ImprovedEWeighted).or_default() += 0.55;
            }
        }
        
        // Check for truncation (no dithering)
        if lsb.entropy < 0.3 && lsb.unique_lsb_values < 10 {
            *scores.entry(DitherAlgorithm::None).or_default() += 1.2;
        }
        
        // Find best match
        let mut best = DitherAlgorithm::Unknown;
        let mut best_score = 0.0f32;
        
        for (&algo, &score) in &scores {
            if score > best_score {
                best_score = score;
                best = algo;
            }
        }
        
        // Convert score to confidence (0-1)
        let confidence = (best_score / 1.5).min(0.95);
        
        // Require minimum score to not be Unknown
        if best_score < 0.4 {
            best = DitherAlgorithm::Unknown;
        }
        
        (best, confidence)
    }
    
    /// Calculate noise floor in dBFS
    fn calculate_noise_floor(&self, noise: &[f32]) -> f32 {
        let rms = (noise.iter().map(|&n| n * n).sum::<f32>() / noise.len() as f32).sqrt();
        if rms > 1e-10 {
            20.0 * rms.log10()
        } else {
            -120.0
        }
    }
}

/// LSB analysis statistics
#[derive(Debug, Clone, Default)]
struct LsbStats {
    activity_ratios: Vec<f32>,
    unique_lsb_values: usize,
    entropy: f32,
    histogram: Vec<u64>,
}

/// PDF analysis results
#[derive(Debug, Clone, Default)]
struct PdfAnalysis {
    flatness: f32,
    triangularity: f32,
    histogram: Vec<u64>,
    peak_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dither_scale_conversion() {
        assert_eq!(DitherScale::from_multiplier(0.5), DitherScale::Half);
        assert_eq!(DitherScale::from_multiplier(1.0), DitherScale::Standard);
        assert_eq!(DitherScale::from_multiplier(2.0), DitherScale::Double);
        assert_eq!(DitherScale::from_multiplier(1.25), DitherScale::OneTwentyFive);
    }
    
    #[test]
    fn test_algorithm_display() {
        assert_eq!(format!("{}", DitherAlgorithm::Rectangular), "Rectangular (RPDF)");
        assert_eq!(format!("{}", DitherAlgorithm::Shibata), "Shibata");
    }
}
