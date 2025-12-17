// spectral_improved.rs
// Improved spectral analysis for detecting lossy codec transcodes
// This replaces/supplements the existing spectral.rs that returns 100% for everything

use std::f64::consts::PI;

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
const CODEC_CUTOFFS: &[(Codec, u32, f32, f32)] = &[
    // (Codec, Bitrate, Typical Cutoff Hz, Tolerance Hz)
    (Codec::MP3, 128, 16000.0, 500.0),
    (Codec::MP3, 192, 18500.0, 500.0),
    (Codec::MP3, 256, 19500.0, 500.0),
    (Codec::MP3, 320, 20500.0, 500.0),
    (Codec::AAC, 128, 15500.0, 500.0),
    (Codec::AAC, 192, 18000.0, 500.0),
    (Codec::AAC, 256, 19000.0, 500.0),
    (Codec::Opus, 64, 12000.0, 500.0),
    (Codec::Opus, 128, 20000.0, 500.0),
    (Codec::Opus, 192, 20000.0, 500.0),
    (Codec::Vorbis, 128, 16000.0, 500.0),
    (Codec::Vorbis, 192, 18500.0, 500.0),
];

/// Improved spectral analyzer
pub struct SpectralAnalyzer {
    /// FFT size (larger = better frequency resolution)
    fft_size: usize,
    /// Overlap between FFT windows (0.0 - 0.99)
    overlap: f32,
    /// Number of windows to average
    num_windows: usize,
    /// Smoothing factor for spectrum
    smoothing_bins: usize,
}

