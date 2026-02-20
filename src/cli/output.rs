use crate::core::analysis::{AnalysisResult, DefectType, Detection, Severity};
use anyhow::Result;
use colorful::{Color, Colorful};
use serde::Serialize;
use serde_json::json;
use std::io::{self, Write};

// ============================================================================
// Badge Definitions (shared by terminal + JSON)
// ============================================================================

/// Serializable badge info — included in JSON output so frontends don't
/// need to re-implement the DefectType → badge mapping.
#[derive(Debug, Clone, Serialize)]
pub struct BadgeInfo {
    /// Short display label, e.g. "MP3", "UPSAMPLED", "MQA"
    pub label: String,
    /// Semantic category for styling: "critical", "warning", "quality", "info"
    pub category: String,
    /// One-line human-readable detail string
    pub detail: String,
}

/// Terminal badge (colors, not serialized)
struct TermBadge {
    label: &'static str,
    fg: Color,
    bg: Color,
}

/// Category constants
const CAT_CRITICAL: &str = "critical";
const CAT_WARNING: &str = "warning";
const CAT_QUALITY: &str = "quality";
const CAT_INFO: &str = "info";

fn defect_term_badge(defect: &DefectType) -> TermBadge {
    match defect {
        DefectType::LossyTranscode { codec, .. } => match codec.to_uppercase().as_str() {
            "MP3" => TermBadge {
                label: " MP3 ",
                fg: Color::White,
                bg: Color::Red,
            },
            "AAC" => TermBadge {
                label: " AAC ",
                fg: Color::White,
                bg: Color::Red,
            },
            "OPUS" => TermBadge {
                label: " OPUS ",
                fg: Color::White,
                bg: Color::Red,
            },
            "VORBIS" => TermBadge {
                label: " VORBIS ",
                fg: Color::White,
                bg: Color::Red,
            },
            _ => TermBadge {
                label: " LOSSY ",
                fg: Color::White,
                bg: Color::Red,
            },
        },
        DefectType::Upsampled { .. } => TermBadge {
            label: " UPSAMPLED ",
            fg: Color::Black,
            bg: Color::Yellow,
        },
        DefectType::BitDepthInflated { .. } => TermBadge {
            label: " BIT DEPTH ",
            fg: Color::Black,
            bg: Color::Yellow,
        },
        DefectType::Clipping { .. } => TermBadge {
            label: " CLIPPING ",
            fg: Color::White,
            bg: Color::Magenta,
        },
        DefectType::SilencePadding { .. } => TermBadge {
            label: " PADDING ",
            fg: Color::White,
            bg: Color::Blue,
        },
        DefectType::MqaEncoded { .. } => TermBadge {
            label: " MQA ",
            fg: Color::White,
            bg: Color::Cyan,
        },
        DefectType::UpsampledLossyTranscode { .. } => TermBadge {
            label: " UPSAMPLED+LOSSY ",
            fg: Color::White,
            bg: Color::Red,
        },
        DefectType::DitheringDetected { .. } => TermBadge {
            label: " DITHER ",
            fg: Color::White,
            bg: Color::Blue,
        },
        DefectType::ResamplingDetected { .. } => TermBadge {
            label: " RESAMPLED ",
            fg: Color::Black,
            bg: Color::Yellow,
        },
        DefectType::LoudnessWarVictim { .. } => TermBadge {
            label: " LOUDNESS WAR ",
            fg: Color::White,
            bg: Color::Magenta,
        },
    }
}

