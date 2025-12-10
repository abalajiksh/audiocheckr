// src/core/analysis/spectral.rs
//
// Spectral analysis for detecting lossy codec signatures
// Uses FFT to analyze frequency content and detect characteristic cutoffs

use std::collections::HashMap;
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::PI;

/// Spectral analysis results
#[derive(Debug, Clone, Default)]
pub struct SpectralAnalysis {
    /// Detected frequency cutoff in Hz (where energy drops significantly)
    pub frequency_cutoff: f32,
    /// Spectral rolloff point (85% energy frequency)
    pub spectral_rolloff: f32,
    /// Steepness of the rolloff in dB/octave
    pub rolloff_steepness: f32,
    /// Whether a sharp "brick-wall" cutoff was detected
    pub has_brick_wall: bool,
    /// Spectral flatness (0=tonal, 1=noise-like)
    pub spectral_flatness: f32,
    /// Matched codec signature if detected
    pub matched_signature: Option<String>,
    /// Confidence in the signature match
    pub signature_confidence: f32,
    /// Evidence for the detection
    pub evidence: Vec<String>,
}

/// Known codec spectral signatures
#[derive(Debug, Clone)]
pub struct SpectralSignature {
    pub name: String,
    pub cutoff_frequencies: Vec<f32>,
    pub rolloff_characteristics: Vec<f32>,
    pub typical_spectral_features: HashMap<String, f32>,
}

/// Spectral analyzer with configurable parameters
pub struct SpectralAnalyzer {
    fft_size: usize,
    hop_size: usize,
    sample_rate: u32,
    window: Vec<f32>,
}

impl SpectralAnalyzer {
    pub fn new(fft_size: usize, hop_size: usize, sample_rate: u32) -> Self {
        // Create Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / fft_size as f32).cos()))
            .collect();
        
        Self {
            fft_size,
            hop_size,
            sample_rate,
            window,
        }
    }
    
    /// Perform comprehensive spectral analysis
    pub fn analyze(&self, samples: &[f32]) -> SpectralAnalysis {
        let nyquist = self.sample_rate as f32 / 2.0;
        
        if samples.len() < self.fft_size {
            return SpectralAnalysis {
                frequency_cutoff: nyquist,
                spectral_rolloff: nyquist * 0.85,
                rolloff_steepness: 0.0,
                has_brick_wall: false,
                spectral_flatness: 0.5,
                matched_signature: None,
                signature_confidence: 0.0,
                evidence: vec!["Insufficient samples for analysis".to_string()],
            };
        }
        
        // Compute average spectrum across multiple frames
        let avg_spectrum = self.compute_average_spectrum(samples);
        let spectrum_db = self.to_db_spectrum(&avg_spectrum);
        
        // Find the frequency cutoff
        let (cutoff_hz, cutoff_confidence) = self.detect_frequency_cutoff(&spectrum_db);
        
        // Calculate spectral rolloff (where 85% of energy is contained)
        let rolloff_hz = self.calculate_spectral_rolloff(&avg_spectrum, 0.85);
        
        // Calculate rolloff steepness
        let steepness = self.calculate_rolloff_steepness(&spectrum_db, cutoff_hz);
        
        // Detect brick-wall cutoff
        let has_brick_wall = steepness > 60.0 && cutoff_hz < nyquist * 0.95;
        
        // Calculate spectral flatness
        let flatness = self.calculate_spectral_flatness(&avg_spectrum);
        
        // Match against known codec signatures
        let signatures = get_encoder_signatures();
        let (matched_sig, sig_confidence) = self.match_codec_signature(
            cutoff_hz, steepness, has_brick_wall, &signatures
        );
        
        // Build evidence
        let mut evidence = Vec::new();
        if cutoff_hz < nyquist * 0.95 {
            evidence.push(format!(
                "Frequency cutoff detected at {:.0} Hz ({:.1}% of Nyquist)",
                cutoff_hz, cutoff_hz / nyquist * 100.0
            ));
        }
        if has_brick_wall {
            evidence.push(format!(
                "Brick-wall cutoff detected with {:.1} dB/octave rolloff",
                steepness
            ));
        }
        if let Some(ref sig) = matched_sig {
            evidence.push(format!(
                "Matches {} signature with {:.0}% confidence",
                sig, sig_confidence * 100.0
            ));
        }
        
        SpectralAnalysis {
            frequency_cutoff: cutoff_hz,
            spectral_rolloff: rolloff_hz,
            rolloff_steepness: steepness,
            has_brick_wall,
            spectral_flatness: flatness,
            matched_signature: matched_sig,
            signature_confidence: sig_confidence,
            evidence,
        }
    }
    
