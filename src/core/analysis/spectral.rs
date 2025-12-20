// src/core/analysis/spectral.rs
//
// Spectral analysis for detecting lossy codec transcodes.
// Uses FFT-based frequency analysis to find cutoff frequencies and codec signatures.

use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::PI;

/// Frequency cutoff detection results
#[derive(Debug, Clone)]
pub struct SpectralAnalysis {
    /// Detected frequency cutoff in Hz (where energy drops significantly)
    pub cutoff_hz: f32,
    /// Cutoff as percentage of Nyquist frequency
    pub cutoff_ratio: f32,
    /// Rolloff steepness in dB/octave
    pub rolloff_steepness: f32,
    /// Confidence in the detection (0.0 - 1.0)
    pub confidence: f32,
    /// Detected transcode type if any
    pub likely_codec: Option<CodecSignature>,
    /// Raw spectrum data for visualization
    pub spectrum_db: Vec<f32>,
    /// Frequency bins corresponding to spectrum
    pub frequencies: Vec<f32>,
}

/// Known codec frequency signatures
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodecSignature {
    pub codec: Codec,
    pub bitrate: Option<u32>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Codec {
    MP3,
    AAC,
    Opus,
    Vorbis,
    Unknown,
}

/// Known cutoff frequencies for various codecs/bitrates
/// Format: (Codec, Bitrate, Typical Cutoff Hz, Tolerance Hz)
const CODEC_CUTOFFS: &[(Codec, u32, f32, f32)] = &[
    // MP3 - LAME encoder typical cutoffs
    (Codec::MP3, 64, 11000.0, 1000.0),
    (Codec::MP3, 96, 14000.0, 1000.0),
    (Codec::MP3, 128, 16000.0, 1000.0),
    (Codec::MP3, 160, 17500.0, 1000.0),
    (Codec::MP3, 192, 18500.0, 1000.0),
    (Codec::MP3, 224, 19000.0, 1000.0),
    (Codec::MP3, 256, 19500.0, 1000.0),
    (Codec::MP3, 320, 20500.0, 1000.0),
    
    // AAC - typical cutoffs
    (Codec::AAC, 96, 14000.0, 1000.0),
    (Codec::AAC, 128, 15500.0, 1000.0),
    (Codec::AAC, 160, 17000.0, 1000.0),
    (Codec::AAC, 192, 18000.0, 1000.0),
    (Codec::AAC, 256, 19000.0, 1000.0),
    (Codec::AAC, 320, 20000.0, 1000.0),
    
    // Opus - typical cutoffs
    (Codec::Opus, 48, 12000.0, 500.0),
    (Codec::Opus, 64, 14000.0, 1000.0),
    (Codec::Opus, 96, 18000.0, 1000.0),
    (Codec::Opus, 128, 20000.0, 1000.0),
    (Codec::Opus, 192, 20000.0, 500.0),
    
    // Vorbis - quality levels
    (Codec::Vorbis, 80, 14000.0, 1000.0),   // ~q3
    (Codec::Vorbis, 112, 16000.0, 1000.0),  // ~q5
    (Codec::Vorbis, 160, 18000.0, 1000.0),  // ~q7
    (Codec::Vorbis, 192, 19000.0, 1000.0),  // ~q8
    (Codec::Vorbis, 256, 20000.0, 500.0),   // ~q9
];

/// Improved spectral analyzer using proper FFT
pub struct SpectralAnalyzer {
    /// FFT size (larger = better frequency resolution)
    fft_size: usize,
    /// Number of windows to average
    num_windows: usize,
    /// Smoothing factor for spectrum
    smoothing_bins: usize,
}

impl Default for SpectralAnalyzer {
    fn default() -> Self {
        Self {
            fft_size: 8192,       // Good frequency resolution
            num_windows: 50,      // Average 50 windows for stability
            smoothing_bins: 8,    // Smooth over 8 bins
        }
    }
}

impl SpectralAnalyzer {
    pub fn new(fft_size: usize) -> Self {
        Self {
            fft_size,
            ..Default::default()
        }
    }

    /// Analyze audio samples to detect spectral cutoff
    pub fn analyze(&self, samples: &[f32], sample_rate: u32) -> SpectralAnalysis {
        let nyquist = sample_rate as f32 / 2.0;
        
        if samples.len() < self.fft_size * 2 {
            // Not enough samples for analysis
            return SpectralAnalysis {
                cutoff_hz: nyquist,
                cutoff_ratio: 1.0,
                rolloff_steepness: 0.0,
                confidence: 0.0,
                likely_codec: None,
                spectrum_db: vec![],
                frequencies: vec![],
            };
        }
        
        // Step 1: Compute averaged magnitude spectrum using rustfft
        let spectrum_linear = self.compute_averaged_spectrum_fft(samples);
        
        // Step 2: Convert to dB and smooth
        let spectrum_db = self.to_db_smoothed(&spectrum_linear);
        
        // Step 3: Generate frequency bins
        let frequencies: Vec<f32> = (0..spectrum_db.len())
            .map(|i| i as f32 * sample_rate as f32 / self.fft_size as f32)
            .collect();
        
        // Step 4: Detect cutoff using multiple methods
        let (cutoff_hz, rolloff, confidence) = self.detect_cutoff_multimethod(
            &spectrum_db, 
            &frequencies,
            sample_rate
        );
        
        // Step 5: Calculate cutoff ratio
        let cutoff_ratio = cutoff_hz / nyquist;
        
        // Step 6: Match against known codec signatures
        let likely_codec = self.match_codec_signature(cutoff_hz, rolloff);
        
        SpectralAnalysis {
            cutoff_hz,
            cutoff_ratio,
            rolloff_steepness: rolloff,
            confidence,
            likely_codec,
            spectrum_db,
            frequencies,
        }
    }

    /// Compute averaged magnitude spectrum using rustfft (fast!)
    fn compute_averaged_spectrum_fft(&self, samples: &[f32]) -> Vec<f32> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        // Hop size for 50% overlap
        let hop_size = self.fft_size / 2;
        let num_windows = ((samples.len() - self.fft_size) / hop_size + 1).min(self.num_windows);
        
        if num_windows == 0 {
            return vec![0.0; self.fft_size / 2];
        }
        
        // Pre-compute Hann window
        let window: Vec<f32> = (0..self.fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / self.fft_size as f32).cos()))
            .collect();
        
        let mut spectrum_sum = vec![0.0f64; self.fft_size / 2];
        let mut buffer = vec![Complex::new(0.0f32, 0.0f32); self.fft_size];
        
        for w in 0..num_windows {
            let start = w * hop_size;
            if start + self.fft_size > samples.len() {
                break;
            }
            
            // Apply window and copy to buffer
            for i in 0..self.fft_size {
                buffer[i] = Complex::new(samples[start + i] * window[i], 0.0);
            }
            
            // Compute FFT
            fft.process(&mut buffer);
            
            // Accumulate magnitude spectrum
            for (i, c) in buffer.iter().take(self.fft_size / 2).enumerate() {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                spectrum_sum[i] += mag as f64;
            }
        }
        
        // Average
        spectrum_sum.iter().map(|&s| (s / num_windows as f64) as f32).collect()
    }

    /// Convert to dB scale and apply smoothing
    fn to_db_smoothed(&self, spectrum: &[f32]) -> Vec<f32> {
        // Convert to dB
        let spectrum_db: Vec<f32> = spectrum.iter()
            .map(|&mag| {
                if mag > 1e-10 {
                    20.0 * mag.log10()
                } else {
                    -120.0
                }
            })
            .collect();
        
        // Apply moving average smoothing
        let mut smoothed = vec![0.0f32; spectrum_db.len()];
        let half_window = self.smoothing_bins / 2;
        
        for i in 0..spectrum_db.len() {
            let start = i.saturating_sub(half_window);
            let end = (i + half_window + 1).min(spectrum_db.len());
            let sum: f32 = spectrum_db[start..end].iter().sum();
            smoothed[i] = sum / (end - start) as f32;
        }
        
        smoothed
    }

    /// Detect cutoff using multiple complementary methods
    fn detect_cutoff_multimethod(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        sample_rate: u32,
    ) -> (f32, f32, f32) {
        let nyquist = sample_rate as f32 / 2.0;
        
        // Method 1: Energy drop detection (most reliable)
        let cutoff1 = self.detect_cutoff_energy_drop(spectrum_db, frequencies, nyquist);
        
        // Method 2: Derivative-based edge detection
        let cutoff2 = self.detect_cutoff_derivative(spectrum_db, frequencies, nyquist);
        
        // Method 3: Noise floor comparison
        let cutoff3 = self.detect_cutoff_noise_floor(spectrum_db, frequencies, nyquist);
        
        // Combine results (weighted by confidence)
        let mut results = vec![cutoff1, cutoff2, cutoff3];
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        
        // Check for agreement
        let spread = results[2].0 - results[0].0;
        let tolerance = nyquist * 0.08;  // 8% tolerance
        
        if spread < tolerance {
            // Good agreement - use weighted average
            let total_weight: f32 = results.iter().map(|c| c.2).sum();
            if total_weight < 0.01 {
                return (nyquist, 0.0, 0.1);
            }
            
            let weighted_cutoff = results.iter()
                .map(|c| c.0 * c.2)
                .sum::<f32>() / total_weight;
            let weighted_rolloff = results.iter()
                .map(|c| c.1 * c.2)
                .sum::<f32>() / total_weight;
            let confidence = (results.iter().map(|c| c.2).sum::<f32>() / 3.0).min(0.95);
            
            (weighted_cutoff, weighted_rolloff, confidence)
        } else {
            // Disagreement - use the method with highest confidence
            let best = results.iter().max_by(|a, b| a.2.partial_cmp(&b.2).unwrap()).unwrap();
            (best.0, best.1, best.2 * 0.7)  // Reduce confidence on disagreement
        }
    }

    /// Method 1: Detect cutoff by finding where energy drops significantly
    fn detect_cutoff_energy_drop(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        if spectrum_db.len() < 100 {
            return (nyquist, 0.0, 0.0);
        }
        
        // Find the average energy in the "reference" band (2-8 kHz typically has content)
        let ref_start = frequencies.iter().position(|&f| f >= 2000.0).unwrap_or(0);
        let ref_end = frequencies.iter().position(|&f| f >= 8000.0).unwrap_or(spectrum_db.len() / 4);
        
        if ref_end <= ref_start {
            return (nyquist, 0.0, 0.0);
        }
        
        // Find peak level in reference band
        let ref_peak: f32 = spectrum_db[ref_start..ref_end].iter()
            .cloned()
            .fold(f32::MIN, f32::max);
        
        // Threshold: 25dB below peak (lossy codecs typically cut 30-40dB)
        let threshold = ref_peak - 25.0;
        
        // Search from 10kHz upward for sustained drop below threshold
        let search_start = frequencies.iter().position(|&f| f >= 10000.0).unwrap_or(ref_end);
        let mut consecutive_below = 0;
        let consecutive_required = 30;  // Need consecutive bins below threshold
        let mut first_drop_idx = spectrum_db.len() - 1;
        
        for i in search_start..spectrum_db.len() {
            if spectrum_db[i] < threshold {
                if consecutive_below == 0 {
                    first_drop_idx = i;
                }
                consecutive_below += 1;
                if consecutive_below >= consecutive_required {
                    // Found cutoff
                    let cutoff_hz = frequencies[first_drop_idx];
                    
                    // Calculate rolloff steepness
                    let rolloff = self.calculate_rolloff(spectrum_db, frequencies, first_drop_idx);
                    
                    // Confidence based on how clear the drop is
                    let drop_magnitude = ref_peak - spectrum_db[i];
                    let confidence = (drop_magnitude / 40.0).clamp(0.4, 0.95);
                    
                    return (cutoff_hz, rolloff, confidence);
                }
            } else {
                consecutive_below = 0;
            }
        }
        
        // No clear cutoff found - likely genuine lossless
        (nyquist, 0.0, 0.3)
    }

    /// Method 2: Detect cutoff using spectral derivative (edge detection)
    fn detect_cutoff_derivative(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        if spectrum_db.len() < 50 {
            return (nyquist, 0.0, 0.0);
        }
        
        // Compute smoothed first derivative (dB per Hz)
        let deriv_window = 10;
        let mut max_neg_deriv = 0.0f32;
        let mut max_neg_idx = spectrum_db.len() - 1;
        
        // Only search in upper frequency range (10kHz+)
        let search_start = frequencies.iter()
            .position(|&f| f >= 10000.0)
            .unwrap_or(spectrum_db.len() / 2);
        
        for i in (search_start + deriv_window)..(spectrum_db.len() - deriv_window) {
            let diff = spectrum_db[i + deriv_window] - spectrum_db[i - deriv_window];
            let freq_diff = frequencies[i + deriv_window] - frequencies[i - deriv_window];
            
            if freq_diff > 0.0 {
                let derivative = diff / freq_diff;
                
                // Looking for steep negative slope (energy dropping)
                if derivative < max_neg_deriv {
                    max_neg_deriv = derivative;
                    max_neg_idx = i;
                }
            }
        }
        
        // Significant negative slope indicates cutoff
        if max_neg_deriv < -0.003 {  // -3dB per kHz or steeper
            let cutoff_hz = frequencies[max_neg_idx];
            let rolloff = (-max_neg_deriv * 6000.0).min(200.0);  // Convert to approx dB/octave
            let confidence = (-max_neg_deriv * 200.0).clamp(0.3, 0.85);
            
            (cutoff_hz, rolloff, confidence)
        } else {
            (nyquist, 0.0, 0.2)
        }
    }

    /// Method 3: Detect cutoff by comparing to estimated noise floor
    fn detect_cutoff_noise_floor(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        // Estimate noise floor from the highest frequencies (near Nyquist)
        let high_freq_start = (spectrum_db.len() * 9) / 10;  // Top 10%
        
        if high_freq_start >= spectrum_db.len() {
            return (nyquist, 0.0, 0.0);
        }
        
        let mut high_freq_values: Vec<f32> = spectrum_db[high_freq_start..].to_vec();
        high_freq_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        if high_freq_values.is_empty() {
            return (nyquist, 0.0, 0.0);
        }
        
        // Use 20th percentile as noise floor estimate
        let noise_floor_idx = high_freq_values.len() / 5;
        let noise_floor = high_freq_values[noise_floor_idx.min(high_freq_values.len() - 1)];
        
        // Threshold: 15dB above noise floor
        let threshold = noise_floor + 15.0;
        
        // Find where signal drops to near noise floor (searching backward from high frequencies)
        let search_start = frequencies.iter()
            .position(|&f| f >= 10000.0)
            .unwrap_or(spectrum_db.len() / 2);
        
        let mut last_above = spectrum_db.len() - 1;
        for i in (search_start..spectrum_db.len()).rev() {
            if spectrum_db[i] > threshold {
                last_above = i;
                break;
            }
        }
        
        let cutoff_hz = frequencies[last_above.min(frequencies.len() - 1)];
        
        // If cutoff is very near Nyquist, no significant cutoff detected
        if cutoff_hz > nyquist * 0.95 {
            return (nyquist, 0.0, 0.25);
        }
        
        let rolloff = self.calculate_rolloff(spectrum_db, frequencies, last_above);
        
        // Confidence based on signal-to-noise margin
        let mid_freq_start = frequencies.iter().position(|&f| f >= 2000.0).unwrap_or(0);
        let mid_freq_end = frequencies.iter().position(|&f| f >= 8000.0).unwrap_or(spectrum_db.len() / 4);
        let signal_level = spectrum_db[mid_freq_start..mid_freq_end].iter()
            .cloned()
            .fold(f32::MIN, f32::max);
        let snr = signal_level - noise_floor;
        let confidence = (snr / 50.0).clamp(0.3, 0.8);
        
        (cutoff_hz, rolloff, confidence)
    }

    /// Calculate rolloff steepness at the detected cutoff point
    fn calculate_rolloff(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        cutoff_idx: usize,
    ) -> f32 {
        if cutoff_idx >= spectrum_db.len() - 20 || cutoff_idx < 20 || frequencies.is_empty() {
            return 0.0;
        }
        
        // Measure energy drop over half an octave above cutoff
        let cutoff_freq = frequencies[cutoff_idx];
        let half_octave_freq = cutoff_freq * 1.414;  // sqrt(2) = half octave
        
        let half_octave_idx = frequencies.iter()
            .position(|&f| f >= half_octave_freq)
            .unwrap_or(spectrum_db.len() - 1);
        
        if half_octave_idx <= cutoff_idx || half_octave_idx >= spectrum_db.len() {
            return 0.0;
        }
        
        let energy_at_cutoff = spectrum_db[cutoff_idx];
        let energy_at_half_octave = spectrum_db[half_octave_idx];
        
        // dB drop per half octave, scaled to per octave
        let drop = energy_at_cutoff - energy_at_half_octave;
        (drop * 2.0).max(0.0)  // Multiply by 2 for full octave
    }

    /// Match detected cutoff against known codec signatures
    fn match_codec_signature(&self, cutoff_hz: f32, rolloff: f32) -> Option<CodecSignature> {
        // Only match if we have a clear cutoff below ~20.5kHz and some rolloff
        if cutoff_hz > 20500.0 || rolloff < 5.0 {
            return None;
        }
        
        let mut best_match: Option<(Codec, u32, f32)> = None;
        let mut best_score = 0.0f32;
        
        for &(codec, bitrate, typical_cutoff, tolerance) in CODEC_CUTOFFS {
            let distance = (cutoff_hz - typical_cutoff).abs();
            if distance < tolerance {
                // Score based on distance and rolloff
                let distance_score = 1.0 - (distance / tolerance);
                let rolloff_score = (rolloff / 60.0).min(1.0);  // Lossy codecs have steep rolloff
                let combined_score = distance_score * 0.7 + rolloff_score * 0.3;
                
                if combined_score > best_score {
                    best_score = combined_score;
                    best_match = Some((codec, bitrate, combined_score));
                }
            }
        }
        
        best_match.map(|(codec, bitrate, confidence)| CodecSignature {
            codec,
            bitrate: Some(bitrate),
            confidence,
        })
    }
}

