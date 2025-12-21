// src/core/analysis/enf_detection.rs
//
// Electrical Network Frequency (ENF) Analysis for AudioCheckr
//
// ENF analysis detects power grid frequency fluctuations embedded in audio recordings.
// This can be used to:
// - Verify recording authenticity (detect edits/splices)
// - Determine approximate recording date/time (with ENF database)
// - Identify recording location (50Hz vs 60Hz regions)
// - Detect synthetic/generated audio (no ENF present)
//
// Power grid frequencies:
// - 50 Hz: Europe, Africa, Asia, Australia, most of South America
// - 60 Hz: North America, parts of South America, Japan (mixed)
//
// The ENF signal typically appears at the fundamental frequency and harmonics
// (50/100/150/200 Hz or 60/120/180/240 Hz) due to electromagnetic interference
// from power lines and equipment.

use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// ENF detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnfDetectionResult {
    /// Whether ENF signal was detected
    pub enf_detected: bool,
    /// Confidence in detection (0.0-1.0)
    pub confidence: f32,
    /// Detected base frequency (50 or 60 Hz, or None)
    pub base_frequency: Option<EnfBaseFrequency>,
    /// Detected harmonics with their strengths
    pub harmonics: Vec<EnfHarmonic>,
    /// ENF frequency variation over time (for authenticity analysis)
    pub frequency_trace: Vec<EnfMeasurement>,
    /// Detected anomalies (potential edits/splices)
    pub anomalies: Vec<EnfAnomaly>,
    /// Overall ENF stability (higher = more consistent recording)
    pub stability_score: f32,
    /// Signal-to-noise ratio of ENF signal in dB
    pub enf_snr_db: f32,
    /// Estimated recording region based on frequency
    pub estimated_region: Option<EnfRegion>,
    /// Evidence for detection
    pub evidence: Vec<String>,
}

impl Default for EnfDetectionResult {
    fn default() -> Self {
        Self {
            enf_detected: false,
            confidence: 0.0,
            base_frequency: None,
            harmonics: Vec::new(),
            frequency_trace: Vec::new(),
            anomalies: Vec::new(),
            stability_score: 0.0,
            enf_snr_db: -100.0,
            estimated_region: None,
            evidence: Vec::new(),
        }
    }
}

/// Base ENF frequency (power grid fundamental)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EnfBaseFrequency {
    /// 50 Hz power grid (Europe, Asia, Africa, Australia)
    Hz50,
    /// 60 Hz power grid (North America, parts of Asia/South America)
    Hz60,
}

impl EnfBaseFrequency {
    pub fn frequency(&self) -> f32 {
        match self {
            EnfBaseFrequency::Hz50 => 50.0,
            EnfBaseFrequency::Hz60 => 60.0,
        }
    }
    
    pub fn harmonics(&self) -> Vec<f32> {
        let base = self.frequency();
        (1..=8).map(|n| base * n as f32).collect()
    }
}

impl std::fmt::Display for EnfBaseFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnfBaseFrequency::Hz50 => write!(f, "50 Hz"),
            EnfBaseFrequency::Hz60 => write!(f, "60 Hz"),
        }
    }
}

/// Geographic region based on power grid frequency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnfRegion {
    /// 50 Hz regions
    Europe,
    Asia,
    Africa,
    Australia,
    SouthAmericaEast,
    /// 60 Hz regions
    NorthAmerica,
    CentralAmerica,
    SouthAmericaWest,
    JapanEast,  // 50 Hz
    JapanWest,  // 60 Hz
    /// Unknown
    Unknown,
}

