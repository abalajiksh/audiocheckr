//! Main audio detector implementation

use crate::core::analysis::{
    AnalysisConfig, AnalysisResult, DefectType, Detection, DetectionMethod,
    QualityMetrics, Severity, TemporalDistribution,
};
use crate::core::dsp::{SpectralAnalyzer, WindowFunction};
use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Main audio quality detector
pub struct AudioDetector {
    config: AnalysisConfig,
}

impl AudioDetector {
    pub fn new(config: AnalysisConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(AnalysisConfig::default())
    }

    /// Analyze an audio file
    pub fn analyze<P: AsRef<Path>>(&self, path: P) -> Result<AnalysisResult> {
        let path = path.as_ref();
        
        // Load audio file
        let (samples, sample_rate, channels, bit_depth) = self.load_audio(path)?;
        
        let duration = samples.len() as f64 / (sample_rate as f64 * channels as f64);
        
        // Calculate file hash
        let file_hash = self.calculate_hash(path)?;
        
        // Run detection pipeline
        let detections = self.run_detection_pipeline(&samples, sample_rate, bit_depth)?;
        
        // Calculate overall confidence
        let confidence = self.calculate_confidence(&detections);
        
        // Calculate quality metrics
        let quality_metrics = self.calculate_quality_metrics(&samples, sample_rate);
        
        Ok(AnalysisResult {
            file_path: path.to_path_buf(),
            file_hash,
            sample_rate,
            bit_depth,
            channels,
            duration,
            detections,
            confidence,
            quality_metrics: Some(quality_metrics),
            analysis_timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Load audio samples from file
    fn load_audio<P: AsRef<Path>>(&self, path: P) -> Result<(Vec<f32>, u32, u16, u16)> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open file: {}", path.display()))?;
        
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        
        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();
        
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .context("Failed to probe audio format")?;
        
        let mut format = probed.format;
        
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .context("No audio track found")?;
        
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
        let bit_depth = track.codec_params.bits_per_sample.unwrap_or(16) as u16;
        
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &decoder_opts)
            .context("Failed to create decoder")?;
        
        let track_id = track.id;
        let mut samples = Vec::new();
        
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(e)) 
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            };
            
            if packet.track_id() != track_id {
                continue;
            }
            
            let decoded = decoder.decode(&packet)?;
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;
            
            let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
            sample_buf.copy_interleaved_ref(decoded);
            