    /// Compute average magnitude spectrum across multiple frames
    fn compute_average_spectrum(&self, samples: &[f32]) -> Vec<f32> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        let num_frames = (samples.len().saturating_sub(self.fft_size)) / self.hop_size + 1;
        let num_frames = num_frames.min(100); // Limit to 100 frames for efficiency
        
        let spectrum_size = self.fft_size / 2;
        let mut avg_spectrum = vec![0.0f32; spectrum_size];
        let mut frame_count = 0;
        
        for i in 0..num_frames {
            let start = i * self.hop_size;
            let end = (start + self.fft_size).min(samples.len());
            
            if end - start < self.fft_size {
                break;
            }
            
            // Apply window and prepare FFT buffer
            let mut buffer: Vec<Complex<f32>> = (0..self.fft_size)
                .map(|j| {
                    let sample = samples[start + j];
                    Complex::new(sample * self.window[j], 0.0)
                })
                .collect();
            
            fft.process(&mut buffer);
            
            // Accumulate magnitude
            for (j, c) in buffer.iter().take(spectrum_size).enumerate() {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                avg_spectrum[j] += mag;
            }
            frame_count += 1;
        }
        
        // Average
        if frame_count > 0 {
            for val in &mut avg_spectrum {
                *val /= frame_count as f32;
            }
        }
        
        avg_spectrum
    }
    
    /// Convert magnitude spectrum to dB
    fn to_db_spectrum(&self, spectrum: &[f32]) -> Vec<f32> {
        spectrum.iter()
            .map(|&mag| {
                if mag > 1e-10 {
                    20.0 * mag.log10()
                } else {
                    -200.0
                }
            })
            .collect()
    }
    
    /// Detect frequency cutoff by finding where energy drops sharply
    fn detect_frequency_cutoff(&self, spectrum_db: &[f32]) -> (f32, f32) {
        let nyquist = self.sample_rate as f32 / 2.0;
        let bin_width = nyquist / spectrum_db.len() as f32;
        
        // Skip very low frequencies (below 1kHz)
        let start_bin = (1000.0 / bin_width) as usize;
        
        // Calculate smoothed spectrum (moving average)
        let window_size = 10;
        let smoothed: Vec<f32> = (0..spectrum_db.len())
            .map(|i| {
                let start = i.saturating_sub(window_size / 2);
                let end = (i + window_size / 2).min(spectrum_db.len());
                let sum: f32 = spectrum_db[start..end].iter().sum();
                sum / (end - start) as f32
            })
            .collect();
        
        // Find peak level in mid-frequencies (1-10kHz)
        let mid_end_bin = ((10000.0 / bin_width) as usize).min(spectrum_db.len());
        let peak_level = smoothed[start_bin..mid_end_bin].iter()
            .fold(f32::MIN, |a, &b| a.max(b));
        
        // Look for where spectrum drops significantly below peak
        let threshold = peak_level - 30.0; // 30dB below peak
        
        let mut cutoff_bin = spectrum_db.len() - 1;
        let mut found_cutoff = false;
        
        // Search from high frequencies down
        for i in (start_bin..spectrum_db.len()).rev() {
            if smoothed[i] > threshold {
                cutoff_bin = i;
                found_cutoff = true;
                break;
            }
        }
        
        let cutoff_hz = cutoff_bin as f32 * bin_width;
        let confidence = if found_cutoff && cutoff_hz < nyquist * 0.95 {
            0.8
        } else {
            0.3
        };
        
        (cutoff_hz.min(nyquist), confidence)
    }
    
    /// Calculate spectral rolloff point
    fn calculate_spectral_rolloff(&self, spectrum: &[f32], percentile: f32) -> f32 {
        let nyquist = self.sample_rate as f32 / 2.0;
        let bin_width = nyquist / spectrum.len() as f32;
        
        let total_energy: f32 = spectrum.iter().map(|m| m * m).sum();
        let threshold = total_energy * percentile;
        
        let mut cumulative = 0.0f32;
        for (i, &mag) in spectrum.iter().enumerate() {
            cumulative += mag * mag;
            if cumulative >= threshold {
                return i as f32 * bin_width;
            }
        }
        
        nyquist
    }
    
    /// Calculate rolloff steepness in dB/octave
    fn calculate_rolloff_steepness(&self, spectrum_db: &[f32], cutoff_hz: f32) -> f32 {
        let nyquist = self.sample_rate as f32 / 2.0;
        let bin_width = nyquist / spectrum_db.len() as f32;
        
        // Find bin at cutoff frequency
        let cutoff_bin = ((cutoff_hz / bin_width) as usize).min(spectrum_db.len() - 1);
        
        // Measure level at cutoff and one octave below
        let octave_below_hz = cutoff_hz / 2.0;
        let octave_below_bin = ((octave_below_hz / bin_width) as usize).max(1);
        
        if cutoff_bin <= octave_below_bin {
            return 0.0;
        }
        
        // Average levels around these points
        let level_at_cutoff = average_db(&spectrum_db, cutoff_bin, 5);
        let level_below = average_db(&spectrum_db, octave_below_bin, 5);
        
        // dB difference over one octave
        let db_diff = level_below - level_at_cutoff;
        db_diff.max(0.0)
    }
    
    /// Calculate spectral flatness (Wiener entropy)
    fn calculate_spectral_flatness(&self, spectrum: &[f32]) -> f32 {
        let n = spectrum.len() as f32;
        
        // Geometric mean (via log)
        let log_sum: f32 = spectrum.iter()
            .map(|&m| (m + 1e-10).ln())
            .sum();
        let geometric_mean = (log_sum / n).exp();
        
        // Arithmetic mean
        let arithmetic_mean = spectrum.iter().sum::<f32>() / n;
        
        if arithmetic_mean < 1e-10 {
            return 0.0;
        }
        
        (geometric_mean / arithmetic_mean).min(1.0)
    }
    
    /// Match detected characteristics against known codec signatures
    fn match_codec_signature(
        &self,
        cutoff_hz: f32,
        steepness: f32,
        has_brick_wall: bool,
        signatures: &[SpectralSignature],
    ) -> (Option<String>, f32) {
        let mut best_match: Option<String> = None;
        let mut best_confidence = 0.0f32;
        
        for sig in signatures {
            for &expected_cutoff in &sig.cutoff_frequencies {
                let cutoff_diff = (cutoff_hz - expected_cutoff).abs();
                let cutoff_tolerance = expected_cutoff * 0.05; // 5% tolerance
                
                if cutoff_diff < cutoff_tolerance && has_brick_wall {
                    let confidence = 1.0 - (cutoff_diff / cutoff_tolerance);
                    if confidence > best_confidence {
                        best_confidence = confidence;
                        best_match = Some(sig.name.clone());
                    }
                }
            }
        }
        
        (best_match, best_confidence)
    }
    
    /// Get FFT size
    pub fn fft_size(&self) -> usize {
        self.fft_size
    }
}