impl std::fmt::Display for EnfRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnfRegion::Europe => write!(f, "Europe (50 Hz)"),
            EnfRegion::Asia => write!(f, "Asia (50 Hz)"),
            EnfRegion::Africa => write!(f, "Africa (50 Hz)"),
            EnfRegion::Australia => write!(f, "Australia (50 Hz)"),
            EnfRegion::SouthAmericaEast => write!(f, "South America East (50 Hz)"),
            EnfRegion::NorthAmerica => write!(f, "North America (60 Hz)"),
            EnfRegion::CentralAmerica => write!(f, "Central America (60 Hz)"),
            EnfRegion::SouthAmericaWest => write!(f, "South America West (60 Hz)"),
            EnfRegion::JapanEast => write!(f, "Japan East (50 Hz)"),
            EnfRegion::JapanWest => write!(f, "Japan West (60 Hz)"),
            EnfRegion::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detected ENF harmonic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnfHarmonic {
    /// Harmonic number (1 = fundamental, 2 = first harmonic, etc.)
    pub harmonic_number: u8,
    /// Expected frequency in Hz
    pub expected_frequency: f32,
    /// Detected frequency in Hz
    pub detected_frequency: f32,
    /// Signal strength in dB
    pub strength_db: f32,
    /// SNR relative to surrounding noise
    pub snr_db: f32,
    /// Confidence in this harmonic detection
    pub confidence: f32,
}

/// Single ENF frequency measurement at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnfMeasurement {
    /// Time offset in seconds from start
    pub time_offset_secs: f32,
    /// Measured frequency in Hz
    pub frequency_hz: f32,
    /// Measurement confidence
    pub confidence: f32,
    /// Signal strength in dB
    pub strength_db: f32,
}

/// Detected anomaly in ENF trace (potential edit/splice)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnfAnomaly {
    /// Time offset where anomaly starts
    pub start_time_secs: f32,
    /// Duration of anomaly in seconds
    pub duration_secs: f32,
    /// Type of anomaly
    pub anomaly_type: EnfAnomalyType,
    /// Severity (0.0-1.0)
    pub severity: f32,
    /// Description
    pub description: String,
}

/// Types of ENF anomalies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnfAnomalyType {
    /// Sudden frequency jump (potential splice)
    FrequencyJump,
    /// ENF signal disappears briefly
    SignalDropout,
    /// Phase discontinuity
    PhaseDiscontinuity,
    /// Frequency drift rate change
    DriftRateChange,
    /// Harmonic ratio anomaly
    HarmonicAnomaly,
}

impl std::fmt::Display for EnfAnomalyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnfAnomalyType::FrequencyJump => write!(f, "Frequency Jump"),
            EnfAnomalyType::SignalDropout => write!(f, "Signal Dropout"),
            EnfAnomalyType::PhaseDiscontinuity => write!(f, "Phase Discontinuity"),
            EnfAnomalyType::DriftRateChange => write!(f, "Drift Rate Change"),
            EnfAnomalyType::HarmonicAnomaly => write!(f, "Harmonic Anomaly"),
        }
    }
}

/// ENF detector configuration
pub struct EnfDetector {
    /// FFT size for frequency analysis
    fft_size: usize,
    /// Hop size between analysis frames
    hop_size: usize,
    /// Minimum SNR to consider ENF detected (dB)
    min_snr_db: f32,
    /// Frequency resolution tolerance (Hz)
    frequency_tolerance: f32,
    /// Minimum number of frames for reliable detection
    min_frames: usize,
    /// Analysis window duration in seconds
    window_duration_secs: f32,
}

impl Default for EnfDetector {
    fn default() -> Self {
        Self {
            fft_size: 32768,  // Very high resolution for precise frequency detection
            hop_size: 8192,   // 75% overlap for smooth tracking
            min_snr_db: 3.0,  // Minimum 3 dB above noise floor
            frequency_tolerance: 0.5,  // ±0.5 Hz tolerance
            min_frames: 10,   // Need at least 10 frames for reliable detection
            window_duration_secs: 1.0,  // 1 second analysis windows
        }
    }
}

impl EnfDetector {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Configure for high sensitivity (noisy recordings)
    pub fn sensitive(mut self) -> Self {
        self.min_snr_db = 1.5;
        self.fft_size = 65536;  // Even higher resolution
        self
    }
    
    /// Configure for fast analysis (less accurate)
    pub fn fast(mut self) -> Self {
        self.fft_size = 16384;
        self.hop_size = 8192;
        self.min_frames = 5;
        self
    }
    