            samples.extend(sample_buf.samples());
        }
        
        Ok((samples, sample_rate, channels, bit_depth))
    }

    /// Calculate MD5 hash of file
    fn calculate_hash<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let data = std::fs::read(path.as_ref())?;
        let hash = md5::compute(&data);
        Ok(format!("{:x}", hash))
    }

    /// Run the full detection pipeline
    fn run_detection_pipeline(
        &self,
        samples: &[f32],
        sample_rate: u32,
        bit_depth: u16,
    ) -> Result<Vec<Detection>> {
        let mut detections = Vec::new();
        
        // Convert to f64 for analysis
        let samples_f64: Vec<f64> = samples.iter().map(|&s| s as f64).collect();
        
        // 1. Dithering Detection
        if let Some(detection) = self.detect_dithering(samples, bit_depth)? {
             detections.push(detection);
             // If dithered, upsampling detection might need to be less strict or skipped?
             // We'll let it run but inform logic
        }

        // 2. Resampling Detection
        let mut found_resampling = false;
        if let Some(detection) = self.detect_resampling(samples, sample_rate)? {
             detections.push(detection);
             found_resampling = true;
        }

        // 3. Spectral cutoff detection (transcode detection)
        // If we found specific resampling evidence, we might skip generic transcode detection 
        // to avoid duplicate/conflicting findings, but transcode usually implies LOSSY (MP3 cutoffs).
        if let Some(detection) = self.detect_spectral_cutoff(&samples_f64, sample_rate)? {
            // Filter out if we already found resampling that explains the cutoff
            if !found_resampling {
                detections.push(detection);
            }
        }
        
        // 4. Upsampling detection (generic)
        // Avoid if we already identified it as specific Resampling
        if !found_resampling {
            if let Some(detection) = self.detect_upsampling(&samples_f64, sample_rate)? {
                detections.push(detection);
            }
        }
        
        // 5. Bit depth analysis
        if let Some(detection) = self.detect_bit_depth_inflation(samples, bit_depth)? {
            detections.push(detection);
        }
        
        // 6. MQA detection (if enabled)
        if self.config.enable_mqa {
            if let Some(detection) = self.detect_mqa(samples, sample_rate, bit_depth)? {
                detections.push(detection);
            }
        }
        
        // 7. Clipping detection (if enabled)
        if self.config.enable_clipping {
            if let Some(detection) = self.detect_clipping(samples, sample_rate)? {
                detections.push(detection);
            }
        }
        
        // Filter by minimum confidence
        detections.retain(|d| d.confidence >= self.config.min_confidence);
        
        Ok(detections)
    }

    fn detect_dithering(&self, samples: &[f32], bit_depth: u16) -> Result<Option<Detection>> {
        use crate::core::analysis::dithering_detection::{DitheringDetector, DitherType};
        let detector = DitheringDetector::new();
        let result = detector.detect(samples, bit_depth);
        
        if result.is_dithered {
            let type_str = match result.dither_type {
                DitherType::TPDF => "TPDF",
                DitherType::RPDF => "RPDF",
                DitherType::Shaped => "Noise Shaped",
                DitherType::Gaussian => "Gaussian",
                _ => "Unknown",
            };
            
            return Ok(Some(Detection {
                defect_type: DefectType::DitheringDetected {
                    dither_type: type_str.to_string(),
                    bit_depth: result.bit_depth,
                    noise_shaping: result.noise_shaping,
                },
                confidence: result.confidence,
                severity: Severity::Info,
                method: DetectionMethod::NoiseFloorAnalysis,
                evidence: Some(format!("{} dither detected at {} bits", type_str, result.bit_depth)),
                temporal: None,
            }));
        }
        Ok(None)
    }

    fn detect_resampling(&self, samples: &[f32], sample_rate: u32) -> Result<Option<Detection>> {
        use crate::core::analysis::resampling_detection::ResamplingDetector;
        let detector = ResamplingDetector::new();
        let result = detector.detect(samples, sample_rate);
        
        if result.is_resampled {
            let quality = result.quality;
            let target = result.target_rate;
            let orig = result.original_rate.unwrap_or(0);
            
            return Ok(Some(Detection {
                defect_type: DefectType::ResamplingDetected {
                    original_rate: orig,
                    target_rate: target,
                    quality: quality.clone(),
                },
                confidence: result.confidence,
                severity: Severity::Medium, // Resampling is usually a modification
                method: DetectionMethod::SpectralShape,
                evidence: Some(format!("Resampling signature detected: {}", quality)),
                temporal: None,
            }));
        }
        Ok(None)
    }

    /// Detect lossy transcode via spectral cutoff
    fn detect_spectral_cutoff(
        &self,
        samples: &[f64],
        sample_rate: u32,
    ) -> Result<Option<Detection>> {
        let mut analyzer = SpectralAnalyzer::new(
            self.config.fft_size,
            self.config.hop_size,
            WindowFunction::BlackmanHarris,
        );
        
        let nyquist = sample_rate as f64 / 2.0;
        
        // Detect cutoff frequency
        let cutoff = analyzer.detect_cutoff(samples, sample_rate, 10.0);
        
        if let Some(cutoff_hz) = cutoff {
            // Check if cutoff is suspiciously low
            let cutoff_ratio = cutoff_hz / nyquist;
            
            if cutoff_ratio < 0.95 {
                // Estimate original format based on cutoff
                let (codec, bitrate) = self.estimate_codec(cutoff_hz);
                
                let confidence = (0.95 - cutoff_ratio) / 0.3; // Scale confidence
                let confidence = confidence.clamp(0.0, 1.0);
                
                let severity = if cutoff_ratio < 0.5 {
                    Severity::Critical
                } else if cutoff_ratio < 0.7 {
                    Severity::High
                } else if cutoff_ratio < 0.85 {
                    Severity::Medium
                } else {
                    Severity::Low
                };
                
                return Ok(Some(Detection {
                    defect_type: DefectType::LossyTranscode {
                        codec,
                        estimated_bitrate: Some(bitrate),
                        cutoff_hz: cutoff_hz as u32,
                    },
                    confidence,
                    severity,
                    method: DetectionMethod::SpectralCutoff,
                    evidence: Some(format!(
                        "Spectral cutoff at {} Hz ({:.1}% of Nyquist)",
                        cutoff_hz as u32,
                        cutoff_ratio * 100.0
                    )),
                    temporal: None,
                }));
            }
        }
        
        Ok(None)
    }

    /// Estimate original codec based on cutoff frequency
    fn estimate_codec(&self, cutoff_hz: f64) -> (String, u32) {
        // Common cutoff frequencies for various formats
        if cutoff_hz < 11025.0 {
            ("MP3".to_string(), 64)
        } else if cutoff_hz < 14000.0 {
            ("MP3".to_string(), 128)
        } else if cutoff_hz < 16000.0 {
            ("MP3".to_string(), 192)
        } else if cutoff_hz < 18000.0 {
            ("MP3/AAC".to_string(), 256)
        } else if cutoff_hz < 20000.0 {
            ("AAC/OGG".to_string(), 320)
        } else {
            ("Unknown".to_string(), 0)
        }
    }

    /// Detect upsampling
    fn detect_upsampling(
        &self,
        samples: &[f64],
        sample_rate: u32,
    ) -> Result<Option<Detection>> {
        // Look for null energy at expected Nyquist frequencies
        let common_rates = [44100, 48000, 88200, 96000];
        
        for &original_rate in &common_rates {
            if original_rate >= sample_rate {
                continue;
            }
            
            let original_nyquist = original_rate as f64 / 2.0;
            
            // Check for null energy above original Nyquist
            let mut analyzer = SpectralAnalyzer::new(
                self.config.fft_size,
                self.config.hop_size,
                WindowFunction::BlackmanHarris,
            );
            
            let spectrum = analyzer.compute_power_spectrum_db(samples);
            let freq_resolution = sample_rate as f64 / self.config.fft_size as f64;
            
            let start_bin = (original_nyquist / freq_resolution) as usize;
            let end_bin = spectrum.len();
            
            if start_bin >= end_bin {
                continue;
            }
            
            // Check energy above original Nyquist
            let high_freq_energy: f64 = spectrum[start_bin..end_bin]
                .iter()
                .map(|&x| 10.0_f64.powf(x / 10.0))
                .sum::<f64>() / (end_bin - start_bin) as f64;
            
            let low_freq_energy: f64 = spectrum[..start_bin]
                .iter()
                .map(|&x| 10.0_f64.powf(x / 10.0))
                .sum::<f64>() / start_bin.max(1) as f64;
            
            let ratio = high_freq_energy / low_freq_energy.max(1e-10);
            
            if ratio < 0.01 {
                // Very little energy above original Nyquist - likely upsampled
                let confidence = (1.0 - ratio * 10.0).clamp(0.0, 1.0);
                
                return Ok(Some(Detection {
                    defect_type: DefectType::Upsampled {
                        original_rate,
                        current_rate: sample_rate,
                    },
                    confidence,
                    severity: Severity::High,
                    method: DetectionMethod::SpectralShape,
                    evidence: Some(format!(
                        "Null energy above {} Hz suggests upsampling from {} Hz",
                        original_nyquist as u32, original_rate
                    )),
                    temporal: None,
                }));
            }
        }
        
        Ok(None)
    }

    /// Detect bit depth inflation
    fn detect_bit_depth_inflation(
        &self,
        samples: &[f32],
        claimed_bits: u16,
    ) -> Result<Option<Detection>> {
        if samples.is_empty() {
            return Ok(None);
        }
        
        // Analyze LSB patterns to determine actual bit depth
        let mut bit_usage = vec![0u64; 32];
        
        for &sample in samples {
            // Convert to integer representation
            let int_val = (sample * (1 << (claimed_bits - 1)) as f32) as i32;
            
            for bit in 0..32 {
                if (int_val >> bit) & 1 != 0 {
                    bit_usage[bit] += 1;
                }
            }
        }
        
        // Find actual bit depth by looking at LSB usage
        let total = samples.len() as f64;
        let mut actual_bits = claimed_bits;
        
        for bit in 0..claimed_bits as usize {
            let usage_ratio = bit_usage[bit] as f64 / total;
            // If LSBs have very low entropy, they're likely padding
            if usage_ratio < 0.01 || usage_ratio > 0.99 {
                actual_bits = (claimed_bits as usize - bit - 1).max(8) as u16;
            } else {
                break;
            }
        }
        
        if actual_bits < claimed_bits - 1 {
            let confidence = (claimed_bits - actual_bits) as f64 / 8.0;
            let confidence = confidence.clamp(0.0, 1.0);
            
            return Ok(Some(Detection {
                defect_type: DefectType::BitDepthInflated {
                    actual_bits,
                    claimed_bits,
                },
                confidence,
                severity: if claimed_bits - actual_bits > 8 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                method: DetectionMethod::BitDepthAnalysis,
                evidence: Some(format!(
                    "LSB analysis suggests {} bit audio padded to {} bits",
                    actual_bits, claimed_bits
                )),
                temporal: None,
            }));
        }
        
        Ok(None)
    }

    /// Detect MQA encoding - FIXED: Now uses proper MqaDetector with bit_depth parameter
    fn detect_mqa(&self, samples: &[f32], sample_rate: u32, bit_depth: u16) -> Result<Option<Detection>> {
        use crate::core::analysis::mqa_detection::MqaDetector;
        
        // Use the comprehensive MQA detector with proper 24-bit handling
        let detector = MqaDetector::default();
        let result = detector.detect(samples, sample_rate, bit_depth as u32);
        
        if result.is_mqa_encoded {
            let mqa_type = match result.mqa_type {
                Some(ref t) => format!("{:?}", t),
                None => "Unknown".to_string(),
            };
            
            return Ok(Some(Detection {
                defect_type: DefectType::MqaEncoded {
                    original_rate: result.original_sample_rate,
                    mqa_type,
                    lsb_entropy: result.lsb_entropy as f64,
                },
                confidence: result.confidence as f64,
                severity: Severity::Info, // MQA is informational, not a defect
                method: DetectionMethod::MqaSignature,
                evidence: Some(result.evidence.join("; ")),
                temporal: None,
            }));
        }
        
        Ok(None)
    }

    /// Detect clipping
    fn detect_clipping(&self, samples: &[f32], sample_rate: u32) -> Result<Option<Detection>> {
        use crate::core::analysis::clipping_detection::ClippingDetector;
        
        let detector = ClippingDetector::new();
        Ok(detector.analyze(samples, sample_rate))
    }

    /// Calculate overall confidence score
    fn calculate_confidence(&self, detections: &[Detection]) -> f64 {
        if detections.is_empty() {
            return 1.0; // High confidence it's genuine
        }
        
        // Calculate weighted average of detection confidences
        let total_weight: f64 = detections
            .iter()
            .map(|d| match d.severity {
                Severity::Critical => 1.0,
                Severity::High => 0.8,
                Severity::Medium => 0.5,
                Severity::Low => 0.3,
                Severity::Info => 0.1,
            })
            .sum();
        
        let weighted_confidence: f64 = detections
            .iter()
            .map(|d| {
                let weight = match d.severity {
                    Severity::Critical => 1.0,
                    Severity::High => 0.8,
                    Severity::Medium => 0.5,
                    Severity::Low => 0.3,
                    Severity::Info => 0.1,
                };
                d.confidence * weight
            })
            .sum();
        
        if total_weight > 0.0 {
            1.0 - (weighted_confidence / total_weight)
        } else {
            1.0
        }
    }

    /// Calculate quality metrics
    fn calculate_quality_metrics(&self, samples: &[f32], _sample_rate: u32) -> QualityMetrics {
        if samples.is_empty() {
            return QualityMetrics::default();
        }
        
        // Calculate basic metrics
        let max_sample = samples.iter().map(|&s| s.abs()).fold(0.0_f32, f32::max);
        let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        
        let true_peak = if max_sample > 0.0 {
            20.0 * (max_sample as f64).log10()
        } else {
            -f64::INFINITY
        };
        
        let rms_db = if rms > 0.0 {
            20.0 * (rms as f64).log10()
        } else {
            -f64::INFINITY
        };
        
        let crest_factor = if rms > 0.0 {
            20.0 * ((max_sample / rms) as f64).log10()
        } else {
            0.0
        };
        
        // Estimate dynamic range
        let dynamic_range = true_peak - rms_db + 3.0; // Approximation
        
        // Estimate noise floor (simplified - look at quiet sections)
        let mut sorted_samples: Vec<f32> = samples.iter().map(|&s| s.abs()).collect();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let noise_floor_sample = sorted_samples[sorted_samples.len() / 100]; // 1st percentile
        let noise_floor = if noise_floor_sample > 0.0 {
            20.0 * (noise_floor_sample as f64).log10()
        } else {
            -96.0
        };
        
        QualityMetrics {
            dynamic_range,
            noise_floor,
            spectral_centroid: 0.0, // Would require FFT analysis
            crest_factor,
            true_peak,
            lufs_integrated: rms_db, // Simplified - real LUFS requires more analysis
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_creation() {
        let detector = AudioDetector::with_default_config();
        assert_eq!(detector.config.fft_size, 8192);
    }

    #[test]
    fn test_codec_estimation() {
        let detector = AudioDetector::with_default_config();
        
        let (codec, bitrate) = detector.estimate_codec(11000.0);
        assert_eq!(codec, "MP3");
        assert_eq!(bitrate, 64);
        
        let (codec, bitrate) = detector.estimate_codec(15000.0);
        assert_eq!(codec, "MP3");
        assert_eq!(bitrate, 192);
    }
}