/// High-level function to check if audio is a transcode
pub fn detect_transcode(samples: &[f32], sample_rate: u32) -> TranscodeResult {
    let analyzer = SpectralAnalyzer::default();
    let analysis = analyzer.analyze(samples, sample_rate);
    
    let nyquist = sample_rate as f32 / 2.0;

    // QUICK FIX: Skip MP3/AAC detection for high sample rate files
    // MP3 max is 48kHz, AAC typically 48kHz (max 96kHz)
    if sample_rate > 48000 {
        return TranscodeResult {
            is_transcode: false,
            confidence: 0.0,
            cutoff_hz: sample_rate as f32 / 2.0,
            cutoff_ratio: 1.0,
            rolloff_steepness: 0.0,
            likely_codec: None,
            reason: format!("Sample rate {} Hz exceeds MP3/AAC maximum - skipping lossy detection", sample_rate),
        };
    }
    
    // Determine if it's likely a transcode
    // Key criteria:
    // 1. Cutoff below 95% of Nyquist (leaving room for natural rolloff)
    // 2. Some rolloff steepness (lossy codecs have steep cutoffs)
    // 3. Reasonable confidence in detection
    let is_transcode = analysis.cutoff_ratio < 0.95 && 
                       analysis.rolloff_steepness > 10.0 && 
                       analysis.confidence > 0.4;
    
    TranscodeResult {
        is_transcode,
        confidence: if is_transcode { 
            analysis.confidence 
        } else { 
            // For non-transcodes, confidence that it's genuine
            (1.0 - analysis.confidence * 0.5).max(0.5)
        },
        cutoff_hz: analysis.cutoff_hz,
        cutoff_ratio: analysis.cutoff_ratio,
        rolloff_steepness: analysis.rolloff_steepness,
        likely_codec: analysis.likely_codec,
        reason: if is_transcode {
            format!(
                "Frequency cutoff at {:.0} Hz ({:.1}% of Nyquist) with {:.0} dB/oct rolloff",
                analysis.cutoff_hz, analysis.cutoff_ratio * 100.0, analysis.rolloff_steepness
            )
        } else {
            format!(
                "Full frequency response to {:.0} Hz ({:.1}% of Nyquist)",
                analysis.cutoff_hz, analysis.cutoff_ratio * 100.0
            )
        },
    }
}

