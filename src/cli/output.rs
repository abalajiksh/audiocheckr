use crate::core::analysis::{AnalysisResult, DefectType, Detection, Severity};
use anyhow::Result;
use colorful::{Color, Colorful};
use serde_json::json;

// ============================================================================
// Badge Definitions
// ============================================================================

struct Badge {
    label: &'static str,
    fg: Color,
    bg: Color,
}

/// Map each defect type to a compact, colored badge
fn defect_badge(defect: &DefectType) -> Badge {
    match defect {
        DefectType::LossyTranscode { codec, .. } => match codec.to_uppercase().as_str() {
            "MP3" => Badge { label: " MP3 ", fg: Color::White, bg: Color::Red },
            "AAC" => Badge { label: " AAC ", fg: Color::White, bg: Color::Red },
            "OPUS" => Badge { label: " OPUS ", fg: Color::White, bg: Color::Red },
            "VORBIS" => Badge { label: " VORBIS ", fg: Color::White, bg: Color::Red },
            _ => Badge { label: " LOSSY ", fg: Color::White, bg: Color::Red },
        },
        DefectType::Upsampled { .. } =>
            Badge { label: " UPSAMPLED ", fg: Color::Black, bg: Color::Yellow },
        DefectType::BitDepthInflated { .. } =>
            Badge { label: " BIT DEPTH ", fg: Color::Black, bg: Color::Yellow },
        DefectType::Clipping { .. } =>
            Badge { label: " CLIPPING ", fg: Color::White, bg: Color::Magenta },
        DefectType::SilencePadding { .. } =>
            Badge { label: " PADDING ", fg: Color::White, bg: Color::Blue },
        DefectType::MqaEncoded { .. } =>
            Badge { label: " MQA ", fg: Color::White, bg: Color::Cyan },
        DefectType::UpsampledLossyTranscode { .. } =>
            Badge { label: " UPSAMPLED+LOSSY ", fg: Color::White, bg: Color::Red },
        DefectType::DitheringDetected { .. } =>
            Badge { label: " DITHER ", fg: Color::White, bg: Color::Blue },
        DefectType::ResamplingDetected { .. } =>
            Badge { label: " RESAMPLED ", fg: Color::Black, bg: Color::Yellow },
        DefectType::LoudnessWarVictim { .. } =>
            Badge { label: " LOUDNESS WAR ", fg: Color::White, bg: Color::Magenta },
    }
}

fn severity_badge(severity: &Severity) -> (&'static str, Color) {
    match severity {
        Severity::Critical => ("CRIT", Color::Red),
        Severity::High     => ("HIGH", Color::Red),
        Severity::Medium   => (" MED", Color::Yellow),
        Severity::Low      => (" LOW", Color::Blue),
        Severity::Info     => ("INFO", Color::Cyan),
    }
}

// ============================================================================
// Quality Bar
// ============================================================================

/// Render a visual quality bar:  ████████░░  82%
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

    let filled_str: String = "█".repeat(filled);
    let empty_str: String = "░".repeat(empty);

    format!(
        "{}{} {:.0}%",
        filled_str.color(bar_color),
        empty_str.color(Color::DarkGray),
        score * 100.0
    )
}

