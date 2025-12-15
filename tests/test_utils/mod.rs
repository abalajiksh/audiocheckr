// tests/test_utils/mod.rs
//
// Test utilities for generating Allure-compatible reports
//
// This module provides helpers for:
// - Generating JUnit XML output with Allure extensions
// - Creating Allure result JSON files directly
// - Test case metadata (severity, epic, feature, story)

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use serde::{Deserialize, Serialize};

/// Allure severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AllureSeverity {
    Blocker,
    Critical,
    Normal,
    Minor,
    Trivial,
}

/// Allure test status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AllureStatus {
    Passed,
    Failed,
    Broken,
    Skipped,
    Unknown,
}

/// Allure label types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureLabel {
    pub name: String,
    pub value: String,
}

impl AllureLabel {
    pub fn severity(severity: AllureSeverity) -> Self {
        Self {
            name: "severity".to_string(),
            value: format!("{:?}", severity).to_lowercase(),
        }
    }
    
    pub fn epic(name: &str) -> Self {
        Self {
            name: "epic".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn feature(name: &str) -> Self {
        Self {
            name: "feature".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn story(name: &str) -> Self {
        Self {
            name: "story".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn suite(name: &str) -> Self {
        Self {
            name: "suite".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn sub_suite(name: &str) -> Self {
        Self {
            name: "subSuite".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn parent_suite(name: &str) -> Self {
        Self {
            name: "parentSuite".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn tag(name: &str) -> Self {
        Self {
            name: "tag".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn owner(name: &str) -> Self {
        Self {
            name: "owner".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn host(name: &str) -> Self {
        Self {
            name: "host".to_string(),
            value: name.to_string(),
        }
    }
    
    pub fn thread(name: &str) -> Self {
        Self {
            name: "thread".to_string(),
            value: name.to_string(),
        }
    }
}

/// Allure attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureAttachment {
    pub name: String,
    pub source: String,
    #[serde(rename = "type")]
    pub mime_type: String,
}

/// Allure step result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureStep {
    pub name: String,
    pub status: AllureStatus,
    pub start: u64,
    pub stop: u64,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub steps: Vec<AllureStep>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<AllureAttachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_details: Option<AllureStatusDetails>,
}

/// Status details for failures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureStatusDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flaky: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<String>,
}

/// Allure test result (main structure written to allure-results)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllureTestResult {
    pub uuid: String,
    pub history_id: String,
    pub full_name: String,
    pub name: String,
    pub status: AllureStatus,
    pub start: u64,
    pub stop: u64,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<AllureLabel>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub steps: Vec<AllureStep>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<AllureAttachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_details: Option<AllureStatusDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub parameters: HashMap<String, String>,
}

/// Builder for AllureTestResult
pub struct AllureTestBuilder {
    result: AllureTestResult,
    current_step: Option<AllureStep>,
}

impl AllureTestBuilder {
    pub fn new(name: &str) -> Self {
        let uuid = generate_uuid();
        let history_id = generate_history_id(name);
        let start = current_timestamp_ms();
        
        Self {
            result: AllureTestResult {
                uuid,
                history_id,
                full_name: name.to_string(),
                name: name.to_string(),
                status: AllureStatus::Unknown,
                start,
                stop: start,
                labels: vec![],
                steps: vec![],
                attachments: vec![],
                status_details: None,
                description: None,
                description_html: None,
                parameters: HashMap::new(),
            },
            current_step: None,
        }
    }
    
    pub fn full_name(mut self, name: &str) -> Self {
        self.result.full_name = name.to_string();
        self
    }
    
    pub fn description(mut self, desc: &str) -> Self {
        self.result.description = Some(desc.to_string());
        self
    }
    
    pub fn description_html(mut self, html: &str) -> Self {
        self.result.description_html = Some(html.to_string());
        self
    }
    
    pub fn severity(mut self, severity: AllureSeverity) -> Self {
        self.result.labels.push(AllureLabel::severity(severity));
        self
    }
    
    pub fn epic(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::epic(name));
        self
    }
    
    pub fn feature(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::feature(name));
        self
    }
    
    pub fn story(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::story(name));
        self
    }
    
    pub fn suite(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::suite(name));
        self
    }
    
    pub fn sub_suite(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::sub_suite(name));
        self
    }
    
    pub fn parent_suite(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::parent_suite(name));
        self
    }
    
    pub fn tag(mut self, name: &str) -> Self {
        self.result.labels.push(AllureLabel::tag(name));
        self
    }
    
    pub fn label(mut self, label: AllureLabel) -> Self {
        self.result.labels.push(label);
        self
    }
    
    pub fn parameter(mut self, name: &str, value: &str) -> Self {
        self.result.parameters.insert(name.to_string(), value.to_string());
        self
    }
    
    pub fn start_step(&mut self, name: &str) {
        self.current_step = Some(AllureStep {
            name: name.to_string(),
            status: AllureStatus::Unknown,
            start: current_timestamp_ms(),
            stop: 0,
            steps: vec![],
            attachments: vec![],
            status_details: None,
        });
    }
    
    pub fn end_step(&mut self, status: AllureStatus) {
        if let Some(mut step) = self.current_step.take() {
            step.stop = current_timestamp_ms();
            step.status = status;
            self.result.steps.push(step);
        }
    }
    
    pub fn add_step(&mut self, name: &str, status: AllureStatus, duration_ms: u64) {
        let start = current_timestamp_ms();
        self.result.steps.push(AllureStep {
            name: name.to_string(),
            status,
            start,
            stop: start + duration_ms,
            steps: vec![],
            attachments: vec![],
            status_details: None,
        });
    }
    
    pub fn passed(mut self) -> Self {
        self.result.status = AllureStatus::Passed;
        self.result.stop = current_timestamp_ms();
        self
    }
    
    pub fn failed(mut self, message: &str, trace: Option<&str>) -> Self {
        self.result.status = AllureStatus::Failed;
        self.result.stop = current_timestamp_ms();
        self.result.status_details = Some(AllureStatusDetails {
            known: None,
            muted: None,
            flaky: None,
            message: Some(message.to_string()),
            trace: trace.map(|s| s.to_string()),
        });
        self
    }
    
    pub fn broken(mut self, message: &str, trace: Option<&str>) -> Self {
        self.result.status = AllureStatus::Broken;
        self.result.stop = current_timestamp_ms();
        self.result.status_details = Some(AllureStatusDetails {
            known: None,
            muted: None,
            flaky: None,
            message: Some(message.to_string()),
            trace: trace.map(|s| s.to_string()),
        });
        self
    }
    
    pub fn skipped(mut self, reason: &str) -> Self {
        self.result.status = AllureStatus::Skipped;
        self.result.stop = current_timestamp_ms();
        self.result.status_details = Some(AllureStatusDetails {
            known: None,
            muted: None,
            flaky: None,
            message: Some(reason.to_string()),
            trace: None,
        });
        self
    }
    
    pub fn attach_text(&mut self, name: &str, content: &str, results_dir: &Path) -> std::io::Result<()> {
        let filename = format!("{}-attachment.txt", generate_uuid());
        let filepath = results_dir.join(&filename);
        fs::write(&filepath, content)?;
        
        self.result.attachments.push(AllureAttachment {
            name: name.to_string(),
            source: filename,
            mime_type: "text/plain".to_string(),
        });
        
        Ok(())
    }
    
    pub fn attach_json(&mut self, name: &str, content: &str, results_dir: &Path) -> std::io::Result<()> {
        let filename = format!("{}-attachment.json", generate_uuid());
        let filepath = results_dir.join(&filename);
        fs::write(&filepath, content)?;
        
        self.result.attachments.push(AllureAttachment {
            name: name.to_string(),
            source: filename,
            mime_type: "application/json".to_string(),
        });
        
        Ok(())
    }
    
    pub fn build(self) -> AllureTestResult {
        self.result
    }
    
    /// Write the test result to the allure-results directory
    pub fn write(self, results_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(results_dir)?;
        
        let result = self.build();
        let filename = format!("{}-result.json", result.uuid);
        let filepath = results_dir.join(filename);
        
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        fs::write(filepath, json)?;
        Ok(())
    }
}

/// Allure test suite for collecting multiple test results
pub struct AllureTestSuite {
    name: String,
    results_dir: PathBuf,
    results: Vec<AllureTestResult>,
}

impl AllureTestSuite {
    pub fn new(name: &str, results_dir: &Path) -> Self {
        Self {
            name: name.to_string(),
            results_dir: results_dir.to_path_buf(),
            results: vec![],
        }
    }
    
    pub fn add_result(&mut self, result: AllureTestResult) {
        self.results.push(result);
    }
    
    /// Write all results and generate JUnit XML for compatibility
    pub fn write_all(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.results_dir)?;
        
        // Write individual Allure JSON results
        for result in &self.results {
            let filename = format!("{}-result.json", result.uuid);
            let filepath = self.results_dir.join(filename);
            let json = serde_json::to_string_pretty(&result)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            fs::write(filepath, json)?;
        }
        
        // Also write JUnit XML for backward compatibility
        self.write_junit_xml()?;
        
        Ok(())
    }
    
    /// Write JUnit XML format (for Jenkins JUnit plugin compatibility)
    fn write_junit_xml(&self) -> std::io::Result<()> {
        let mut xml = String::new();
        
        let tests = self.results.len();
        let failures = self.results.iter().filter(|r| matches!(r.status, AllureStatus::Failed)).count();
        let errors = self.results.iter().filter(|r| matches!(r.status, AllureStatus::Broken)).count();
        let skipped = self.results.iter().filter(|r| matches!(r.status, AllureStatus::Skipped)).count();
        
        let total_time: f64 = self.results.iter()
            .map(|r| (r.stop - r.start) as f64 / 1000.0)
            .sum();
        
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str(&format!(
            "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">\n",
            escape_xml(&self.name), tests, failures, errors, skipped, total_time
        ));
        
        for result in &self.results {
            let time = (result.stop - result.start) as f64 / 1000.0;
            let classname = result.labels.iter()
                .find(|l| l.name == "suite")
                .map(|l| l.value.as_str())
                .unwrap_or(&self.name);
            
            xml.push_str(&format!(
                "  <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">\n",
                escape_xml(classname),
                escape_xml(&result.name),
                time
            ));
            
            // Add properties for Allure labels
            if !result.labels.is_empty() {
                xml.push_str("    <properties>\n");
                for label in &result.labels {
                    xml.push_str(&format!(
                        "      <property name=\"allure.label.{}\" value=\"{}\"/>\n",
                        escape_xml(&label.name),
                        escape_xml(&label.value)
                    ));
                }
                xml.push_str("    </properties>\n");
            }
            
            match result.status {
                AllureStatus::Failed => {
                    let message = result.status_details.as_ref()
                        .and_then(|d| d.message.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("Test failed");
                    let trace = result.status_details.as_ref()
                        .and_then(|d| d.trace.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    xml.push_str(&format!(
                        "    <failure message=\"{}\">{}</failure>\n",
                        escape_xml(message),
                        escape_xml(trace)
                    ));
                }
                AllureStatus::Broken => {
                    let message = result.status_details.as_ref()
                        .and_then(|d| d.message.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("Test error");
                    let trace = result.status_details.as_ref()
                        .and_then(|d| d.trace.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    xml.push_str(&format!(
                        "    <error message=\"{}\">{}</error>\n",
                        escape_xml(message),
                        escape_xml(trace)
                    ));
                }
                AllureStatus::Skipped => {
                    let message = result.status_details.as_ref()
                        .and_then(|d| d.message.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or("Test skipped");
                    xml.push_str(&format!(
                        "    <skipped message=\"{}\"/>\n",
                        escape_xml(message)
                    ));
                }
                _ => {}
            }
            
            // Add system-out with description
            if let Some(ref desc) = result.description {
                xml.push_str(&format!(
                    "    <system-out><![CDATA[{}]]></system-out>\n",
                    desc
                ));
            }
            
            xml.push_str("  </testcase>\n");
        }
        
        xml.push_str("</testsuite>\n");
        
        let filepath = self.results_dir.join(format!("{}-junit.xml", sanitize_filename(&self.name)));
        fs::write(filepath, xml)?;
        
        Ok(())
    }
}

/// Environment info for Allure report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureEnvironment {
    pub properties: HashMap<String, String>,
}

impl AllureEnvironment {
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
        }
    }
    
    pub fn add(&mut self, key: &str, value: &str) -> &mut Self {
        self.properties.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Write environment.properties file
    pub fn write(&self, results_dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(results_dir)?;
        
        let mut content = String::new();
        for (key, value) in &self.properties {
            content.push_str(&format!("{}={}\n", key, value));
        }
        
        fs::write(results_dir.join("environment.properties"), content)?;
        Ok(())
    }
}

impl Default for AllureEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Categories for grouping failures in Allure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllureCategory {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "matchedStatuses")]
    pub matched_statuses: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "messageRegex")]
    pub message_regex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "traceRegex")]
    pub trace_regex: Option<String>,
}

impl AllureCategory {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            matched_statuses: None,
            message_regex: None,
            trace_regex: None,
        }
    }
    
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
    
