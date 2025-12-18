// tests/test_utils/mod.rs
//
// Shared utilities for test suites with Allure reporting support
// 
// v2: Added "Wrong Defect Type" category for detecting incorrect defect classification
// v3: Added AllureStatus enum for compatibility with qualification_test.rs
// v4: Added "Extra Defects" category for detecting additional wrong defects beyond expected

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

// ============================================================================
// Allure Report Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureTestResult {
    pub uuid: String,
    pub name: String,
    #[serde(rename = "fullName")]
    pub full_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "statusDetails")]
    pub status_details: Option<StatusDetails>,
    pub labels: Vec<AllureLabel>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<AllureParameter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AllureAttachment>,
    pub start: u64,
    pub stop: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureLabel {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureParameter {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureAttachment {
    pub name: String,
    pub source: String,
    #[serde(rename = "type")]
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureCategory {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "matchedStatuses")]
    pub matched_statuses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "messageRegex")]
    pub message_regex: Option<String>,
}

// ============================================================================
// Status Enum (for compatibility with qualification_test.rs)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllureStatus {
    Passed,
    Failed,
    Broken,
    Skipped,
    Unknown,
}

impl AllureStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AllureStatus::Passed => "passed",
            AllureStatus::Failed => "failed",
            AllureStatus::Broken => "broken",
            AllureStatus::Skipped => "skipped",
            AllureStatus::Unknown => "unknown",
        }
    }
}

impl Default for AllureStatus {
    fn default() -> Self {
        AllureStatus::Unknown
    }
}

// ============================================================================
// Severity Levels
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum AllureSeverity {
    Blocker,
    Critical,
    Normal,
    Minor,
    Trivial,
}

impl AllureSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            AllureSeverity::Blocker => "blocker",
            AllureSeverity::Critical => "critical",
            AllureSeverity::Normal => "normal",
            AllureSeverity::Minor => "minor",
            AllureSeverity::Trivial => "trivial",
        }
    }
}

// ============================================================================
// Builder Pattern for Test Results
// ============================================================================

pub struct AllureTestBuilder {
    result: AllureTestResult,
    results_dir: Option<std::path::PathBuf>,
}

