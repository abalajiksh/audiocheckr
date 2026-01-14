use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use chrono::Local;

// ... existing structs ...
#[derive(Debug, Deserialize, Clone)]
pub struct AnalysisResult {
    pub file_path: String,
    pub detections: Vec<Detection>,
    pub confidence: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Detection {
    pub defect_type: serde_json::Value,
    pub confidence: f64,
    pub severity: String,
}

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: u64,
    pub error_message: Option<String>,
}

#[derive(PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

// Stats collector
pub struct TestStats {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub warnings: usize, // Build warnings from cargo check
}

impl TestStats {
    pub fn new() -> Self {
        Self { total: 0, passed: 0, failed: 0, skipped: 0, warnings: 0 }
    }
}

pub fn send_discord_webhook(webhook_url: &str, suite_name: &str, stats: &TestStats, build_warnings: bool) {
    let client = reqwest::blocking::Client::new();
    
    let color = if stats.failed > 0 {
        15548997 // Red
    } else if build_warnings {
        16776960 // Yellow (Passed with warnings)
    } else {
        5763719 // Green
    };

    let status_title = if stats.failed > 0 {
        format!("❌ {} Failed", suite_name)
    } else if build_warnings {
        format!("⚠️ {} Passed (with warnings)", suite_name)
    } else {
        format!("✅ {} Passed", suite_name)
    };

    let description = format!(
        "**Results:** {}/{} passed\n**Failed:** {}\n**Skipped:** {}\n**Warnings:** {}",
        stats.passed, stats.total, stats.failed, stats.skipped, stats.warnings
    );

    let payload = json!({
        "embeds": [{
            "title": status_title,
            "description": description,
            "color": color,
            "footer": {
                "text": format!("Build finished at {}", Local::now().format("%Y-%m-%d %H:%M:%S"))
            }
        }]
    });

    let _ = client.post(webhook_url)
        .json(&payload)
        .send();
}

// ... existing helper functions ...