    pub fn statuses(mut self, statuses: Vec<&str>) -> Self {
        self.matched_statuses = Some(statuses.iter().map(|s| s.to_string()).collect());
        self
    }
    
    pub fn message_regex(mut self, regex: &str) -> Self {
        self.message_regex = Some(regex.to_string());
        self
    }
}

/// Write categories.json for Allure
pub fn write_categories(categories: &[AllureCategory], results_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(results_dir)?;
    
    let json = serde_json::to_string_pretty(&categories)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    
    fs::write(results_dir.join("categories.json"), json)?;
    Ok(())
}

/// Default categories for AudioCheckr tests
pub fn default_audiocheckr_categories() -> Vec<AllureCategory> {
    vec![
        AllureCategory::new("False Positives")
            .description("Clean files incorrectly flagged as defective")
            .statuses(vec!["failed"])
            .message_regex(".*FALSE POSITIVE.*"),
        AllureCategory::new("False Negatives")
            .description("Defective files incorrectly marked as clean")
            .statuses(vec!["failed"])
            .message_regex(".*FALSE NEGATIVE.*"),
        AllureCategory::new("Bit Depth Detection Issues")
            .description("Failures related to bit depth detection")
            .statuses(vec!["failed"])
            .message_regex(".*BitDepth.*|.*bit depth.*"),
        AllureCategory::new("Transcode Detection Issues")
            .description("Failures related to lossy codec detection")
            .statuses(vec!["failed"])
            .message_regex(".*Transcode.*|.*MP3.*|.*AAC.*|.*Opus.*|.*Vorbis.*"),
        AllureCategory::new("Upsampling Detection Issues")
            .description("Failures related to upsampling detection")
            .statuses(vec!["failed"])
            .message_regex(".*Upsample.*"),
        AllureCategory::new("Test Infrastructure")
            .description("Test setup or infrastructure failures")
            .statuses(vec!["broken"])
            .message_regex(".*"),
        AllureCategory::new("Skipped Tests")
            .description("Tests that were skipped")
            .statuses(vec!["skipped"])
            .message_regex(".*"),
    ]
}