impl AllureTestBuilder {
    pub fn new(name: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            result: AllureTestResult {
                uuid: Uuid::new_v4().to_string(),
                name: name.to_string(),
                full_name: name.to_string(),
                status: "passed".to_string(),
                description: None,
                status_details: None,
                labels: Vec::new(),
                parameters: Vec::new(),
                attachments: Vec::new(),
                start: now,
                stop: now,
            },
            results_dir: None,
        }
    }

    pub fn full_name(mut self, full_name: &str) -> Self {
        self.result.full_name = full_name.to_string();
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.result.description = Some(desc.to_string());
        self
    }

    pub fn passed(mut self) -> Self {
        self.result.status = "passed".to_string();
        self
    }

    pub fn failed(mut self, message: &str, trace: Option<&str>) -> Self {
        self.result.status = "failed".to_string();
        self.result.status_details = Some(StatusDetails {
            message: Some(message.to_string()),
            trace: trace.map(|t| t.to_string()),
        });
        self
    }

    pub fn broken(mut self, message: &str, trace: Option<&str>) -> Self {
        self.result.status = "broken".to_string();
        self.result.status_details = Some(StatusDetails {
            message: Some(message.to_string()),
            trace: trace.map(|t| t.to_string()),
        });
        self
    }

    pub fn skipped(mut self, message: &str) -> Self {
        self.result.status = "skipped".to_string();
        self.result.status_details = Some(StatusDetails {
            message: Some(message.to_string()),
            trace: None,
        });
        self
    }

    fn add_label(mut self, name: &str, value: &str) -> Self {
        self.result.labels.push(AllureLabel {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn severity(self, severity: AllureSeverity) -> Self {
        self.add_label("severity", severity.as_str())
    }

    pub fn epic(self, epic: &str) -> Self {
        self.add_label("epic", epic)
    }

    pub fn feature(self, feature: &str) -> Self {
        self.add_label("feature", feature)
    }

    pub fn story(self, story: &str) -> Self {
        self.add_label("story", story)
    }

    pub fn suite(self, suite: &str) -> Self {
        self.add_label("suite", suite)
    }

    pub fn sub_suite(self, sub_suite: &str) -> Self {
        self.add_label("subSuite", sub_suite)
    }

    pub fn tag(self, tag: &str) -> Self {
        self.add_label("tag", tag)
    }

    pub fn parameter(mut self, name: &str, value: &str) -> Self {
        self.result.parameters.push(AllureParameter {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn attach_text(&mut self, name: &str, content: &str, results_dir: &Path) -> Result<(), std::io::Error> {
        let attachment_name = format!("{}-attachment.txt", Uuid::new_v4());
        let attachment_path = results_dir.join(&attachment_name);
        
        fs::create_dir_all(results_dir)?;
        let mut file = File::create(&attachment_path)?;
        file.write_all(content.as_bytes())?;
        
        self.result.attachments.push(AllureAttachment {
            name: name.to_string(),
            source: attachment_name,
            content_type: "text/plain".to_string(),
        });
        
        Ok(())
    }

    pub fn build(mut self) -> AllureTestResult {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.result.stop = now;
        self.result
    }
}

// ============================================================================
// Test Suite Container
// ============================================================================

pub struct AllureTestSuite {
    name: String,
    results: Vec<AllureTestResult>,
    results_dir: std::path::PathBuf,
}

impl AllureTestSuite {
    pub fn new(name: &str, results_dir: &Path) -> Self {
        Self {
            name: name.to_string(),
            results: Vec::new(),
            results_dir: results_dir.to_path_buf(),
        }
    }

    pub fn add_result(&mut self, result: AllureTestResult) {
        self.results.push(result);
    }

    pub fn write_all(&self) -> Result<(), std::io::Error> {
        fs::create_dir_all(&self.results_dir)?;
        
        for result in &self.results {
            let filename = format!("{}-result.json", result.uuid);
            let filepath = self.results_dir.join(&filename);
            let json = serde_json::to_string_pretty(result)?;
            let mut file = File::create(filepath)?;
            file.write_all(json.as_bytes())?;
        }
        
        Ok(())
    }
}

// ============================================================================
// Environment Info
// ============================================================================

pub struct AllureEnvironment {
    properties: HashMap<String, String>,
}

impl AllureEnvironment {
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: &str, value: &str) {
        self.properties.insert(key.to_string(), value.to_string());
    }

    pub fn write(&self, results_dir: &Path) -> Result<(), std::io::Error> {
        fs::create_dir_all(results_dir)?;
        let filepath = results_dir.join("environment.properties");
        let mut file = File::create(filepath)?;
        
        for (key, value) in &self.properties {
            writeln!(file, "{}={}", key, value)?;
        }
        
        Ok(())
    }
}

impl Default for AllureEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Categories Configuration
// ============================================================================

/// Write categories.json to define test failure categories in Allure
pub fn write_categories(categories: &[AllureCategory], results_dir: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(results_dir)?;
    let filepath = results_dir.join("categories.json");
    let json = serde_json::to_string_pretty(categories)?;
    let mut file = File::create(filepath)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Default categories for AudioCheckr tests
/// v2: Added "Wrong Defect Type" category
/// v4: Added "Extra Defects" category for wrong additional detections
pub fn default_audiocheckr_categories() -> Vec<AllureCategory> {
    vec![
        AllureCategory {
            name: "Extra Defects Detected".to_string(),
            description: Some("Expected defects were found but additional wrong defects were also detected".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*EXTRA DEFECTS.*".to_string()),
        },
        AllureCategory {
            name: "Wrong Defect Type".to_string(),
            description: Some("Detected a defect but not the expected type (e.g., Mp3Transcode when Upsampled expected)".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*WRONG DEFECT TYPE.*".to_string()),
        },
        AllureCategory {
            name: "False Positives".to_string(),
            description: Some("Clean files incorrectly flagged as defective".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*FALSE POSITIVE.*".to_string()),
        },
        AllureCategory {
            name: "False Negatives".to_string(),
            description: Some("Defective files not detected".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*FALSE NEGATIVE.*".to_string()),
        },
        AllureCategory {
            name: "Detection Failures".to_string(),
            description: Some("General detection algorithm failures".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*detection.*|.*Detection.*".to_string()),
        },
        AllureCategory {
            name: "Infrastructure Issues".to_string(),
            description: Some("Test infrastructure problems (file not found, binary issues)".to_string()),
            matched_statuses: vec!["broken".to_string()],
            message_regex: None,
        },
        AllureCategory {
            name: "Assertion Failures".to_string(),
            description: Some("Test assertion failures".to_string()),
            matched_statuses: vec!["failed".to_string()],
            message_regex: Some(".*assert.*|.*Assert.*".to_string()),
        },
    ]
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Sanitize a string to be used as a test name (alphanumeric + underscore only)
pub fn sanitize_test_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Format duration in human-readable form
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{} ms", ms)
    } else if ms < 60000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{:.1} min", ms as f64 / 60000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_test_name("test file.flac"), "test_file_flac");
        assert_eq!(sanitize_test_name("Test_123"), "Test_123");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500 ms");
        assert_eq!(format_duration(1500), "1.5 s");
        assert_eq!(format_duration(90000), "1.5 min");
    }
}