/// Helper function to average dB values around a bin
fn average_db(spectrum_db: &[f32], center: usize, width: usize) -> f32 {
    let start = center.saturating_sub(width);
    let end = (center + width).min(spectrum_db.len());
    let sum: f32 = spectrum_db[start..end].iter().sum();
    sum / (end - start) as f32
}

/// Get built-in encoder signatures with known cutoff frequencies
pub fn get_encoder_signatures() -> Vec<SpectralSignature> {
    vec![
        SpectralSignature {
            name: "MP3 64kbps".to_string(),
            cutoff_frequencies: vec![11025.0, 12000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "MP3 128kbps".to_string(),
            cutoff_frequencies: vec![16000.0, 15500.0, 16500.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "MP3 192kbps".to_string(),
            cutoff_frequencies: vec![18500.0, 19000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "MP3 256kbps".to_string(),
            cutoff_frequencies: vec![19500.0, 20000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "MP3 320kbps".to_string(),
            cutoff_frequencies: vec![20000.0, 20500.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "AAC 128kbps".to_string(),
            cutoff_frequencies: vec![16000.0, 17000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "AAC 256kbps".to_string(),
            cutoff_frequencies: vec![20000.0, 19000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "Opus 64kbps".to_string(),
            cutoff_frequencies: vec![12000.0, 13000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "Opus 128kbps".to_string(),
            cutoff_frequencies: vec![20000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "Vorbis Q3".to_string(),
            cutoff_frequencies: vec![16000.0, 17000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
        SpectralSignature {
            name: "Vorbis Q7".to_string(),
            cutoff_frequencies: vec![19000.0, 20000.0],
            rolloff_characteristics: vec![],
            typical_spectral_features: HashMap::new(),
        },
    ]
}

/// Match against known signatures
pub fn match_signature(analysis: &SpectralAnalysis, signatures: &[SpectralSignature]) -> Option<(String, f32)> {
    for sig in signatures {
        for &cutoff in &sig.cutoff_frequencies {
            if (analysis.frequency_cutoff - cutoff).abs() < 500.0 && analysis.has_brick_wall {
                return Some((sig.name.clone(), 0.8));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spectral_analyzer_creation() {
        let analyzer = SpectralAnalyzer::new(4096, 1024, 44100);
        assert_eq!(analyzer.fft_size(), 4096);
    }
    
    #[test]
    fn test_get_signatures() {
        let sigs = get_encoder_signatures();
        assert!(!sigs.is_empty());
        assert!(sigs.iter().any(|s| s.name.contains("MP3")));
    }
}