/// Single source of truth: DefectType → serializable badge info.
/// Used by both terminal rendering and JSON enrichment.
pub fn defect_badge_info(defect: &DefectType) -> BadgeInfo {
    let (label, category) = match defect {
        DefectType::LossyTranscode { codec, .. } => {
            (codec.to_uppercase(), CAT_CRITICAL.to_string())
        }
        DefectType::Upsampled { .. } => ("UPSAMPLED".into(), CAT_WARNING.into()),
        DefectType::BitDepthInflated { .. } => ("BIT DEPTH".into(), CAT_WARNING.into()),
        DefectType::Clipping { .. } => ("CLIPPING".into(), CAT_QUALITY.into()),
        DefectType::SilencePadding { .. } => ("PADDING".into(), CAT_INFO.into()),
        DefectType::MqaEncoded { .. } => ("MQA".into(), CAT_INFO.into()),
        DefectType::UpsampledLossyTranscode { .. } => {
            ("UPSAMPLED+LOSSY".into(), CAT_CRITICAL.into())
        }
        DefectType::DitheringDetected { .. } => ("DITHER".into(), CAT_INFO.into()),
        DefectType::ResamplingDetected { .. } => ("RESAMPLED".into(), CAT_WARNING.into()),
        DefectType::LoudnessWarVictim { .. } => ("LOUDNESS WAR".into(), CAT_QUALITY.into()),
    };

    BadgeInfo {
        label,
        category,
        detail: format_defect_detail(defect),
    }
}

fn severity_term_badge(severity: &Severity) -> (&'static str, Color) {
    match severity {
        Severity::Critical => ("CRIT", Color::Red),
        Severity::High => ("HIGH", Color::Red),
        Severity::Medium => (" MED", Color::Yellow),
        Severity::Low => (" LOW", Color::Blue),
        Severity::Info => ("INFO", Color::Cyan),
    }
}

// ============================================================================
// Quality Bar (terminal only)
// ============================================================================

fn quality_bar(score: f64, width: usize) -> String {
    let filled = ((score * width as f64).round() as usize).min(width);
    let empty = width - filled;

    let bar_color = if score >= 0.9 {
        Color::Green
    } else if score >= 0.7 {
        Color::Yellow
    } else {
        Color::Red
    };

    format!(
        "{}{} {:.0}%",
        "█".repeat(filled).color(bar_color),
        "░".repeat(empty).color(Color::DarkGray),
        score * 100.0
    )
}

fn verdict_label(genuine: bool, score: f64) -> &'static str {
    if genuine {
        if score >= 0.9 {
            "Lossless"
        } else {
            "Probably Lossless"
        }
    } else if score < 0.5 {
        "Lossy / Fake"
    } else {
        "Suspect"
    }
}