    /// Analyze audio for ENF presence
    pub fn analyze(&self, samples: &[f32], sample_rate: u32) -> EnfDetectionResult {
        let mut result = EnfDetectionResult::default();
        
        // Need sufficient samples for analysis
        if samples.len() < self.fft_size * 2 {
            result.evidence.push("Insufficient samples for ENF analysis".to_string());
            return result;
        }
        
        // Step 1: Detect which base frequency (50 or 60 Hz) is present
        let (base_freq, base_confidence) = self.detect_base_frequency(samples, sample_rate);
        
        if base_confidence < 0.3 {
            result.evidence.push("No clear ENF signal detected".to_string());
            return result;
        }
        
        result.base_frequency = Some(base_freq);
        
        // Step 2: Analyze all harmonics
        result.harmonics = self.analyze_harmonics(samples, sample_rate, base_freq);
        
        // Step 3: Track ENF frequency over time
        result.frequency_trace = self.track_frequency(samples, sample_rate, base_freq);
        
        // Step 4: Analyze trace for anomalies
        result.anomalies = self.detect_anomalies(&result.frequency_trace);
        
        // Step 5: Calculate overall metrics
        result.enf_snr_db = self.calculate_snr(&result.harmonics);
        result.stability_score = self.calculate_stability(&result.frequency_trace);
        
        // Step 6: Determine if ENF is reliably detected
        let harmonic_count = result.harmonics.iter()
            .filter(|h| h.confidence > 0.5)
            .count();
        
        result.enf_detected = harmonic_count >= 2 && result.enf_snr_db > self.min_snr_db;
        
        // Calculate overall confidence
        result.confidence = self.calculate_confidence(
            &result.harmonics,
            &result.frequency_trace,
            result.enf_snr_db,
        );
        
        // Determine estimated region
        result.estimated_region = Some(self.estimate_region(base_freq));
        
        // Build evidence
        self.build_evidence(&mut result);
        
        result
    }
    
    /// Detect whether 50 Hz or 60 Hz base frequency is present
    fn detect_base_frequency(&self, samples: &[f32], sample_rate: u32) -> (EnfBaseFrequency, f32) {
        let spectrum = self.compute_high_res_spectrum(samples, sample_rate);
        let bin_hz = sample_rate as f32 / self.fft_size as f32;
        
        // Calculate energy around 50 Hz and its harmonics
        let energy_50 = self.calculate_harmonic_energy(&spectrum, 50.0, bin_hz, 4);
        
        // Calculate energy around 60 Hz and its harmonics
        let energy_60 = self.calculate_harmonic_energy(&spectrum, 60.0, bin_hz, 4);
        
        // Calculate noise floor for comparison
        let noise_floor = self.estimate_noise_floor(&spectrum, bin_hz);
        
        let snr_50 = if noise_floor > 0.0 { energy_50 / noise_floor } else { 0.0 };
        let snr_60 = if noise_floor > 0.0 { energy_60 / noise_floor } else { 0.0 };
        
        if snr_50 > snr_60 && snr_50 > 2.0 {
            let confidence = (snr_50 / (snr_50 + snr_60 + 0.1)).min(0.95);
            (EnfBaseFrequency::Hz50, confidence)
        } else if snr_60 > snr_50 && snr_60 > 2.0 {
            let confidence = (snr_60 / (snr_50 + snr_60 + 0.1)).min(0.95);
            (EnfBaseFrequency::Hz60, confidence)
        } else {
            // Weak or no signal - default to 50 Hz with low confidence
            (EnfBaseFrequency::Hz50, 0.0)
        }
    }
    
    /// Analyze all harmonics of the detected base frequency
    fn analyze_harmonics(
        &self,
        samples: &[f32],
        sample_rate: u32,
        base_freq: EnfBaseFrequency,
    ) -> Vec<EnfHarmonic> {
        let spectrum = self.compute_high_res_spectrum(samples, sample_rate);
        let bin_hz = sample_rate as f32 / self.fft_size as f32;
        let nyquist = sample_rate as f32 / 2.0;
        
        let mut harmonics = Vec::new();
        
        for n in 1..=8 {
            let expected_freq = base_freq.frequency() * n as f32;
            
            // Skip if above Nyquist
            if expected_freq >= nyquist - 10.0 {
                break;
            }
            
            let (detected_freq, strength, snr) = self.measure_peak_near(
                &spectrum, expected_freq, bin_hz, self.frequency_tolerance * 2.0
            );
            
            let confidence = if snr > 10.0 {
                0.95
            } else if snr > 5.0 {
                0.7 + (snr - 5.0) * 0.05
            } else if snr > 2.0 {
                0.4 + (snr - 2.0) * 0.1
            } else if snr > 0.0 {
                snr * 0.2
            } else {
                0.0
            };
            
            harmonics.push(EnfHarmonic {
                harmonic_number: n,
                expected_frequency: expected_freq,
                detected_frequency: detected_freq,
                strength_db: if strength > 0.0 { 20.0 * strength.log10() } else { -120.0 },
                snr_db: if snr > 0.0 { 10.0 * snr.log10() } else { -60.0 },
                confidence,
            });
        }
        
        harmonics
    }
    