impl Default for SpectralAnalyzer {
    fn default() -> Self {
        Self {
            fft_size: 8192,       // Good balance of resolution and speed
            overlap: 0.75,        // 75% overlap for better averaging
            num_windows: 64,      // Average 64 windows
            smoothing_bins: 16,   // Smooth over 16 bins
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
        
        // Step 1: Compute averaged magnitude spectrum
        let spectrum_linear = self.compute_averaged_spectrum(samples);
        
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

    /// Compute averaged magnitude spectrum using Welch's method
    fn compute_averaged_spectrum(&self, samples: &[f32]) -> Vec<f32> {
        let hop_size = ((1.0 - self.overlap) * self.fft_size as f32) as usize;
        let num_windows = ((samples.len() - self.fft_size) / hop_size + 1).min(self.num_windows);
        
        if num_windows == 0 {
            return vec![0.0; self.fft_size / 2];
        }
        
        let mut spectrum_sum = vec![0.0f64; self.fft_size / 2];
        let window = self.hann_window(self.fft_size);
        
        for w in 0..num_windows {
            let start = w * hop_size;
            if start + self.fft_size > samples.len() {
                break;
            }
            
            // Apply window
            let windowed: Vec<f32> = samples[start..start + self.fft_size]
                .iter()
                .zip(window.iter())
                .map(|(&s, &w)| s * w)
                .collect();
            
            // Compute FFT magnitude
            let magnitudes = self.compute_fft_magnitude(&windowed);
            
            for (i, &mag) in magnitudes.iter().enumerate() {
                spectrum_sum[i] += mag as f64;
            }
        }
        
        // Average
        spectrum_sum.iter().map(|&s| (s / num_windows as f64) as f32).collect()
    }

    /// Compute FFT magnitude spectrum
    fn compute_fft_magnitude(&self, samples: &[f32]) -> Vec<f32> {
        // Using simple DFT for demonstration - replace with rustfft in production
        let n = samples.len();
        let mut magnitudes = vec![0.0f32; n / 2];
        
        // Only compute up to Nyquist
        for k in 0..n/2 {
            let mut real = 0.0f64;
            let mut imag = 0.0f64;
            
            for (i, &sample) in samples.iter().enumerate() {
                let angle = -2.0 * PI * k as f64 * i as f64 / n as f64;
                real += sample as f64 * angle.cos();
                imag += sample as f64 * angle.sin();
            }
            
            magnitudes[k] = ((real * real + imag * imag).sqrt() / n as f64) as f32;
        }
        
        magnitudes
    }

    /// Generate Hann window
    fn hann_window(&self, size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (size - 1) as f64).cos()) as f32)
            .collect()
    }

    /// Convert to dB scale and apply smoothing
    fn to_db_smoothed(&self, spectrum: &[f32]) -> Vec<f32> {
        // Convert to dB
        let spectrum_db: Vec<f32> = spectrum.iter()
            .map(|&mag| {
                if mag > 1e-10 {
                    20.0 * mag.log10()
                } else {
                    -200.0
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
        
        // Method 1: Energy drop detection
        let cutoff1 = self.detect_cutoff_energy_drop(spectrum_db, frequencies, nyquist);
        
        // Method 2: Derivative-based edge detection
        let cutoff2 = self.detect_cutoff_derivative(spectrum_db, frequencies, nyquist);
        
        // Method 3: Noise floor comparison
        let cutoff3 = self.detect_cutoff_noise_floor(spectrum_db, frequencies, nyquist);
        
        // Combine results (weighted average of agreeing methods)
        let mut cutoffs = vec![cutoff1, cutoff2, cutoff3];
        cutoffs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        
        // Check for agreement
        let spread = cutoffs[2].0 - cutoffs[0].0;
        let tolerance = nyquist * 0.05;  // 5% of Nyquist
        
        if spread < tolerance {
            // Good agreement - use weighted average
            let total_weight: f32 = cutoffs.iter().map(|c| c.2).sum();
            let weighted_cutoff = cutoffs.iter()
                .map(|c| c.0 * c.2)
                .sum::<f32>() / total_weight;
            let weighted_rolloff = cutoffs.iter()
                .map(|c| c.1 * c.2)
                .sum::<f32>() / total_weight;
            let confidence = cutoffs.iter().map(|c| c.2).sum::<f32>() / 3.0;
            
            (weighted_cutoff, weighted_rolloff, confidence.min(1.0))
        } else {
            // Disagreement - use median with lower confidence
            let median_idx = 1;
            (cutoffs[median_idx].0, cutoffs[median_idx].1, cutoffs[median_idx].2 * 0.6)
        }
    }

    /// Method 1: Detect cutoff by finding where energy drops significantly
    fn detect_cutoff_energy_drop(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        // Find the average energy in the "reference" band (1-10 kHz typically has content)
        let ref_start = frequencies.iter().position(|&f| f >= 1000.0).unwrap_or(0);
        let ref_end = frequencies.iter().position(|&f| f >= 10000.0).unwrap_or(spectrum_db.len() / 2);
        
        if ref_end <= ref_start {
            return (nyquist, 0.0, 0.0);
        }
        
        let ref_energy: f32 = spectrum_db[ref_start..ref_end].iter().sum::<f32>() 
            / (ref_end - ref_start) as f32;
        
        // Threshold: 20dB below reference
        let threshold = ref_energy - 20.0;
        
        // Search from high frequencies down for sustained drop
        let search_start = frequencies.iter().position(|&f| f >= 12000.0).unwrap_or(ref_end);
        let mut consecutive_below = 0;
        let consecutive_required = 20;  // Need ~20 consecutive bins below threshold
        
        for i in (search_start..spectrum_db.len()).rev() {
            if spectrum_db[i] < threshold {
                consecutive_below += 1;
                if consecutive_below >= consecutive_required {
                    // Found cutoff - calculate rolloff
                    let cutoff_idx = (i + consecutive_required).min(spectrum_db.len() - 1);
                    let cutoff_hz = frequencies[cutoff_idx];
                    
                    // Calculate rolloff steepness
                    let rolloff = self.calculate_rolloff(spectrum_db, frequencies, cutoff_idx);
                    
                    // Confidence based on how clear the drop is
                    let drop_magnitude = ref_energy - spectrum_db[i];
                    let confidence = (drop_magnitude / 30.0).clamp(0.3, 0.9);
                    
                    return (cutoff_hz, rolloff, confidence);
                }
            } else {
                consecutive_below = 0;
            }
        }
        
        // No clear cutoff found
        (nyquist, 0.0, 0.3)
    }

    /// Method 2: Detect cutoff using spectral derivative (edge detection)
    fn detect_cutoff_derivative(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        if spectrum_db.len() < 10 {
            return (nyquist, 0.0, 0.0);
        }
        
        // Compute smoothed first derivative
        let mut derivative = Vec::with_capacity(spectrum_db.len());
        let deriv_window = 5;
        
        for i in deriv_window..spectrum_db.len() - deriv_window {
            let diff = spectrum_db[i + deriv_window] - spectrum_db[i - deriv_window];
            let freq_diff = frequencies[i + deriv_window] - frequencies[i - deriv_window];
            if freq_diff > 0.0 {
                derivative.push((i, diff / freq_diff));
            }
        }
        
        // Find the most negative derivative (steepest drop) in upper frequencies
        let search_start = derivative.iter()
            .position(|(i, _)| frequencies[*i] >= 12000.0)
            .unwrap_or(0);
        
        let mut min_deriv = 0.0f32;
        let mut min_idx = derivative.len() - 1;
        
        for &(idx, deriv) in &derivative[search_start..] {
            if deriv < min_deriv {
                min_deriv = deriv;
                min_idx = idx;
            }
        }
        
        // Threshold for significant cutoff
        if min_deriv < -0.001 {  // Significant negative slope
            let cutoff_hz = frequencies[min_idx];
            let rolloff = (-min_deriv * 1000.0 * 6.0) as f32;  // Convert to dB/octave approx
            let confidence = (-min_deriv * 1000.0).clamp(0.3, 0.85);
            
            (cutoff_hz, rolloff, confidence as f32)
        } else {
            (nyquist, 0.0, 0.3)
        }
    }

    /// Method 3: Detect cutoff by comparing to estimated noise floor
    fn detect_cutoff_noise_floor(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        nyquist: f32,
    ) -> (f32, f32, f32) {
        // Estimate noise floor from the quietest 10% of spectrum above 15kHz
        let high_freq_start = frequencies.iter()
            .position(|&f| f >= 15000.0)
            .unwrap_or(spectrum_db.len() * 3 / 4);
        
        let mut high_freq_values: Vec<f32> = spectrum_db[high_freq_start..].to_vec();
        high_freq_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        if high_freq_values.is_empty() {
            return (nyquist, 0.0, 0.0);
        }
        
        // Use 10th percentile as noise floor estimate
        let noise_floor_idx = high_freq_values.len() / 10;
        let noise_floor = high_freq_values[noise_floor_idx];
        
        // Threshold: 10dB above noise floor
        let threshold = noise_floor + 10.0;
        
        // Find where signal drops to near noise floor (searching backward from Nyquist)
        let mut last_above = spectrum_db.len() - 1;
        for i in (0..spectrum_db.len()).rev() {
            if spectrum_db[i] > threshold {
                last_above = i;
                break;
            }
        }
        
        let cutoff_hz = frequencies[last_above];
        
        // If cutoff is near Nyquist, no significant cutoff detected
        if cutoff_hz > nyquist * 0.95 {
            return (nyquist, 0.0, 0.3);
        }
        
        let rolloff = self.calculate_rolloff(spectrum_db, frequencies, last_above);
        
        // Confidence based on signal-to-noise margin
        let signal_level = spectrum_db[..last_above].iter()
            .skip(spectrum_db.len() / 4)
            .cloned()
            .fold(f32::MIN, f32::max);
        let snr = signal_level - noise_floor;
        let confidence = (snr / 40.0).clamp(0.3, 0.85);
        
        (cutoff_hz, rolloff, confidence)
    }

    /// Calculate rolloff steepness at the detected cutoff point
    fn calculate_rolloff(
        &self,
        spectrum_db: &[f32],
        frequencies: &[f32],
        cutoff_idx: usize,
    ) -> f32 {
        if cutoff_idx >= spectrum_db.len() - 10 || cutoff_idx < 10 {
            return 0.0;
        }
        
        // Measure energy drop over one octave above cutoff
        let cutoff_freq = frequencies[cutoff_idx];
        let octave_freq = cutoff_freq * 2.0;
        
        let octave_idx = frequencies.iter()
            .position(|&f| f >= octave_freq)
            .unwrap_or(spectrum_db.len() - 1);
        
        if octave_idx <= cutoff_idx {
            return 0.0;
        }
        
        let energy_at_cutoff = spectrum_db[cutoff_idx];
        let energy_at_octave = spectrum_db[octave_idx];
        
        // dB drop per octave
        (energy_at_cutoff - energy_at_octave).max(0.0)
    }

    /// Match detected cutoff against known codec signatures
    fn match_codec_signature(&self, cutoff_hz: f32, rolloff: f32) -> Option<CodecSignature> {
        // Only match if we have a clear cutoff below ~20kHz and significant rolloff
        if cutoff_hz > 20000.0 || rolloff < 10.0 {
            return None;
        }
        
        let mut best_match: Option<(Codec, u32, f32)> = None;
        let mut best_distance = f32::MAX;
        
        for &(codec, bitrate, typical_cutoff, tolerance) in CODEC_CUTOFFS {
            let distance = (cutoff_hz - typical_cutoff).abs();
            if distance < tolerance && distance < best_distance {
                best_distance = distance;
                best_match = Some((codec, bitrate, 1.0 - distance / tolerance));
            }
        }
        
        best_match.map(|(codec, bitrate, confidence)| CodecSignature {
            codec,
            bitrate: Some(bitrate),
            confidence: confidence * (rolloff / 100.0).min(1.0),
        })
    }
}

/// High-level function to check if audio is a transcode
pub fn detect_transcode(samples: &[f32], sample_rate: u32) -> TranscodeResult {
    let analyzer = SpectralAnalyzer::default();
    let analysis = analyzer.analyze(samples, sample_rate);
    
    let nyquist = sample_rate as f32 / 2.0;
    
    // Determine if it's likely a transcode
    let is_transcode = analysis.cutoff_ratio < 0.92 && // Cutoff below 92% of Nyquist
                       analysis.rolloff_steepness > 20.0 && // Sharp rolloff
                       analysis.confidence > 0.5;          // Reasonable confidence
    
    TranscodeResult {
        is_transcode,
        confidence: if is_transcode { analysis.confidence } else { 1.0 - analysis.confidence },
        cutoff_hz: analysis.cutoff_hz,
        cutoff_ratio: analysis.cutoff_ratio,
        rolloff_steepness: analysis.rolloff_steepness,
        likely_codec: analysis.likely_codec,
        reason: if is_transcode {
            format!(
                "Frequency cutoff at {:.1} Hz ({:.1}% of Nyquist) with {:.1} dB/oct rolloff",
                analysis.cutoff_hz, analysis.cutoff_ratio * 100.0, analysis.rolloff_steepness
            )
        } else {
            format!(
                "Full frequency response to {:.1} Hz ({:.1}% of Nyquist)",
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
            for harmonic in 1..50 {
                let freq = 100.0 * harmonic as f32;
                if freq < cutoff_hz {
                    sample += (2.0 * PI * freq * t).sin() / harmonic as f32;
                }
            }
            
            samples.push(sample * 0.3);  // Normalize
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
}