fn verdict_display(genuine: bool, score: f64) -> String {
    let label = verdict_label(genuine, score);
    if genuine {
        label.color(Color::Green).to_string()
    } else if score < 0.5 {
        label.color(Color::Red).to_string()
    } else {
        label.color(Color::Yellow).to_string()
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn format_sample_rate(rate: u32) -> String {
    if rate % 1000 == 0 {
        format!("{} kHz", rate / 1000)
    } else {
        format!("{:.1} kHz", rate as f64 / 1000.0)
    }
}

fn dim(text: &str) -> String {
    text.color(Color::DarkGray).to_string()
}

fn compute_quality_score(result: &AnalysisResult) -> f64 {
    result
        .quality_metrics
        .as_ref()
        .map(|m| 1.0 - (m.noise_floor.abs() / 120.0).min(1.0))
        .unwrap_or(if result.is_genuine() { 0.95 } else { 0.4 })
}

// ============================================================================
// Defect Detail Formatting
// ============================================================================

fn format_defect_detail(defect: &DefectType) -> String {
    match defect {
        DefectType::LossyTranscode {
            codec,
            estimated_bitrate,
            cutoff_hz,
        } => {
            let bitrate = estimated_bitrate
                .map(|b| format!(" @ ~{} kbps", b))
                .unwrap_or_default();
            format!("{}{}, cutoff {} Hz", codec, bitrate, cutoff_hz)
        }
        DefectType::Upsampled {
            original_rate,
            current_rate,
        } => {
            format!(
                "{} → {}",
                format_sample_rate(*original_rate),
                format_sample_rate(*current_rate)
            )
        }
        DefectType::BitDepthInflated {
            actual_bits,
            claimed_bits,
        } => {
            format!(
                "{}-bit content in {}-bit container",
                actual_bits, claimed_bits
            )
        }
        DefectType::Clipping {
            peak_level,
            clipped_samples,
        } => {
            format!(
                "peak {:.1} dBFS, {} samples clipped",
                peak_level, clipped_samples
            )
        }
        DefectType::SilencePadding { padding_duration } => {
            format!("{:.2}s of silence padding", padding_duration)
        }
        DefectType::MqaEncoded {
            encoder_version,
            bit_depth,
            ..
        } => {
            format!("encoder v{}, {}-bit container", encoder_version, bit_depth)
        }
        DefectType::UpsampledLossyTranscode {
            codec,
            original_rate,
            current_rate,
            estimated_bitrate,
            cutoff_hz,
        } => {
            let bitrate = estimated_bitrate
                .map(|b| format!(" ~{} kbps", b))
                .unwrap_or_default();
            format!(
                "{}{} upsampled {} → {} Hz, cutoff {} Hz",
                codec, bitrate, original_rate, current_rate, cutoff_hz
            )
        }
        DefectType::DitheringDetected {
            dither_type,
            bit_depth,
            noise_shaping,
        } => {
            let shaping = if *noise_shaping { ", noise-shaped" } else { "" };
            format!("{} → {}-bit{}", dither_type, bit_depth, shaping)
        }
        DefectType::ResamplingDetected {
            original_rate,
            target_rate,
            quality,
        } => {
            let orig = if *original_rate > 0 {
                format!("{} Hz → ", original_rate)
            } else {
                String::new()
            };
            format!("{}{} Hz ({})", orig, target_rate, quality)
        }
        DefectType::LoudnessWarVictim {
            tt_dr_score,
            integrated_lufs,
            plr_db,
        } => {
            format!(
                "DR {:.0}, {:.1} LUFS, PLR {:.1} dB",
                tt_dr_score, integrated_lufs, plr_db
            )
        }
    }
}

// ============================================================================
// Enriched JSON Structures
// ============================================================================

#[derive(Serialize)]
struct EnrichedOutput<'a> {
    file: String,
    format: FileFormat,
    verdict: VerdictInfo,
    detections: Vec<EnrichedDetection<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality_metrics: Option<&'a crate::core::analysis::QualityMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dynamic_range: Option<&'a crate::core::analysis::DynamicRangeResult>,
}

#[derive(Serialize)]
struct FileFormat {
    sample_rate: u32,
    sample_rate_display: String,
    bit_depth: u32,
    channels: u32,
    extension: String,
}

#[derive(Serialize)]
struct VerdictInfo {
    genuine: bool,
    quality_score: f64,
    label: String,
    badge_labels: Vec<String>,
}

#[derive(Serialize)]
struct EnrichedDetection<'a> {
    badge: BadgeInfo,
    severity: String,
    confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<&'a str>,
    defect_type: &'a DefectType,
}

fn build_enriched<'a>(result: &'a AnalysisResult) -> EnrichedOutput<'a> {
    let genuine = result.is_genuine();
    let score = compute_quality_score(result);

    let enriched_detections: Vec<EnrichedDetection<'a>> = result
        .detections
        .iter()
        .map(|d| EnrichedDetection {
            badge: defect_badge_info(&d.defect_type),
            severity: format!("{:?}", d.severity).to_lowercase(),
            confidence: d.confidence,
            evidence: d.evidence.as_deref(),
            defect_type: &d.defect_type,
        })
        .collect();

    let badge_labels: Vec<String> = enriched_detections
        .iter()
        .map(|d| d.badge.label.clone())
        .collect();

    let ext = result
        .file_path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_uppercase();

    EnrichedOutput {
        file: result.file_path.display().to_string(),
        format: FileFormat {
            sample_rate: result.sample_rate,
            sample_rate_display: format_sample_rate(result.sample_rate),
            bit_depth: result.bit_depth as u32,
            channels: result.channels as u32,
            extension: ext,
        },
        verdict: VerdictInfo {
            genuine,
            quality_score: score,
            label: verdict_label(genuine, score).to_string(),
            badge_labels,
        },
        detections: enriched_detections,
        quality_metrics: result.quality_metrics.as_ref(),
        dynamic_range: result.dynamic_range.as_ref(),
    }
}

