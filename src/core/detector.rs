//! Main audio detector implementation (v2)
//!
//! Implements P0–P6 from the diagnosis document:
//! - P0: codec-specific defect types
//! - P1: SpectralAnalyzer-based lossy pipeline + MFCC integration
//! - P2: true spectral-shelf upsampling detector
//! - P3: multi-heuristic bit-depth inflation detector
//! - P4: simple pre-echo detector
//! - P5: tiered confidence thresholds by bitrate / defect type
//! - P6: heuristic multi-generation lossy detection

use crate::core::analysis::dynamic_range::{DynamicRangeAnalyzer, DynamicRangeResult};
use crate::core::analysis::{
    AnalysisConfig, AnalysisResult, DefectType, Detection, DetectionMethod, QualityMetrics,
    Severity,
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

pub struct AudioDetector {
    pub(crate) config: AnalysisConfig,
}

impl AudioDetector {
    pub fn new(config: AnalysisConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(AnalysisConfig::default())
    }

    /// Analyze an audio file end‑to‑end.
    pub fn analyze<P: AsRef<Path>>(&self, path: P) -> Result<AnalysisResult> {
        let path = path.as_ref();

        let (samples, sample_rate, channels, bit_depth) = self.load_audio(path)?;
        let duration = samples.len() as f64 / (sample_rate as f64 * channels as f64);
        let file_hash = self.calculate_hash(path)?;

        let detections = self.run_detection_pipeline(&samples, sample_rate, bit_depth, channels)?;

        let confidence = self.calculate_confidence(&detections);
        let quality_metrics = self.calculate_quality_metrics(&samples, sample_rate);
        let dynamic_range = self.run_dynamic_range_analysis(&samples, sample_rate, channels);

        // Downmix to mono for MFCC in the stored AnalysisResult only; the
        // decision logic already ran MFCC internally for detection.
        let mfcc = if self.config.enable_mfcc {
            let mono: Vec<f64> = samples
                .chunks(channels as usize)
                .map(|frame| frame.iter().map(|&s| s as f64).sum::<f64>() / channels as f64)
                .collect();
            Some(self.run_mfcc_analysis(&mono, sample_rate))
        } else {
            None
        };

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
            dynamic_range,
            mfcc,
        })
    }

    // ──────────────────────── helpers: loading / basic analysis ────────────────────────

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

        let sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);
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
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break
                }
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

    fn calculate_hash<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let data = std::fs::read(path.as_ref())?;
        let hash = md5::compute(&data);
        Ok(format!("{:x}", hash))
    }

    fn run_dynamic_range_analysis(
        &self,
        samples: &[f32],
        sample_rate: u32,
        channels: u16,
    ) -> Option<DynamicRangeResult> {
        if samples.is_empty() {
            return None;
        }

        let n_channels = channels as usize;
        let samples_per_channel = samples.len() / n_channels;
        let mut deinterleaved = vec![Vec::with_capacity(samples_per_channel); n_channels];

        for (i, &s) in samples.iter().enumerate() {
            deinterleaved[i % n_channels].push(s as f64);
        }

        let channel_refs: Vec<&[f64]> = deinterleaved.iter().map(|c| c.as_slice()).collect();
        let analyzer = DynamicRangeAnalyzer::new(sample_rate);
        Some(analyzer.analyze(&channel_refs))
    }

    fn run_mfcc_analysis(
        &self,
        mono_samples: &[f64],
        sample_rate: u32,
    ) -> crate::core::analysis::mfcc::MfccResult {
        use crate::core::analysis::mfcc::{MfccAnalyzer, MfccConfig};
        let cfg = MfccConfig::for_codec_detection();
        let analyzer = MfccAnalyzer::new(sample_rate, cfg);
        analyzer.analyze(mono_samples)
    }

    // ───────────────────────────── core detection pipeline ─────────────────────────────

    fn run_detection_pipeline(
        &self,
        samples: &[f32],
        sample_rate: u32,
        bit_depth: u16,
        channels: u16,
    ) -> Result<Vec<Detection>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let mut detections = Vec::new();

        // Convenience views
        let samples_f64: Vec<f64> = samples.iter().copied().map(|x| x as f64).collect();
        let mono_f64: Vec<f64> = samples
            .chunks(channels as usize)
            .map(|frame| frame.iter().map(|&s| s as f64).sum::<f64>() / channels as f64)
            .collect();

        // State flags for gating
        let mut has_resampling = false;
        let mut has_transcode = false;
        let mut has_bit_inflation = false;

        // 1) Dithering (informational)
        if let Some(det) = self.detect_dithering(samples, bit_depth)? {
            detections.push(det);
        }

        // 2) Resampling artifacts
        if let Some(det) = self.detect_resampling(samples, sample_rate)? {
            has_resampling = true;
            detections.push(det);
        }

        // 3) Spectral cutoff (codec‑specific, P0/P1) – skipped if resampled
        let spectral_det = if !has_resampling {
            self.detect_spectral_cutoff(&samples_f64, sample_rate)?
        } else {
            None
        };

        if let Some(det) = spectral_det.clone() {
            has_transcode = true;
            detections.push(det);
        }

        // 4) Bit‑depth inflation (multi‑heuristic, P3)
        if let Some(det) = self.detect_bit_depth_inflation_multi(samples, bit_depth)? {
            has_bit_inflation = true;
            detections.push(det);
        }

        // 5) Upsampling shelf (P2) – gated by resampling / transcode / bit inflation
        if !has_resampling && !has_transcode && !has_bit_inflation {
            if let Some(det) = self.detect_upsampling_shelf(&samples_f64, sample_rate)? {
                detections.push(det);
            }
        }

        // 6) MQA (OK in current code)
        if self.config.enable_mqa {
            if let Some(det) = self.detect_mqa(samples, sample_rate, bit_depth)? {
                detections.push(det);
            }
        }

        // 7) Clipping (OK in current code)
        if self.config.enable_clipping {
            if let Some(det) = self.detect_clipping(samples, sample_rate)? {
                detections.push(det);
            }
        }

        // 8) MFCC + SFM lossy detection (P1) – only if spectral cutoff missed
        let mut mfcc_det: Option<Detection> = None;
        let mut sfm_det: Option<Detection> = None;

        if self.config.enable_mfcc && !has_transcode {
            let mfcc_res = self.run_mfcc_analysis(&mono_f64, sample_rate);
            mfcc_det = self.detect_lossy_via_mfcc(&mfcc_res);
            sfm_det = self.detect_lossy_via_sfm(&mono_f64, sample_rate);
        }

        match (mfcc_det.take(), sfm_det.take()) {
            (Some(m), Some(s)) => {
                // Pick whichever is more confident
                if m.confidence >= s.confidence {
                    detections.push(m);
                } else {
                    detections.push(s);
                }
            }
            (Some(m), None) => detections.push(m),
            (None, Some(s)) => detections.push(s),
            (None, None) => {}
        }

        // 9) Pre‑echo detector (P4) – lightweight, MFCC‑based
        if let Some(det) = self.detect_pre_echo(&mono_f64, sample_rate) {
            detections.push(det);
        }

        // 10) Multi‑generation heuristic (P6)
        if let Some(det) = self.detect_multigeneration_lossy(&detections) {
            detections.push(det);
        }

        // Final confidence gating with per‑defect tiers (P5)
        let min_global = self.config.min_confidence;
        detections.retain(|d| self.passes_confidence_gate(d, min_global));

        Ok(detections)
    }

    // ───────────────────────────── individual detectors ─────────────────────────────

    fn detect_dithering(&self, samples: &[f32], bit_depth: u16) -> Result<Option<Detection>> {
        use crate::core::analysis::dithering_detection::{DitherType, DitheringDetector};

        let det = DitheringDetector::new();
        let res = det.detect(samples, bit_depth);

        if !res.is_dithered {
            return Ok(None);
        }

        let type_str = match res.dither_type {
            DitherType::TPDF => "TPDF",
            DitherType::RPDF => "RPDF",
            DitherType::Shaped => "Noise Shaped",
            DitherType::Gaussian => "Gaussian",
            _ => "Unknown",
        };

        Ok(Some(Detection {
            defect_type: DefectType::DitheringDetected {
                dither_type: type_str.to_string(),
                bit_depth: res.bit_depth,
                noise_shaping: res.noise_shaping,
            },
            confidence: res.confidence,
            severity: Severity::Info,
            method: DetectionMethod::NoiseFloorAnalysis,
            evidence: Some(format!(
                "{} dither detected at {} bits",
                type_str, res.bit_depth
            )),
            temporal: None,
        }))
    }

    fn detect_resampling(&self, samples: &[f32], sample_rate: u32) -> Result<Option<Detection>> {
        use crate::core::analysis::resampling_detection::ResamplingDetector;

        let det = ResamplingDetector::new();
        let res = det.detect(samples, sample_rate);

        if !res.is_resampled {
            return Ok(None);
        }

        Ok(Some(Detection {
            defect_type: DefectType::ResamplingDetected {
                original_rate: res.original_rate.unwrap_or(0),
                target_rate: res.target_rate,
                quality: res.quality.clone(),
            },
            confidence: res.confidence,
            severity: Severity::Medium,
            method: DetectionMethod::SpectralShape,
            evidence: Some(format!("Resampling signature detected: {}", res.quality)),
            temporal: None,
        }))
    }

    /// Spectral cutoff based lossy detector (P0/P1).
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
        let cutoff = analyzer.detect_cutoff(samples, sample_rate, 10.0);

        let cutoff_hz = match cutoff {
            Some(v) => v,
            None => return Ok(None),
        };

        let cutoff_ratio = cutoff_hz / nyquist;
        if cutoff_ratio >= 0.95 {
            return Ok(None);
        }

        let (codec_id, est_bitrate) = self.estimate_codec(cutoff_hz);
        let raw = (0.95 - cutoff_ratio) / 0.3;
        let mut confidence = raw.clamp(0.0, 1.0);

        // Slight boost when cutoff is very low
        if cutoff_ratio < 0.6 {
            confidence = (confidence + 0.15).min(1.0);
        }

        let severity = if cutoff_ratio < 0.5 {
            Severity::Critical
        } else if cutoff_ratio < 0.7 {
            Severity::High
        } else if cutoff_ratio < 0.85 {
            Severity::Medium
        } else {
            Severity::Low
        };

        let defect_type = match codec_id.as_str() {
            "mp3" => DefectType::Mp3Transcode {
                estimated_bitrate: Some(est_bitrate),
                cutoff_hz: cutoff_hz as u32,
            },
            "aac" => DefectType::AacTranscode {
                estimated_bitrate: Some(est_bitrate),
                cutoff_hz: cutoff_hz as u32,
            },
            "opus" => DefectType::OpusTranscode {
                estimated_bitrate: Some(est_bitrate),
                cutoff_hz: cutoff_hz as u32,
            },
            "ogg" | "vorbis" => DefectType::OggVorbisTranscode {
                estimated_bitrate: Some(est_bitrate),
                cutoff_hz: cutoff_hz as u32,
            },
            _ => DefectType::LossyTranscode {
                codec: codec_id,
                estimated_bitrate: Some(est_bitrate),
                cutoff_hz: cutoff_hz as u32,
            },
        };

        Ok(Some(Detection {
            defect_type,
            confidence,
            severity,
            method: DetectionMethod::SpectralCutoff,
            evidence: Some(format!(
                "Spectral cutoff at {} Hz ({:.1}% of Nyquist)",
                cutoff_hz as u32,
                cutoff_ratio * 100.0
            )),
            temporal: None,
        }))
    }

    /// Map cutoff frequency → approximate codec/bitrate bucket.
    fn estimate_codec(&self, cutoff_hz: f64) -> (String, u32) {
        if cutoff_hz < 11_000.0 {
            ("mp3".to_string(), 64)
        } else if cutoff_hz < 14_000.0 {
            ("mp3".to_string(), 128)
        } else if cutoff_hz < 16_000.0 {
            ("mp3".to_string(), 192)
        } else if cutoff_hz < 18_000.0 {
            ("mp3".to_string(), 256)
        } else if cutoff_hz < 20_000.0 {
            ("aac".to_string(), 320)
        } else {
            ("unknown".to_string(), 0)
        }
    }

    /// Multi‑heuristic bit‑depth inflation detector (P3).
    /// This replaces the very simple bit‑usage counter with:
    /// - effective bit usage estimate
    /// - LSB entropy
    /// - quantisation noise level
    fn detect_bit_depth_inflation_multi(
        &self,
        samples: &[f32],
        claimed_bits: u16,
    ) -> Result<Option<Detection>> {
        if samples.is_empty() || claimed_bits < 16 {
            return Ok(None);
        }

        // 1) Quantise to integer domain
        let scale = (1_i32 << (claimed_bits - 1)) as f32;
        let mut ints: Vec<i32> = Vec::with_capacity(samples.len());
        for &s in samples {
            let v = (s * scale).round().clamp(
                -(1_i32 << (claimed_bits - 1)) as f32,
                ((1_i32 << (claimed_bits - 1)) - 1) as f32,
            ) as i32;
            ints.push(v);
        }

        // 2) Effective bit usage: look for never‑used MSBs and always‑zero LSBs
        let mut bit_counts = vec![0u64; claimed_bits as usize];
        for &v in &ints {
            for b in 0..claimed_bits {
                if (v >> b) & 1 != 0 {
                    bit_counts[b as usize] += 1;
                }
            }
        }
        let total = ints.len() as f64;

        let mut highest_used_bit = 0usize;
        for b in (0..claimed_bits as usize).rev() {
            let usage = bit_counts[b] as f64 / total;
            if usage > 0.001 {
                highest_used_bit = b;
                break;
            }
        }
        let effective_bits = (highest_used_bit + 1).max(8) as u16;

        // 3) LSB entropy over lower N bits
        let n_lsb = 4.min(claimed_bits as usize);
        let mut lsb_hist = vec![0u64; 1 << n_lsb];
        for &v in &ints {
            let bucket = (v & ((1 << n_lsb) - 1)) as usize;
            lsb_hist[bucket] += 1;
        }
        let entropy = {
            let mut e = 0.0;
            for &c in &lsb_hist {
                if c == 0 {
                    continue;
                }
                let p = c as f64 / total;
                e -= p * p.ln();
            }
            e / ((1 << n_lsb) as f64).ln() // normalised 0..1
        };

        // 4) Quantisation noise estimate: energy of residual vs full scale
        let mut residual_energy = 0.0_f64;
        let mut signal_energy = 0.0_f64;
        for &v in &ints {
            signal_energy += (v as f64).powi(2);
            let truncated = (v >> (claimed_bits - effective_bits) as i32)
                << (claimed_bits - effective_bits) as i32;
            let r = v - truncated;
            residual_energy += (r as f64).powi(2);
        }
        let q_noise_db = if residual_energy > 0.0 && signal_energy > 0.0 {
            10.0 * (residual_energy / signal_energy).log10()
        } else {
            -120.0
        };

        // 5) Voting
        let mut votes = 0usize;

        // Vote 1: significant gap between claimed and effective bits
        if effective_bits + 2 <= claimed_bits {
            votes += 1;
        }

        // Vote 2: LSB entropy very high (dithered/inflated) or very low (zero‑padded)
        if entropy > 0.9 || entropy < 0.2 {
            votes += 1;
        }

        // Vote 3: residual noise too small for genuine 24‑bit
        if claimed_bits >= 24 && q_noise_db < -90.0 {
            votes += 1;
        }

        if votes < 2 {
            return Ok(None);
        }

        let bit_gap = (claimed_bits - effective_bits).max(1) as f64;
        let mut confidence = (bit_gap / 8.0).clamp(0.3, 1.0);

        // Slightly increase confidence with strong entropy or residual evidence
        if entropy < 0.1 || entropy > 0.95 {
            confidence = (confidence + 0.1).min(1.0);
        }
        if q_noise_db < -96.0 {
            confidence = (confidence + 0.1).min(1.0);
        }

        Ok(Some(Detection {
            defect_type: DefectType::BitDepthInflated {
                actual_bits: effective_bits,
                claimed_bits,
            },
            confidence,
            severity: if bit_gap >= 8.0 {
                Severity::High
            } else {
                Severity::Medium
            },
            method: DetectionMethod::BitDepthAnalysis,
            evidence: Some(format!(
                "effective_bits≈{}, claimed_bits={}, LSB_entropy={:.2}, q_noise≈{:.1} dB",
                effective_bits, claimed_bits, entropy, q_noise_db
            )),
            temporal: None,
        }))
    }

    /// Spectral‑shelf upsampling detector (P2).
    fn detect_upsampling_shelf(
        &self,
        samples: &[f64],
        sample_rate: u32,
    ) -> Result<Option<Detection>> {
        let common_roots = [44_100, 48_000, 88_200, 96_000];

        let mut analyzer = SpectralAnalyzer::new(
            self.config.fft_size,
            self.config.hop_size,
            WindowFunction::BlackmanHarris,
        );
        let spectrum_db = analyzer.compute_power_spectrum_db(samples);
        let bin_hz = sample_rate as f64 / self.config.fft_size as f64;

        if spectrum_db.len() < 64 {
            return Ok(None);
        }

        // Convert to linear power
        let spectrum_pow: Vec<f64> = spectrum_db
            .iter()
            .map(|&db| 10.0_f64.powf(db / 10.0))
            .collect();

        for root in common_roots {
            if root >= sample_rate {
                continue;
            }
            let orig_nyq = root as f64 / 2.0;
            let start_bin = (orig_nyq / bin_hz).round() as usize;
            if start_bin >= spectrum_pow.len() - 4 {
                continue;
            }

            let low_band = &spectrum_pow[start_bin / 2..start_bin];
            let high_band = &spectrum_pow[start_bin..];

            let low_energy = low_band.iter().copied().sum::<f64>() / low_band.len().max(1) as f64;
            let high_energy =
                high_band.iter().copied().sum::<f64>() / high_band.len().max(1) as f64;

            if low_energy <= 1e-14 {
                continue;
            }
            let ratio = high_energy / low_energy;

            // 30 dB gap between low and high
            if ratio < 10f64.powf(-30.0 / 10.0) {
                let confidence =
                    ((10f64.powf(-30.0 / 10.0) - ratio) / 10f64.powf(-30.0 / 10.0)).clamp(0.4, 1.0);

                return Ok(Some(Detection {
                    defect_type: DefectType::Upsampled {
                        original_rate: root,
                        current_rate: sample_rate,
                    },
                    confidence,
                    severity: Severity::High,
                    method: DetectionMethod::SpectralShape,
                    evidence: Some(format!(
                        "Energy above ~{} Hz is {:.1} dB lower than below",
                        orig_nyq as u32,
                        10.0 * (ratio.max(1e-15)).log10()
                    )),
                    temporal: None,
                }));
            }
        }

        Ok(None)
    }

    fn detect_mqa(
        &self,
        samples: &[f32],
        sample_rate: u32,
        bit_depth: u16,
    ) -> Result<Option<Detection>> {
        use crate::core::analysis::mqa_detection::MqaDetector;

        let det = MqaDetector::default();
        let res = det.detect(samples, sample_rate, bit_depth as u32);

        if !res.is_mqa_encoded {
            return Ok(None);
        }

        let mqa_type = res
            .mqa_type
            .as_ref()
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "Unknown".to_string());
        let encoder_version = res
            .encoder_version
            .as_ref()
            .map(|v| format!("{:?}", v))
            .unwrap_or_else(|| "Unknown".to_string());

        Ok(Some(Detection {
            defect_type: DefectType::MqaEncoded {
                original_rate: res.original_sample_rate,
                mqa_type,
                lsb_entropy: res.lsb_entropy as f64,
                encoder_version,
                bit_depth,
            },
            confidence: res.confidence as f64,
            severity: Severity::Info,
            method: DetectionMethod::MqaSignature,
            evidence: Some(res.evidence.join("; ")),
            temporal: None,
        }))
    }

    fn detect_clipping(&self, samples: &[f32], sample_rate: u32) -> Result<Option<Detection>> {
        use crate::core::analysis::clipping_detection::ClippingDetector;
        let det = ClippingDetector::new();
        Ok(det.analyze(samples, sample_rate))
    }

    /// MFCC‑based generic lossy detector (P1).
    fn detect_lossy_via_mfcc(
        &self,
        mfcc: &crate::core::analysis::mfcc::MfccResult,
    ) -> Option<Detection> {
        if mfcc.n_frames < 10 {
            return None;
        }

        let std = &mfcc.stats.std_dev;
        if std.len() <= 5 {
            return None;
        }

        let high_std = std[5..].iter().sum::<f64>() / (std.len() - 5) as f64;
        let base_threshold = 2.2_f64;
        if high_std >= base_threshold {
            return None;
        }

        let delta_ok = if let Some(ref dstd) = mfcc.stats.delta_std {
            if dstd.len() > 5 {
                let high_delta = dstd[5..].iter().sum::<f64>() / (dstd.len() - 5) as f64;
                high_delta < 0.6
            } else {
                true
            }
        } else {
            true
        };

        if !delta_ok {
            return None;
        }

        let mut conf = ((base_threshold - high_std) / base_threshold).clamp(0.3, 1.0);

        let avg_kurt = mfcc.stats.kurtosis[5..]
            .iter()
            .map(|k| k.abs())
            .sum::<f64>()
            / (std.len() - 5) as f64;
        if avg_kurt < 0.5 {
            conf = (conf + 0.15).min(1.0);
        }

        Some(Detection {
            defect_type: DefectType::LossyTranscode {
                codec: "Unknown (MFCC)".to_string(),
                estimated_bitrate: None,
                cutoff_hz: 0,
            },
            confidence: conf,
            severity: if conf > 0.75 {
                Severity::High
            } else {
                Severity::Medium
            },
            method: DetectionMethod::MfccAnalysis,
            evidence: Some(format!(
                "MFCC high-order std={:.3}, mean|kurtosis|={:.3}",
                high_std, avg_kurt
            )),
            temporal: None,
        })
    }

    /// SFM‑based lossy detector (P1).
    fn detect_lossy_via_sfm(&self, mono: &[f64], sample_rate: u32) -> Option<Detection> {
        use rustfft::{num_complex::Complex, FftPlanner};

        let fft_size = 4096;
        let hop_size = 2048;

        if mono.len() < fft_size * 2 {
            return None;
        }

        let nyquist = sample_rate as f64 / 2.0;
        let bin_hz = sample_rate as f64 / fft_size as f64;
        let lo_bin = (8_000.0 / bin_hz).ceil() as usize;
        let hi_bin = ((20_000.0f64.min(nyquist - 100.0)) / bin_hz).floor() as usize;
        if hi_bin <= lo_bin + 10 {
            return None;
        }

        let window: Vec<f64> = (0..fft_size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (fft_size - 1) as f64).cos())
            })
            .collect();

        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let skip = (sample_rate * 5) as usize;
        let start = skip.min(mono.len().saturating_sub(fft_size * 8));
        let buf = &mono[start..];

        let frames = ((buf.len().saturating_sub(fft_size)) / hop_size + 1).min(400);

        let mut sfm_sum = 0.0;
        let mut n_frames = 0usize;

        for f in 0..frames {
            let s = f * hop_size;
            if s + fft_size > buf.len() {
                break;
            }

            let mut tmp: Vec<Complex<f64>> = buf[s..s + fft_size]
                .iter()
                .enumerate()
                .map(|(i, &x)| Complex::new(x * window[i], 0.0))
                .collect();

            fft.process(&mut tmp);

            let powers: Vec<f64> = tmp[lo_bin..=hi_bin.min(fft_size / 2)]
                .iter()
                .map(|c| c.re * c.re + c.im * c.im + 1e-30)
                .collect();

            let n = powers.len() as f64;
            let log_sum: f64 = powers.iter().map(|p| p.ln()).sum();
            let geo = (log_sum / n).exp();
            let arith = powers.iter().sum::<f64>() / n;

            if arith > 1e-15 {
                sfm_sum += geo / arith;
                n_frames += 1;
            }
        }

        if n_frames < 10 {
            return None;
        }

        let sfm = sfm_sum / n_frames as f64;
        let threshold = 0.35;
        if sfm <= threshold {
            return None;
        }

        let conf = ((sfm - threshold) / (0.7 - threshold)).clamp(0.3, 0.95);

        Some(Detection {
            defect_type: DefectType::LossyTranscode {
                codec: "Unknown (SFM)".to_string(),
                estimated_bitrate: None,
                cutoff_hz: 0,
            },
            confidence: conf,
            severity: if conf > 0.7 {
                Severity::High
            } else {
                Severity::Medium
            },
            method: DetectionMethod::StatisticalAnalysis,
            evidence: Some(format!("SFM[8–20 kHz]={:.3}", sfm)),
            temporal: None,
        })
    }

    /// Very lightweight pre‑echo heuristic (P4).
    ///
    /// Looks for transient blocks whose energy envelope rises *before*
    /// the main attack in high‑frequency bands – a signature of MDCT
    /// pre‑echo on drums / percussive content.
    fn detect_pre_echo(&self, mono: &[f64], sample_rate: u32) -> Option<Detection> {
        use rustfft::{num_complex::Complex, FftPlanner};

        let fft_size = 2048;
        let hop = 512;

        if mono.len() < fft_size * 4 {
            return None;
        }

        let bin_hz = sample_rate as f64 / fft_size as f64;
        let hf_start = (6_000.0 / bin_hz).ceil() as usize;

        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let mut hf_env = Vec::new();

        for frame in mono.chunks(hop).take_while(|c| c.len() >= fft_size) {
            let mut buf: Vec<Complex<f64>> = frame[..fft_size]
                .iter()
                .map(|&x| Complex::new(x, 0.0))
                .collect();
            fft.process(&mut buf);

            let e: f64 = buf[hf_start..fft_size / 2]
                .iter()
                .map(|c| c.re * c.re + c.im * c.im)
                .sum();
            hf_env.push(e.max(1e-20).log10());
        }

        if hf_env.len() < 16 {
            return None;
        }

        // Very simple pattern: repeated “spikes before spikes”.
        let mut suspicious = 0usize;
        for w in hf_env.windows(4) {
            let pre = (w[0] + w[1]) / 2.0;
            let attack = (w[2] + w[3]) / 2.0;
            if pre > attack - 0.2 && attack > pre + 0.2 {
                suspicious += 1;
            }
        }

        if suspicious < 4 {
            return None;
        }

        let conf = (suspicious as f64 / hf_env.len() as f64 * 10.0).clamp(0.3, 0.9);

        Some(Detection {
            defect_type: DefectType::LossyTranscode {
                codec: "Unknown (pre-echo)".to_string(),
                estimated_bitrate: None,
                cutoff_hz: 0,
            },
            confidence: conf,
            severity: Severity::Medium,
            method: DetectionMethod::TemporalAnalysis,
            evidence: Some(format!(
                "Pre-echo-like HF envelope pattern in {} frames",
                suspicious
            )),
            temporal: None,
        })
    }

    /// Heuristic multi‑generation marker (P6).
    ///
    /// If we already have *two or more* strong lossy detections from
    /// independent methods (e.g. spectral cutoff + MFCC + SFM), emit a
    /// secondary LossyTranscode with evidence suggesting multiple generations.
    fn detect_multigeneration_lossy(&self, detections: &[Detection]) -> Option<Detection> {
        let strong_lossy: Vec<&Detection> = detections
            .iter()
            .filter(|d| {
                matches!(
                    d.defect_type,
                    DefectType::Mp3Transcode { .. }
                        | DefectType::AacTranscode { .. }
                        | DefectType::OpusTranscode { .. }
                        | DefectType::OggVorbisTranscode { .. }
                        | DefectType::LossyTranscode { .. }
                )
            })
            .filter(|d| d.confidence >= 0.6)
            .collect();

        if strong_lossy.len() < 2 {
            return None;
        }

        // Aggregate confidence across methods.
        let agg_conf: f64 =
            strong_lossy.iter().map(|d| d.confidence).sum::<f64>() / strong_lossy.len() as f64;

        Some(Detection {
            defect_type: DefectType::LossyTranscode {
                codec: "Likely multi-generation lossy".to_string(),
                estimated_bitrate: None,
                cutoff_hz: 0,
            },
            confidence: agg_conf.clamp(0.4, 0.95),
            severity: Severity::High,
            method: DetectionMethod::MultiMethod,
            evidence: Some(format!(
                "{} independent lossy detectors fired (spectral/MFCC/SFM/pre-echo)",
                strong_lossy.len()
            )),
            temporal: None,
        })
    }

    // ──────────────────────────── confidence and metrics ────────────────────────────

    /// Per‑defect confidence gate (P5).
    fn passes_confidence_gate(&self, det: &Detection, global_min: f64) -> bool {
        let (min_for_type, is_likely_high_bitrate) = match det.defect_type {
            DefectType::Mp3Transcode {
                estimated_bitrate, ..
            } => {
                let hb = estimated_bitrate.unwrap_or(192) >= 256;
                (if hb { 0.3 } else { 0.5 }, hb)
            }
            DefectType::AacTranscode {
                estimated_bitrate, ..
            } => {
                let hb = estimated_bitrate.unwrap_or(192) >= 256;
                (if hb { 0.3 } else { 0.5 }, hb)
            }
            DefectType::OpusTranscode {
                estimated_bitrate, ..
            } => (0.3, true),
            DefectType::OggVorbisTranscode { .. } => (0.3, true),
            DefectType::LossyTranscode { .. } => (0.4, false),
            DefectType::Upsampled { .. } => (0.4, false),
            DefectType::BitDepthInflated { .. } => (0.4, false),
            _ => (global_min, false),
        };

        let min_final = min_for_type.min(global_min);
        let mut ok = det.confidence >= min_final;

        // Extra forgiveness for obviously high‑bitrate Opus/AAC/MP3
        if !ok && is_likely_high_bitrate && det.confidence >= min_final * 0.8 {
            ok = true;
        }

        ok
    }

    fn calculate_confidence(&self, detections: &[Detection]) -> f64 {
        if detections.is_empty() {
            return 1.0;
        }

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

        let weighted: f64 = detections
            .iter()
            .map(|d| {
                let w = match d.severity {
                    Severity::Critical => 1.0,
                    Severity::High => 0.8,
                    Severity::Medium => 0.5,
                    Severity::Low => 0.3,
                    Severity::Info => 0.1,
                };
                d.confidence * w
            })
            .sum();

        if total_weight > 0.0 {
            1.0 - (weighted / total_weight)
        } else {
            1.0
        }
    }

    fn calculate_quality_metrics(&self, samples: &[f32], _sample_rate: u32) -> QualityMetrics {
        if samples.is_empty() {
            return QualityMetrics::default();
        }

        let max_s = samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max);
        let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

        let true_peak = if max_s > 0.0 {
            20.0 * (max_s as f64).log10()
        } else {
            -f64::INFINITY
        };

        let rms_db = if rms > 0.0 {
            20.0 * (rms as f64).log10()
        } else {
            -f64::INFINITY
        };

        let crest = if rms > 0.0 {
            20.0 * ((max_s / rms) as f64).log10()
        } else {
            0.0
        };

        let dynamic_range = true_peak - rms_db + 3.0;

        let mut sorted: Vec<f32> = samples.iter().copied().map(f32::abs).collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let idx = sorted.len() / 100;
        let nf_s = sorted[idx];
        let noise_floor = if nf_s > 0.0 {
            20.0 * (nf_s as f64).log10()
        } else {
            -96.0
        };

        QualityMetrics {
            dynamic_range,
            noise_floor,
            spectral_centroid: 0.0,
            crest_factor: crest,
            true_peak,
            lufs_integrated: rms_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_uses_default_config() {
        let d = AudioDetector::with_default_config();
        assert!(d.config.fft_size >= 4096);
    }

    #[test]
    fn codec_estimation_basic() {
        let d = AudioDetector::with_default_config();
        let (c1, b1) = d.estimate_codec(11_000.0);
        assert_eq!(c1, "mp3");
        assert_eq!(b1, 64);

        let (c2, b2) = d.estimate_codec(15_000.0);
        assert_eq!(c2, "mp3");
        assert_eq!(b2, 192);
    }
}