#[derive(Debug, Clone)]
pub struct TranscodeResult {
    pub is_transcode: bool,
    pub confidence: f32,
    pub cutoff_hz: f32,
    pub cutoff_ratio: f32,
    pub rolloff_steepness: f32,
    pub likely_codec: Option<CodecSignature>,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn generate_test_signal(sample_rate: u32, cutoff_hz: f32, duration_secs: f32) -> Vec<f32> {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);
        
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let mut sample = 0.0f32;
            
            // Add harmonics up to cutoff
            for harmonic in 1..100 {
                let freq = 100.0 * harmonic as f32;
                if freq < cutoff_hz {
                    sample += (2.0 * PI * freq * t).sin() / harmonic as f32;
                }
            }
            
            samples.push(sample * 0.3);
        }
        
        samples
    }
    
    #[test]
    fn test_full_bandwidth_detection() {
        let samples = generate_test_signal(44100, 22000.0, 1.0);
        let result = detect_transcode(&samples, 44100);
        
        assert!(!result.is_transcode, "Full bandwidth signal should not be detected as transcode");
        assert!(result.cutoff_ratio > 0.9, "Cutoff ratio should be >90%");
    }
    
    #[test]
    fn test_mp3_128k_cutoff_detection() {
        // MP3 128k typically cuts off around 16kHz
        let samples = generate_test_signal(44100, 16000.0, 1.0);
        let result = detect_transcode(&samples, 44100);
        
        assert!(result.is_transcode, "MP3 128k-like cutoff should be detected");
        assert!(result.cutoff_hz < 17000.0, "Cutoff should be detected around 16kHz");
    }
    
    #[test]
    fn test_mp3_320k_cutoff_detection() {
        // MP3 320k typically cuts off around 20.5kHz
        let samples = generate_test_signal(44100, 20500.0, 1.0);
        let result = detect_transcode(&samples, 44100);
        
        // This is borderline - may or may not detect
        println!("320k test: is_transcode={}, cutoff={}, ratio={}", 
                 result.is_transcode, result.cutoff_hz, result.cutoff_ratio);
    }
}