// ============================================================================
// OutputHandler
// ============================================================================

pub struct OutputHandler {
    verbose: bool,
}

impl OutputHandler {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    // ── Text output (to arbitrary writer) ───────────────────────────

    pub fn write_text(&self, result: &AnalysisResult, w: &mut dyn Write) -> Result<()> {
        let filename = result
            .file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let genuine = result.is_genuine();
        let score = compute_quality_score(result);

        // Header
        let header_icon = if genuine { "✓" } else { "✗" };
        let header_color = if genuine { Color::Green } else { Color::Red };
        writeln!(w)?;
        writeln!(
            w,
            "{}  {}",
            header_icon.color(header_color),
            filename.to_string().color(Color::White),
        )?;

        // Metadata
        writeln!(
            w,
            "   {} {} {} {}  {}  {} ch",
            dim("│"),
            format_sample_rate(result.sample_rate),
            dim("/"),
            format!("{}-bit", result.bit_depth),
            result
                .file_path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_uppercase(),
            result.channels,
        )?;

        // Quality bar
        writeln!(
            w,
            "   {} Quality  {}   {}",
            dim("│"),
            quality_bar(score, 20),
            verdict_display(genuine, score),
        )?;

        // Badge row
        if !result.detections.is_empty() {
            let badges: Vec<String> = result
                .detections
                .iter()
                .map(|d| {
                    let b = defect_term_badge(&d.defect_type);
                    b.label.color(b.fg).bg_color(b.bg).to_string()
                })
                .collect();
            writeln!(w, "   {} {}", dim("│"), badges.join(" "))?;
        }

        // Detailed detections
        if !result.detections.is_empty() && (self.verbose || !genuine) {
            writeln!(w, "   {}", dim("│"))?;
            writeln!(w, "   {}  {}", dim("│"), dim("Detections"))?;

            for detection in &result.detections {
                let (sev_label, sev_color) = severity_term_badge(&detection.severity);
                let conf = format!("{:.0}%", detection.confidence * 100.0);

                writeln!(
                    w,
                    "   {}  {} {}  {}",
                    dim("│"),
                    sev_label.color(sev_color),
                    dim(&conf),
                    format_defect_detail(&detection.defect_type),
                )?;

                if let Some(evidence) = &detection.evidence {
                    writeln!(w, "   {}       {}", dim("│"), dim(evidence))?;
                }
            }
        }

        // Verbose: quality metrics
        if self.verbose {
            if let Some(metrics) = &result.quality_metrics {
                writeln!(w, "   {}", dim("│"))?;
                writeln!(w, "   {}  {}", dim("│"), dim("Signal"))?;
                writeln!(
                    w,
                    "   {}  Dynamic Range  {:.1} dB",
                    dim("│"),
                    metrics.dynamic_range
                )?;
                writeln!(
                    w,
                    "   {}  Noise Floor    {:.1} dB",
                    dim("│"),
                    metrics.noise_floor
                )?;
            }
        }

        // Dynamic range
        if let Some(ref dr) = result.dynamic_range {
            writeln!(w, "   {}", dim("│"))?;
            writeln!(w, "   {}  {}", dim("│"), dim("Dynamic Range"))?;
            writeln!(
                w,
                "   {}  TT DR         {:.1} dB  {}",
                dim("│"),
                dr.tt_dr_score,
                dr_verdict_colored(&format!("{}", dr.verdict))
            )?;
            writeln!(
                w,
                "   {}  LUFS          {:.1}",
                dim("│"),
                dr.integrated_loudness_lufs
            )?;
            writeln!(
                w,
                "   {}  Crest Factor  {:.1} dB",
                dim("│"),
                dr.crest_factor_db
            )?;
            writeln!(w, "   {}  PLR           {:.1} dB", dim("│"), dr.plr_db)?;
            writeln!(
                w,
                "   {}  True Peak     {:.1} dBFS",
                dim("│"),
                dr.true_peak_dbfs
            )?;
            if dr.loudness_war_victim {
                writeln!(
                    w,
                    "   {}  {}",
                    dim("│"),
                    "⚠ Loudness war victim".color(Color::Yellow)
                )?;
            }
        }

        // Footer
        if self.verbose {
            writeln!(w, "   {}", dim("│"))?;
            writeln!(
                w,
                "   {}  {}",
                dim("╰"),
                dim(&result.file_path.display().to_string())
            )?;
        } else {
            writeln!(w, "   {}", dim("╰"))?;
        }

        Ok(())
    }