fn verdict_display(genuine: bool, score: f64) -> String {
    if genuine {
        if score >= 0.9 {
            "Lossless".to_string().color(Color::Green).to_string()
        } else {
            "Probably Lossless".to_string().color(Color::Green).to_string()
        }
    } else {
        if score < 0.5 {
            "Lossy / Fake".to_string().color(Color::Red).to_string()
        } else {
            "Suspect".to_string().color(Color::Yellow).to_string()
        }
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

// ============================================================================
// Defect Detail Formatting
// ============================================================================

fn format_defect_detail(defect: &DefectType) -> String {
    match defect {
        DefectType::LossyTranscode { codec, estimated_bitrate, cutoff_hz } => {
            let bitrate = estimated_bitrate
                .map(|b| format!(" @ ~{} kbps", b))
                .unwrap_or_default();
            format!("{}{}, cutoff {} Hz", codec, bitrate, cutoff_hz)
        }
        DefectType::Upsampled { original_rate, current_rate } => {
            format!(
                "{} → {}",
                format_sample_rate(*original_rate),
                format_sample_rate(*current_rate)
            )
        }
        DefectType::BitDepthInflated { actual_bits, claimed_bits } => {
            format!("{}-bit content in {}-bit container", actual_bits, claimed_bits)
        }
        DefectType::Clipping { peak_level, clipped_samples } => {
            format!("peak {:.1} dBFS, {} samples clipped", peak_level, clipped_samples)
        }
        DefectType::SilencePadding { padding_duration } => {
            format!("{:.2}s of silence padding", padding_duration)
        }
        DefectType::MqaEncoded { encoder_version, bit_depth, .. } => {
            format!("encoder v{}, {}-bit container", encoder_version, bit_depth)
        }
        DefectType::UpsampledLossyTranscode { codec, original_rate, current_rate, estimated_bitrate, cutoff_hz } => {
            let bitrate = estimated_bitrate
                .map(|b| format!(" ~{} kbps", b))
                .unwrap_or_default();
            format!(
                "{}{} upsampled {} → {} Hz, cutoff {} Hz",
                codec, bitrate, original_rate, current_rate, cutoff_hz
            )
        }
        DefectType::DitheringDetected { dither_type, bit_depth, noise_shaping } => {
            let shaping = if *noise_shaping { ", noise-shaped" } else { "" };
            format!("{} → {}-bit{}", dither_type, bit_depth, shaping)
        }
        DefectType::ResamplingDetected { original_rate, target_rate, quality } => {
            let orig = if *original_rate > 0 {
                format!("{} Hz → ", original_rate)
            } else {
                String::new()
            };
            format!("{}{} Hz ({})", orig, target_rate, quality)
        }
        DefectType::LoudnessWarVictim { tt_dr_score, integrated_lufs, plr_db } => {
            format!(
                "DR {:.0}, {:.1} LUFS, PLR {:.1} dB",
                tt_dr_score, integrated_lufs, plr_db
            )
        }
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

    // ── Text output ─────────────────────────────────────────────────────

    pub fn print_result(&self, result: &AnalysisResult) -> Result<()> {
        let filename = result
            .file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let genuine = result.is_genuine();
        let score = result
            .quality_metrics
            .as_ref()
            .map(|m| 1.0 - (m.noise_floor.abs() / 120.0).min(1.0))
            .unwrap_or(if genuine { 0.95 } else { 0.4 });

        // ── Header ──────────────────────────────────────────────────
        println!();
        let header_icon = if genuine { "✓" } else { "✗" };
        let header_color = if genuine { Color::Green } else { Color::Red };
        println!(
            "{}  {}",
            header_icon.color(header_color),
            filename.to_string().color(Color::White),
        );

        // ── Metadata line ───────────────────────────────────────────
        let meta = format!(
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
        );
        println!("{}", meta);

        // ── Quality bar ─────────────────────────────────────────────
        println!(
            "   {} Quality  {}   {}",
            dim("│"),
            quality_bar(score, 20),
            verdict_display(genuine, score),
        );

        // ── Badge row (compact defect summary at a glance) ──────────
        if !result.detections.is_empty() {
            let badges: Vec<String> = result
                .detections
                .iter()
                .map(|d| {
                    let b = defect_badge(&d.defect_type);
                    b.label.color(b.fg).bg_color(b.bg).to_string()
                })
                .collect();

            println!("   {} {}", dim("│"), badges.join(" "));
        }

        // ── Detailed detections ─────────────────────────────────────
        if !result.detections.is_empty() && (self.verbose || !genuine) {
            println!("   {}", dim("│"));
            println!("   {}  {}", dim("│"), dim("Detections"));

            for detection in &result.detections {
                let (sev_label, sev_color) = severity_badge(&detection.severity);
                let conf = format!("{:.0}%", detection.confidence * 100.0);

                println!(
                    "   {}  {} {}  {}",
                    dim("│"),
                    sev_label.color(sev_color),
                    dim(&conf),
                    format_defect_detail(&detection.defect_type),
                );

                if let Some(evidence) = &detection.evidence {
                    println!(
                        "   {}       {}",
                        dim("│"),
                        dim(evidence),
                    );
                }
            }
        }

        // ── Verbose: extra quality metrics ──────────────────────────
        if self.verbose {
            if let Some(metrics) = &result.quality_metrics {
                println!("   {}", dim("│"));
                println!("   {}  {}", dim("│"), dim("Signal"));
                println!(
                    "   {}  Dynamic Range  {:.1} dB",
                    dim("│"),
                    metrics.dynamic_range
                );
                println!(
                    "   {}  Noise Floor    {:.1} dB",
                    dim("│"),
                    metrics.noise_floor
                );
            }
        }

        // ── Dynamic range block ─────────────────────────────────────
        if let Some(ref dr) = result.dynamic_range {
            println!("   {}", dim("│"));
            println!("   {}  {}", dim("│"), dim("Dynamic Range"));
            println!(
                "   {}  TT DR         {:.1} dB  {}",
                dim("│"),
                dr.tt_dr_score,
                dr_verdict_colored(&dr.verdict),
            );
            println!(
                "   {}  LUFS          {:.1}",
                dim("│"),
                dr.integrated_loudness_lufs,
            );
            println!(
                "   {}  Crest Factor  {:.1} dB",
                dim("│"),
                dr.crest_factor_db,
            );
            println!(
                "   {}  PLR           {:.1} dB",
                dim("│"),
                dr.plr_db,
            );
            println!(
                "   {}  True Peak     {:.1} dBFS",
                dim("│"),
                dr.true_peak_dbfs,
            );
            if dr.loudness_war_victim {
                println!(
                    "   {}  {}",
                    dim("│"),
                    "⚠ Loudness war victim".color(Color::Yellow),
                );
            }
        }

        // ── Path (verbose only) ─────────────────────────────────────
        if self.verbose {
            println!("   {}", dim("│"));
            println!(
                "   {}  {}",
                dim("╰"),
                dim(&result.file_path.display().to_string()),
            );
        } else {
            println!("   {}", dim("╰"));
        }

        Ok(())
    }

    // ── JSON output ─────────────────────────────────────────────────

    pub fn print_json(&self, result: &AnalysisResult) -> Result<()> {
        let json = json!(result);
        println!("{}", serde_json::to_string_pretty(&json)?);
        Ok(())
    }

    // ── Summary ─────────────────────────────────────────────────────

    pub fn print_summary(
        &self,
        total: usize,
        genuine: usize,
        suspect: usize,
        errors: usize,
    ) {
        println!();
        println!("{}", dim(&"─".repeat(50)));
        println!(
            "  {} files analyzed",
            total.to_string().color(Color::White),
        );

        if genuine > 0 {
            println!(
                "  {} genuine",
                genuine.to_string().color(Color::Green),
            );
        }
        if suspect > 0 {
            println!(
                "  {} suspect",
                suspect.to_string().color(Color::Red),
            );
        }
        if errors > 0 {
            println!(
                "  {} errors",
                errors.to_string().color(Color::Yellow),
            );
        }
        println!("{}", dim(&"─".repeat(50)));
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
