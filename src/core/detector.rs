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
//!
//! ## Fixes applied (diagnostic v2)
//!
//! - **P1 (MFCC/SFM false positives)**: Raised `high_std` threshold from
//!   2.2 → 1.8, added delta-std cross-check, raised SFM threshold from
//!   0.35 → 0.45, clamped 5-second skip to file length, added minimum
//!   signal-energy gate to SFM.
//!
//! - **P2 (Dithering sample rate)**: Changed `DitheringDetector::new()` to
//!   `DitheringDetector::with_sample_rate(sample_rate)` so noise-shaping
//!   FFT bins are mapped correctly at 48 kHz / 96 kHz / etc.
//!
//! - **P3 (Bit-depth voting)**: Relaxed from require-all-3 to require
//!   2-of-3 votes for bit-depth inflation, and added the standalone
//!   `bit_depth.rs` 4-method analyzer as a cross-check.
//!
//! - **P4 (Upsampling shelf)**: Lowered energy-ratio threshold from 30 dB
//!   to 20 dB, and normalised band widths so the comparison is fair.
//!
//! - **P5 (Downsampling)**: Implemented downsampling detection by looking
//!   for anti-alias filter rolloff signatures below the current Nyquist
//!   at common original-Nyquist frequencies.

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
        let mut _has_bit_inflation = false;

        // ── FIX P2: pass sample_rate to dithering detector ──────────
        // 1) Dithering (informational)
        if let Some(det) = self.detect_dithering(samples, bit_depth, sample_rate)? {
            detections.push(det);
        }

        // 2) Resampling artifacts — use mono downmix for spectral analysis
        let mono_f32: Vec<f32> = samples
            .chunks(channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        if let Some(det) = self.detect_resampling(&mono_f32, sample_rate)? {
            has_resampling = true;
            detections.push(det);
        }

        // 3) Spectral cutoff (codec‑specific, P0/P1) – skipped if resampled
        //    FIX: use mono downmix, NOT interleaved stereo
        let spectral_det = if !has_resampling {
            self.detect_spectral_cutoff(&mono_f64, sample_rate)?
        } else {
            None
        };

        if let Some(det) = spectral_det.clone() {
            has_transcode = true;
            detections.push(det);
        }

        // ── FIX P3: relaxed bit-depth inflation ─────────────────────
        // 4) Bit‑depth inflation (multi‑heuristic, relaxed 2-of-3 voting)
        if let Some(det) = self.detect_bit_depth_inflation_multi(samples, bit_depth)? {
            _has_bit_inflation = true;
            detections.push(det);
        }

        // 5) Upsampling shelf (P2).
        //    FIX: use mono downmix, NOT interleaved stereo
        if !has_transcode {
            if let Some(det) = self.detect_upsampling_shelf(&mono_f64, sample_rate)? {
                detections.push(det);
            }
        }

        // ── FIX P5: downsampling detection ──────────────────────────
        // 5b) Downsampling detection (new)
        //    FIX: use mono downmix, NOT interleaved stereo
        if !has_transcode && !has_resampling {
            if let Some(det) = self.detect_downsampling(&mono_f64, sample_rate)? {
                detections.push(det);
            }
        }

        // 6) MQA
        if self.config.enable_mqa {
            if let Some(det) = self.detect_mqa(samples, sample_rate, bit_depth)? {
                detections.push(det);
            }
        }

        // 7) Clipping
        if self.config.enable_clipping {
            if let Some(det) = self.detect_clipping(samples, sample_rate)? {
                detections.push(det);
            }
        }

        // ── FIX P1: tightened MFCC/SFM thresholds ──────────────────
        // 8) MFCC + SFM lossy detection – only if spectral cutoff missed
        let mut mfcc_det: Option<Detection> = None;
        let mut sfm_det: Option<Detection> = None;

        if self.config.enable_mfcc && !has_transcode {
            let mfcc_res = self.run_mfcc_analysis(&mono_f64, sample_rate);
            mfcc_det = self.detect_lossy_via_mfcc(&mfcc_res);
            sfm_det = self.detect_lossy_via_sfm(&mono_f64, sample_rate);
        }

        match (mfcc_det.take(), sfm_det.take()) {
            (Some(m), Some(s)) => {
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

        // 9) Pre‑echo detector
        if let Some(det) = self.detect_pre_echo(&mono_f64, sample_rate) {
            detections.push(det);
        }

        // 10) Multi‑generation heuristic (P6)
        if let Some(det) = self.detect_multigeneration_lossy(&detections) {
            detections.push(det);
        }

        // 11) Post‑processing: prioritise codec / lossy evidence over generic
        // bit‑depth inflation.
        let has_lossy = detections
            .iter()
            .any(|d| d.defect_type.is_lossy_transcode());

        if has_lossy {
            detections.retain(|d| !matches!(d.defect_type, DefectType::BitDepthInflated { .. }));
        }

        // FIX 4: Suppress standalone BitDepthInflated with confidence < 0.85
        // to avoid false positives on genuine 24-bit files with unusual
        // characteristics (heavy limiting, shaped dither, etc.)
        if !has_lossy && !has_resampling {
            detections.retain(|d| {
                if let DefectType::BitDepthInflated { .. } = &d.defect_type {
                    d.confidence >= 0.85
                } else {
                    true
                }
            });
        }

        // Final confidence gating with per‑defect tiers (P5)
        let min_global = self.config.min_confidence;
        detections.retain(|d| self.passes_confidence_gate(d, min_global));

        Ok(detections)
    }

    // ───────────────────────────── individual detectors ─────────────────────────────

    /// ── FIX P2: accept sample_rate and forward to DitheringDetector ──
    fn detect_dithering(
        &self,
        samples: &[f32],
        bit_depth: u16,
        sample_rate: u32,
    ) -> Result<Option<Detection>> {
        use crate::core::analysis::dithering_detection::{DitherType, DitheringDetector};

        // ── FIX P2: use with_sample_rate instead of new() ───────────
        // The old code called DitheringDetector::new() which hardcodes
        // sample_rate=44100.  For 48 kHz / 96 kHz files this shifts the
        // noise-shaping FFT bin → Hz mapping and causes 38/60 dithered
        // test files to return CLEAN.
        let det = DitheringDetector::with_sample_rate(sample_rate);
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
                "{} dither detected at {} bits (sr={})",
                type_str, res.bit_depth, sample_rate
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
            ("aac".to_string(), 256)
        } else if cutoff_hz < 19_500.0 {
            ("aac".to_string(), 320)
        } else if cutoff_hz < 20_500.0 {
            // Could be MP3 320k or AAC 320k — use generic
            ("mp3".to_string(), 320)
        } else {
            ("unknown".to_string(), 0)
        }
    }

    /// ── FIX P3: Multi‑heuristic bit‑depth inflation detector ──────
    ///
    /// Relaxed from require-all-3 to require 2-of-3 votes.
    ///
    /// The old triple-AND gate required:
    ///   (1) effective_bits + 4 <= claimed_bits
    ///   (2) LSB entropy > 0.9 OR < 0.2
    ///   (3) quantisation noise < −96 dB
    ///
    /// Genuine 16→24 with dither has moderate LSB entropy (0.3–0.8)
    /// which fails condition 2.  Relaxing to 2-of-3 catches these
    /// while keeping the false-positive rate low because any two
    /// agreeing heuristics is strong evidence.
    fn detect_bit_depth_inflation_multi(
        &self,
        samples: &[f32],
        claimed_bits: u16,
    ) -> Result<Option<Detection>> {
        if samples.is_empty() || claimed_bits < 20 {
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

        // 2) Effective bit usage
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
            if usage > 0.01 {
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
            e / ((1 << n_lsb) as f64).ln()
        };

        // 4) Quantisation noise estimate
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

        // 5) Voting – ── FIX P3: relaxed to 2-of-3 ──────────────────
        let mut votes = 0usize;

        // Vote 1: significant gap between claimed and effective bits (≥ 4 bits)
        if effective_bits + 4 <= claimed_bits {
            votes += 1;
        }

        // Vote 2: LSB entropy very high (random-filled) or very low (zero-padded)
        //          ── FIX P3: widened entropy window to catch dithered files ──
        //          Old: entropy > 0.9 || entropy < 0.2
        //          New: entropy > 0.85 || entropy < 0.25
        if entropy > 0.85 || entropy < 0.25 {
            votes += 1;
        }

        // Vote 3: residual noise too small for genuine 24-bit+
        if claimed_bits >= 24 && q_noise_db < -96.0 {
            votes += 1;
        }

        // ── FIX P3: require 2-of-3 instead of all 3 ────────────────
        if votes < 2 {
            return Ok(None);
        }

        let bit_gap = (claimed_bits - effective_bits).max(1) as f64;
        let mut confidence = (bit_gap / 8.0).clamp(0.3, 1.0);

        // Boost confidence when all 3 agree
        if votes == 3 {
            confidence = (confidence + 0.15).min(1.0);
        }

        // Boost for strong entropy or residual evidence
        if entropy < 0.1 || entropy > 0.95 {
            confidence = (confidence + 0.1).min(1.0);
        }
        if q_noise_db < -100.0 {
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
                "effective_bits≈{}, claimed_bits={}, LSB_entropy={:.2}, q_noise≈{:.1} dB, votes={}/3",
                effective_bits, claimed_bits, entropy, q_noise_db, votes
            )),
            temporal: None,
        }))
    }

    /// ── FIX P4: Spectral‑shelf upsampling detector ─────────────────
    ///
    /// Lowered threshold from 30 dB → 20 dB.
    ///
    /// Good SRC (SoXR VHQ, iZotope) places imaging at −80 to −120 dB
    /// absolute, but the *average energy ratio* between the low band
    /// (half-width) and high band is only 15–25 dB because:
    /// - Low band was half the width of high band (unfair comparison)
    /// - Ambient noise / dither fills the high band
    /// - High-quality SRCs have gentle rolloff
    ///
    /// Additionally, band widths are now normalised so the comparison
    /// is energy-per-Hz rather than total energy over unequal spans.
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

            if low_band.is_empty() || high_band.is_empty() {
                continue;
            }

            // ── FIX P4: normalise by bandwidth (energy per bin) ─────
            let low_energy = low_band.iter().copied().sum::<f64>() / low_band.len() as f64;
            let high_energy = high_band.iter().copied().sum::<f64>() / high_band.len() as f64;

            if low_energy <= 1e-14 {
                continue;
            }
            let ratio = high_energy / low_energy;

            // ── FIX P4: 20 dB gap instead of 30 dB ─────────────────
            let threshold_ratio = 10f64.powf(-20.0 / 10.0); // 0.01

            if ratio < threshold_ratio {
                let gap_db = -10.0 * ratio.max(1e-15).log10();
                let confidence = ((gap_db - 20.0) / 30.0).clamp(0.4, 0.95);

                return Ok(Some(Detection {
                    defect_type: DefectType::Upsampled {
                        original_rate: root,
                        current_rate: sample_rate,
                    },
                    confidence,
                    severity: Severity::High,
                    method: DetectionMethod::SpectralShape,
                    evidence: Some(format!(
                        "Energy above ~{} Hz is {:.1} dB lower than below (threshold: 20 dB)",
                        orig_nyq as u32, gap_db
                    )),
                    temporal: None,
                }));
            }
        }

        Ok(None)
    }

    /// ── FIX P5: Downsampling detection (new) ───────────────────────
    ///
    /// Detects audio that was downsampled from a higher rate by looking
    /// for anti-alias filter rolloff signatures.
    ///
    /// When audio is downsampled (e.g. 176.4 kHz → 96 kHz), the SRC
    /// applies a low-pass filter at the *target* Nyquist (48 kHz for a
    /// 96 kHz file). If the file is then stored at the target rate,
    /// there's no sign of downsampling in the spectrum (the filter is
    /// right at Nyquist, indistinguishable from normal).
    ///
    /// However, if the audio is further resampled UP (e.g. 96k → 192k)
    /// or if the original filter had imperfect rolloff, we can detect
    /// the filter knee.
    ///
    /// More commonly, we look for cases where the file's actual content
    /// ends well below its Nyquist — e.g. a 192 kHz file whose energy
    /// drops sharply at 48 kHz (was originally 96 kHz, then upsampled
    /// to 192 kHz). This is really upsampling detection, but from the
    /// user's perspective they have a "downsampled then upsampled" file.
    ///
    /// For *true* downsampling (e.g. user has a 96 kHz file that was
    /// downsampled from 192 kHz), the signature is a very steep filter
    /// rolloff in the last few percent of the spectrum — steeper than
    /// what a natural recording would exhibit.
    fn detect_downsampling(&self, samples: &[f64], sample_rate: u32) -> Result<Option<Detection>> {
        // Candidate original rates that are HIGHER than current rate
        let higher_rates: &[u32] = &[88_200, 96_000, 176_400, 192_000, 352_800, 384_000];

        let mut analyzer = SpectralAnalyzer::new(
            self.config.fft_size,
            self.config.hop_size,
            WindowFunction::BlackmanHarris,
        );
        let spectrum_db = analyzer.compute_power_spectrum_db(samples);
        let bin_hz = sample_rate as f64 / self.config.fft_size as f64;
        let nyquist = sample_rate as f64 / 2.0;

        if spectrum_db.len() < 64 {
            return Ok(None);
        }

        // Look for a very steep rolloff in the top 5-10% of spectrum
        // that suggests a brick-wall anti-alias filter from a higher
        // sample rate was applied.
        //
        // For each candidate higher rate, the expected filter knee is at
        // candidate_nyquist mapped into our spectrum. If candidate_nyquist
        // equals our nyquist, the filter is invisible. But for rates that
        // are close-but-not-equal (e.g. 44.1→48 kHz), the filter knee
        // sits just below our Nyquist.

        // Strategy: measure the rolloff steepness in the top 10% of spectrum.
        // A natural recording rolls off gradually; a downsampled file has
        // a brick wall.
        let rolloff_start_bin = (spectrum_db.len() * 85) / 100;
        let rolloff_end_bin = (spectrum_db.len() * 98) / 100;

        if rolloff_end_bin <= rolloff_start_bin + 4 {
            return Ok(None);
        }

        // Measure reference energy in 60-80% of spectrum
        let ref_start = (spectrum_db.len() * 60) / 100;
        let ref_end = (spectrum_db.len() * 80) / 100;

        if ref_end <= ref_start {
            return Ok(None);
        }

        let ref_energy: f64 =
            spectrum_db[ref_start..ref_end].iter().sum::<f64>() / (ref_end - ref_start) as f64;

        let rolloff_energy: f64 = spectrum_db[rolloff_start_bin..rolloff_end_bin]
            .iter()
            .sum::<f64>()
            / (rolloff_end_bin - rolloff_start_bin) as f64;

        let drop_db = ref_energy - rolloff_energy;

        // A natural recording might drop 10-15 dB near Nyquist.
        // A downsampled file drops 40+ dB with a very sharp knee.
        if drop_db > 30.0 {
            // Estimate the original rate from the rolloff position
            // Find the bin where the steep drop begins
            let mut knee_bin = rolloff_start_bin;
            for i in rolloff_start_bin..rolloff_end_bin {
                if spectrum_db[i] < ref_energy - 20.0 {
                    knee_bin = i;
                    break;
                }
            }

            let knee_freq = knee_bin as f64 * bin_hz;

            // Match knee frequency to a plausible original Nyquist
            let mut best_orig = 0u32;
            for &rate in higher_rates {
                if rate <= sample_rate {
                    continue;
                }
                let orig_nyquist = rate as f64 / 2.0;
                // The filter knee should be at our current Nyquist
                // (since that's where the anti-alias filter was set)
                // For same-family rates, this is just at Nyquist
                if (knee_freq - nyquist).abs() < nyquist * 0.15 {
                    best_orig = rate;
                    break;
                }
            }

            if best_orig == 0 {
                // Can't determine original rate, but the steep rolloff
                // is still suspicious
                best_orig = sample_rate * 2;
            }

            let confidence = ((drop_db - 30.0) / 30.0).clamp(0.3, 0.85);

            return Ok(Some(Detection {
                defect_type: DefectType::ResamplingDetected {
                    original_rate: best_orig,
                    target_rate: sample_rate,
                    quality: format!("Downsampled (steep {:.0} dB rolloff near Nyquist)", drop_db),
                },
                confidence,
                severity: Severity::Medium,
                method: DetectionMethod::SpectralShape,
                evidence: Some(format!(
                    "Steep rolloff of {:.1} dB at {:.0} Hz suggests downsampling from {} Hz",
                    drop_db, knee_freq, best_orig
                )),
                temporal: None,
            }));
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

    /// ── FIX P1/v3: MFCC‑based generic lossy detector (relaxed) ─────
    ///
    /// Changes from v2 → v3:
    /// - Raised `base_threshold` from 1.8 → 2.5 (1.8 was over-tightened,
    ///   causing zero detections on real lossy files)
    /// - Relaxed delta cross-check from 0.4 → 0.7
    /// - Lowered energy gate from 0.3 → 0.15
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

        // ── FIX v3: relaxed from 1.8 → 2.5 ─────────────────────────
        // 1.8 was too aggressive — real lossy transcodes often have
        // high_std in the 1.8–2.4 range and were being missed.
        let base_threshold = 2.5_f64;

        if high_std >= base_threshold {
            return None;
        }

        // ── FIX v3: lowered energy gate from 0.3 → 0.15 ────────────
        // Very low std-dev (< 0.15) indicates near-silence, not lossy.
        if high_std < 0.15 {
            return None;
        }

        // ── FIX v3: relaxed delta cross-check from 0.4 → 0.7 ───────
        let delta_ok = if let Some(ref dstd) = mfcc.stats.delta_std {
            if dstd.len() > 5 {
                let high_delta = dstd[5..].iter().sum::<f64>() / (dstd.len() - 5) as f64;
                high_delta < 0.7
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

    /// ── FIX P1/v3: SFM‑based lossy detector (relaxed) ─────────────
    ///
    /// Changes from v2 → v3:
    /// - Lowered threshold from 0.45 → 0.35
    /// - Lowered band energy gate from -60 dB → -75 dB
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

        // ── FIX P1: clamp skip to file length ───────────────────────
        let skip = ((sample_rate * 5) as usize).min(mono.len().saturating_sub(fft_size * 8));
        let buf = &mono[skip..];

        if buf.len() < fft_size * 2 {
            return None;
        }

        let frames = ((buf.len().saturating_sub(fft_size)) / hop_size + 1).min(400);

        let mut sfm_sum = 0.0;
        let mut n_frames = 0usize;
        let mut band_energy_sum = 0.0_f64;

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
                band_energy_sum += arith;
                n_frames += 1;
            }
        }

        if n_frames < 10 {
            return None;
        }

        // ── FIX v3: lowered band energy gate from -60 → -75 dB ─────
        let avg_band_energy = band_energy_sum / n_frames as f64;
        let band_db = if avg_band_energy > 1e-20 {
            10.0 * avg_band_energy.log10()
        } else {
            -120.0
        };
        if band_db < -75.0 {
            return None;
        }

        let sfm = sfm_sum / n_frames as f64;

        // ── FIX v3: lowered threshold from 0.45 → 0.35 ─────────────
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
            evidence: Some(format!(
                "SFM[8–20 kHz]={:.3}, band_energy={:.1} dB",
                sfm, band_db
            )),
            temporal: None,
        })
    }

    /// Very lightweight pre‑echo heuristic (P4).
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

    #[test]
    fn codec_estimation_aac_range() {
        let d = AudioDetector::with_default_config();
        let (c, b) = d.estimate_codec(17_500.0);
        assert_eq!(c, "aac");
        assert_eq!(b, 256);
    }
}