    /// Text to stdout
    pub fn print_result(&self, result: &AnalysisResult) -> Result<()> {
        self.write_text(result, &mut io::stdout().lock())
    }

    // ── JSON output ─────────────────────────────────────────────────

    /// Enriched JSON to a writer
    pub fn write_json(&self, result: &AnalysisResult, w: &mut dyn Write) -> Result<()> {
        let enriched = build_enriched(result);
        serde_json::to_writer_pretty(&mut *w, &enriched)?;
        writeln!(w)?;
        Ok(())
    }

    /// Enriched JSON to stdout
    pub fn print_json(&self, result: &AnalysisResult) -> Result<()> {
        self.write_json(result, &mut io::stdout().lock())
    }

    // ── Both mode: text → stderr, JSON → stdout ─────────────────────

    pub fn print_both(&self, result: &AnalysisResult) -> Result<()> {
        self.write_text(result, &mut io::stderr().lock())?;
        self.write_json(result, &mut io::stdout().lock())?;
        Ok(())
    }

    // ── Summary ─────────────────────────────────────────────────────

    pub fn write_summary(
        &self,
        total: usize,
        genuine: usize,
        suspect: usize,
        errors: usize,
        w: &mut dyn Write,
    ) -> Result<()> {
        writeln!(w)?;
        writeln!(w, "{}", dim(&"─".repeat(50)))?;
        writeln!(
            w,
            "  {} files analyzed",
            total.to_string().color(Color::White)
        )?;
        if genuine > 0 {
            writeln!(w, "  {} genuine", genuine.to_string().color(Color::Green))?;
        }
        if suspect > 0 {
            writeln!(w, "  {} suspect", suspect.to_string().color(Color::Red))?;
        }
        if errors > 0 {
            writeln!(w, "  {} errors", errors.to_string().color(Color::Yellow))?;
        }
        writeln!(w, "{}", dim(&"─".repeat(50)))?;
        Ok(())
    }

    pub fn print_summary(&self, total: usize, genuine: usize, suspect: usize, errors: usize) {
        let _ = self.write_summary(total, genuine, suspect, errors, &mut io::stdout().lock());
    }

    /// Summary to stderr (for "both" mode — keeps stdout as clean JSON)
    pub fn print_summary_stderr(
        &self,
        total: usize,
        genuine: usize,
        suspect: usize,
        errors: usize,
    ) {
        let _ = self.write_summary(total, genuine, suspect, errors, &mut io::stderr().lock());
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

fn dr_verdict_colored(verdict: &str) -> String {
    let lower = verdict.to_lowercase();
    if lower.contains("excellent") || lower.contains("good") {
        verdict.color(Color::Green).to_string()
    } else if lower.contains("moderate") || lower.contains("acceptable") {
        verdict.color(Color::Yellow).to_string()
    } else {
        verdict.color(Color::Red).to_string()
    }
}