// Helper functions

fn generate_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0));
    let nanos = duration.as_nanos();
    // Use wrapping_mul to avoid overflow panic in debug builds
    let random: u64 = (nanos as u64) ^ (std::process::id() as u64).wrapping_mul(0x517cc1b727220a95);
    format!("{:016x}-{:04x}-{:04x}-{:04x}-{:012x}",
        random,
        (nanos >> 64) as u16,
        (nanos >> 48) as u16,
        (nanos >> 32) as u16,
        nanos as u64 & 0xffffffffffff
    )
}

fn generate_history_id(name: &str) -> String {
    // Simple hash of the test name for history tracking
    let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
    for byte in name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV-1a prime
    }
    format!("{:016x}", hash)
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_allure_builder() {
        let result = AllureTestBuilder::new("test_example")
            .description("Example test case")
            .severity(AllureSeverity::Normal)
            .epic("AudioCheckr")
            .feature("Bit Depth Detection")
            .story("16-bit to 24-bit detection")
            .tag("regression")
            .parameter("sample_rate", "96000")
            .passed()
            .build();
        
        assert_eq!(result.name, "test_example");
        assert!(matches!(result.status, AllureStatus::Passed));
        assert!(!result.labels.is_empty());
    }
    
    #[test]
    fn test_uuid_generation() {
        let uuid1 = generate_uuid();
        let uuid2 = generate_uuid();
        
        // UUIDs should be different (with very high probability)
        assert_ne!(uuid1, uuid2);
        // UUIDs should have expected format
        assert!(uuid1.contains('-'));
    }
}