    /// Track ENF frequency over time
    fn track_frequency(
        &self,
        samples: &[f32],
        sample_rate: u32,
        base_freq: EnfBaseFrequency,
    ) -> Vec<EnfMeasurement> {
        let mut measurements = Vec::new();
        
        let frame_samples = (self.window_duration_secs * sample_rate as f32) as usize;
        let hop_samples = frame_samples / 2;  // 50% overlap
        
        let num_frames = (samples.len().saturating_sub(frame_samples)) / hop_samples + 1;
        
        if num_frames < 2 {
            return measurements;
        }
        
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        // Hann window
        let window: Vec<f32> = (0..self.fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / self.fft_size as f32).cos()))
            .collect();
        
        for frame in 0..num_frames {
            let start = frame * hop_samples;
            let end = (start + frame_samples).min(samples.len());
            
            if end - start < self.fft_size {
                continue;
            }
            
            // Compute spectrum for this frame
            let mut buffer: Vec<Complex<f32>> = samples[start..start + self.fft_size]
                .iter()
                .enumerate()
                .map(|(i, &s)| Complex::new(s * window[i], 0.0))
                .collect();
            
            fft.process(&mut buffer);
            
            let spectrum: Vec<f32> = buffer.iter()
                .take(self.fft_size / 2)
                .map(|c| (c.re * c.re + c.im * c.im).sqrt())
                .collect();
            
            let bin_hz = sample_rate as f32 / self.fft_size as f32;
            
            // Find peak near expected frequency (use 2nd harmonic for better precision)
            let target_freq = base_freq.frequency() * 2.0;  // 100 or 120 Hz
            let (freq, strength, snr) = self.measure_peak_near(
                &spectrum, target_freq, bin_hz, 1.0
            );
            
            // Convert back to fundamental
            let fundamental_freq = freq / 2.0;
            
            let time_offset = (start as f32 + frame_samples as f32 / 2.0) / sample_rate as f32;
            
            let confidence = if snr > 5.0 { 0.9 } else if snr > 2.0 { 0.6 } else { 0.3 };
            
            measurements.push(EnfMeasurement {
                time_offset_secs: time_offset,
                frequency_hz: fundamental_freq,
                confidence,
                strength_db: if strength > 0.0 { 20.0 * strength.log10() } else { -120.0 },
            });
        }
        
        measurements
    }
    
    /// Detect anomalies in the frequency trace
    fn detect_anomalies(&self, trace: &[EnfMeasurement]) -> Vec<EnfAnomaly> {
        let mut anomalies = Vec::new();
        
        if trace.len() < 3 {
            return anomalies;
        }
        
        // Calculate typical frequency variation
        let frequencies: Vec<f32> = trace.iter()
            .filter(|m| m.confidence > 0.5)
            .map(|m| m.frequency_hz)
            .collect();
        
        if frequencies.len() < 3 {
            return anomalies;
        }
        
        let mean_freq: f32 = frequencies.iter().sum::<f32>() / frequencies.len() as f32;
        let variance: f32 = frequencies.iter()
            .map(|f| (f - mean_freq).powi(2))
            .sum::<f32>() / frequencies.len() as f32;
        let std_dev = variance.sqrt();
        
        // Detect frequency jumps
        for i in 1..trace.len() {
            if trace[i].confidence < 0.5 || trace[i-1].confidence < 0.5 {
                continue;
            }
            
            let freq_diff = (trace[i].frequency_hz - trace[i-1].frequency_hz).abs();
            let time_diff = trace[i].time_offset_secs - trace[i-1].time_offset_secs;
            
            // Typical ENF variation is ~0.02 Hz over seconds
            // Jump of > 0.1 Hz is suspicious
            if freq_diff > 0.1 && freq_diff > std_dev * 3.0 {
                anomalies.push(EnfAnomaly {
                    start_time_secs: trace[i-1].time_offset_secs,
                    duration_secs: time_diff,
                    anomaly_type: EnfAnomalyType::FrequencyJump,
                    severity: (freq_diff / 0.5).min(1.0),
                    description: format!(
                        "Frequency jump of {:.3} Hz at {:.1}s (typical variation: {:.4} Hz)",
                        freq_diff, trace[i].time_offset_secs, std_dev
                    ),
                });
            }
        }
        
        // Detect signal dropouts
        let mut in_dropout = false;
        let mut dropout_start = 0.0f32;
        
        for (i, m) in trace.iter().enumerate() {
            if m.confidence < 0.3 && m.strength_db < -60.0 {
                if !in_dropout {
                    in_dropout = true;
                    dropout_start = m.time_offset_secs;
                }
            } else if in_dropout {
                let duration = m.time_offset_secs - dropout_start;
                if duration > 0.5 {  // Dropouts longer than 0.5s are significant
                    anomalies.push(EnfAnomaly {
                        start_time_secs: dropout_start,
                        duration_secs: duration,
                        anomaly_type: EnfAnomalyType::SignalDropout,
                        severity: (duration / 5.0).min(1.0),
                        description: format!(
                            "ENF signal dropout for {:.1}s starting at {:.1}s",
                            duration, dropout_start
                        ),
                    });
                }
                in_dropout = false;
            }
        }
        
        anomalies
    }
    
    /// Compute high-resolution magnitude spectrum
    fn compute_high_res_spectrum(&self, samples: &[f32], sample_rate: u32) -> Vec<f32> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        
        // Use multiple frames for averaging
        let num_frames = (samples.len() / self.hop_size).min(32);
        let mut spectrum_accum = vec![0.0f64; self.fft_size / 2];
        
        // Blackman-Harris window for high dynamic range
        let window: Vec<f32> = (0..self.fft_size)
            .map(|i| {
                let x = i as f32 / self.fft_size as f32;
                0.35875 - 0.48829 * (2.0 * PI * x).cos()
                    + 0.14128 * (4.0 * PI * x).cos()
                    - 0.01168 * (6.0 * PI * x).cos()
            })
            .collect();
        
        for frame in 0..num_frames {
            let start = frame * self.hop_size;
            if start + self.fft_size > samples.len() {
                break;
            }
            
            let mut buffer: Vec<Complex<f32>> = samples[start..start + self.fft_size]
                .iter()
                .enumerate()
                .map(|(i, &s)| Complex::new(s * window[i], 0.0))
                .collect();
            
            fft.process(&mut buffer);
            
            for (i, c) in buffer.iter().take(self.fft_size / 2).enumerate() {
                let mag = (c.re * c.re + c.im * c.im).sqrt();
                spectrum_accum[i] += mag as f64;
            }
        }
        
        spectrum_accum.iter()
            .map(|&s| (s / num_frames as f64) as f32)
            .collect()
    }
    
    /// Calculate total energy around a frequency and its harmonics
    fn calculate_harmonic_energy(
        &self,
        spectrum: &[f32],
        base_freq: f32,
        bin_hz: f32,
        num_harmonics: u32,
    ) -> f32 {
        let mut total_energy = 0.0f32;
        
        for n in 1..=num_harmonics {
            let target_freq = base_freq * n as f32;
            let center_bin = (target_freq / bin_hz) as usize;
            let search_bins = (2.0 / bin_hz) as usize;  // ±2 Hz search
            
            let start = center_bin.saturating_sub(search_bins);
            let end = (center_bin + search_bins).min(spectrum.len());
            
            if end > start {
                let peak = spectrum[start..end].iter().cloned().fold(0.0f32, f32::max);
                total_energy += peak * peak;
            }
        }
        
        total_energy.sqrt()
    }
    
    /// Estimate noise floor (avoiding ENF frequencies)
    fn estimate_noise_floor(&self, spectrum: &[f32], bin_hz: f32) -> f32 {
        // Sample noise from regions away from 50/60 Hz harmonics
        let noise_regions = [
            (70.0, 90.0),
            (130.0, 150.0),
            (190.0, 210.0),
            (250.0, 280.0),
        ];
        
        let mut noise_samples = Vec::new();
        
        for (low, high) in noise_regions {
            let start = (low / bin_hz) as usize;
            let end = (high / bin_hz) as usize;
            
            if end <= spectrum.len() {
                for &val in &spectrum[start..end] {
                    noise_samples.push(val);
                }
            }
        }
        
        if noise_samples.is_empty() {
            return 0.0;
        }
        
        // Use median as robust noise estimate
        noise_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        noise_samples[noise_samples.len() / 2]
    }
    
    /// Measure peak near expected frequency
    fn measure_peak_near(
        &self,
        spectrum: &[f32],
        target_freq: f32,
        bin_hz: f32,
        tolerance_hz: f32,
    ) -> (f32, f32, f32) {
        let center_bin = (target_freq / bin_hz) as usize;
        let search_bins = (tolerance_hz / bin_hz) as usize;
        
        let start = center_bin.saturating_sub(search_bins);
        let end = (center_bin + search_bins).min(spectrum.len());
        
        if end <= start {
            return (target_freq, 0.0, 0.0);
        }
        
        // Find peak
        let mut peak_bin = start;
        let mut peak_val = 0.0f32;
        
        for i in start..end {
            if spectrum[i] > peak_val {
                peak_val = spectrum[i];
                peak_bin = i;
            }
        }
        
        let detected_freq = (peak_bin as f32 + 0.5) * bin_hz;
        
        // Estimate local noise
        let noise_start = start.saturating_sub(search_bins * 2);
        let noise_end = (end + search_bins * 2).min(spectrum.len());
        
        let mut noise_vals: Vec<f32> = Vec::new();
        if noise_start < start {
            noise_vals.extend(&spectrum[noise_start..start]);
        }
        if noise_end > end {
            noise_vals.extend(&spectrum[end..noise_end]);
        }
        
        let noise_floor = if !noise_vals.is_empty() {
            noise_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
            noise_vals[noise_vals.len() / 2]
        } else {
            0.001
        };
        
        let snr = if noise_floor > 0.0 { peak_val / noise_floor } else { 0.0 };
        
        (detected_freq, peak_val, snr)
    }
    
    /// Calculate overall SNR from harmonics
    fn calculate_snr(&self, harmonics: &[EnfHarmonic]) -> f32 {
        if harmonics.is_empty() {
            return -100.0;
        }
        
        let total_snr: f32 = harmonics.iter()
            .filter(|h| h.snr_db > -30.0)
            .map(|h| 10.0f32.powf(h.snr_db / 10.0))
            .sum();
        
        if total_snr > 0.0 {
            10.0 * total_snr.log10()
        } else {
            -100.0
        }
    }
    
    /// Calculate stability score from frequency trace
    fn calculate_stability(&self, trace: &[EnfMeasurement]) -> f32 {
        if trace.len() < 3 {
            return 0.0;
        }
        
        let valid_measurements: Vec<&EnfMeasurement> = trace.iter()
            .filter(|m| m.confidence > 0.5)
            .collect();
        
        if valid_measurements.len() < 3 {
            return 0.0;
        }
        
        // Calculate coefficient of variation
        let mean: f32 = valid_measurements.iter()
            .map(|m| m.frequency_hz)
            .sum::<f32>() / valid_measurements.len() as f32;
        
        let variance: f32 = valid_measurements.iter()
            .map(|m| (m.frequency_hz - mean).powi(2))
            .sum::<f32>() / valid_measurements.len() as f32;
        
        let cv = variance.sqrt() / mean;
        
        // Typical ENF CV is < 0.001 (very stable)
        // Score decreases as CV increases
        (1.0 - cv * 500.0).clamp(0.0, 1.0)
    }
    
    /// Calculate overall detection confidence
    fn calculate_confidence(
        &self,
        harmonics: &[EnfHarmonic],
        trace: &[EnfMeasurement],
        snr_db: f32,
    ) -> f32 {
        let mut confidence = 0.0f32;
        
        // Harmonic presence (up to 0.4)
        let strong_harmonics = harmonics.iter()
            .filter(|h| h.confidence > 0.6)
            .count();
        confidence += (strong_harmonics as f32 / 4.0).min(1.0) * 0.4;
        
        // SNR contribution (up to 0.3)
        if snr_db > 10.0 {
            confidence += 0.3;
        } else if snr_db > 5.0 {
            confidence += 0.2;
        } else if snr_db > 2.0 {
            confidence += 0.1;
        }
        
        // Trace consistency (up to 0.3)
        let valid_trace = trace.iter().filter(|m| m.confidence > 0.5).count();
        if valid_trace > 20 {
            confidence += 0.3;
        } else if valid_trace > 10 {
            confidence += 0.2;
        } else if valid_trace > 5 {
            confidence += 0.1;
        }
        
        confidence.min(0.95)
    }
    
    /// Estimate recording region from base frequency
    fn estimate_region(&self, base_freq: EnfBaseFrequency) -> EnfRegion {
        match base_freq {
            EnfBaseFrequency::Hz50 => EnfRegion::Europe,  // Most common 50 Hz region
            EnfBaseFrequency::Hz60 => EnfRegion::NorthAmerica,  // Most common 60 Hz region
        }
    }
    
    /// Build evidence strings for the result
    fn build_evidence(&self, result: &mut EnfDetectionResult) {
        if let Some(ref base) = result.base_frequency {
            result.evidence.push(format!(
                "Detected {} power grid frequency",
                base
            ));
        }
        
        let strong_harmonics: Vec<_> = result.harmonics.iter()
            .filter(|h| h.confidence > 0.6)
            .collect();
        
        if !strong_harmonics.is_empty() {
            result.evidence.push(format!(
                "{} harmonics detected with high confidence",
                strong_harmonics.len()
            ));
        }
        
        if result.enf_snr_db > 5.0 {
            result.evidence.push(format!(
                "Strong ENF signal: {:.1} dB SNR",
                result.enf_snr_db
            ));
        }
        
        if result.stability_score > 0.8 {
            result.evidence.push("High frequency stability (consistent recording)".to_string());
        }
        
        if !result.anomalies.is_empty() {
            result.evidence.push(format!(
                "{} potential edit/splice points detected",
                result.anomalies.len()
            ));
        }
        
        if let Some(ref region) = result.estimated_region {
            result.evidence.push(format!(
                "Likely recording region: {}",
                region
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_base_frequency_harmonics() {
        let hz50 = EnfBaseFrequency::Hz50;
        let harmonics = hz50.harmonics();
        assert_eq!(harmonics[0], 50.0);
        assert_eq!(harmonics[1], 100.0);
        assert_eq!(harmonics[2], 150.0);
        
        let hz60 = EnfBaseFrequency::Hz60;
        let harmonics = hz60.harmonics();
        assert_eq!(harmonics[0], 60.0);
        assert_eq!(harmonics[1], 120.0);
    }
    
    #[test]
    fn test_detector_creation() {
        let detector = EnfDetector::new();
        assert_eq!(detector.fft_size, 32768);
        
        let sensitive = EnfDetector::new().sensitive();
        assert_eq!(sensitive.fft_size, 65536);
        assert!(sensitive.min_snr_db < detector.min_snr_db);
    }
    
    #[test]
    fn test_empty_samples() {
        let detector = EnfDetector::new();
        let result = detector.analyze(&[], 44100);
        assert!(!result.enf_detected);
    }
    
    #[test]
    fn test_synthetic_enf() {
        let detector = EnfDetector::new().fast();
        let sample_rate = 44100;
        let duration = 5.0;
        let num_samples = (sample_rate as f32 * duration) as usize;
        
        // Generate 50 Hz + harmonics
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                let fundamental = (2.0 * PI * 50.0 * t).sin() * 0.001;
                let harmonic2 = (2.0 * PI * 100.0 * t).sin() * 0.0005;
                let harmonic3 = (2.0 * PI * 150.0 * t).sin() * 0.00025;
                fundamental + harmonic2 + harmonic3
            })
            .collect();
        
        let result = detector.analyze(&samples, sample_rate);
        
        // Should detect 50 Hz base frequency
        if let Some(base) = result.base_frequency {
            assert_eq!(base, EnfBaseFrequency::Hz50);
        }
    }
}
