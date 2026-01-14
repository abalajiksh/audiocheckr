use std::path::{Path, PathBuf};
use std::process::Command;
use serde::Serialize;
use std::fs;
use uuid::Uuid;

pub fn get_binary_path() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Prefer release binary
    let release = root.join("target/release/audiocheckr");
    if release.exists() {
        return release;
    }
    // Fallback to debug
    let debug = root.join("target/debug/audiocheckr");
    if debug.exists() {
        return debug;
    }
    // Default to release path even if missing (will fail later with clear error)
    release
}

pub fn run_audiocheckr(file_path: &str) -> Command {
    let mut cmd = Command::new(get_binary_path());
    cmd.arg(file_path);
    cmd
}

pub fn default_audiocheckr_categories() -> Vec<String> {
    vec![
        "Control_Original".to_string(),
        "MP3_128".to_string(),
        "AAC_LC".to_string(),
    ]
}

pub fn write_categories(_categories: &[String], _output_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AllureSeverity {
    Blocker,
    Critical,
    Normal,
    Minor,
    Trivial,
}

pub struct AllureEnvironment;

impl AllureEnvironment {
    pub fn new() -> Self { Self }
    pub fn add(&mut self, _key: &str, _value: &str) {}
    pub fn write(&self, _output_dir: &Path) -> std::io::Result<()> { Ok(()) }
}

pub struct AllureTestSuite {
    name: String,
    output_dir: PathBuf,
    results: Vec<AllureResult>,
}

impl AllureTestSuite {
    pub fn new(name: &str, output_dir: &Path) -> Self {
        let _ = fs::create_dir_all(output_dir);
        Self {
            name: name.to_string(),
            output_dir: output_dir.to_path_buf(),
            results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: AllureResult) {
        self.results.push(result);
    }

    pub fn write_all(&self) -> std::io::Result<()> {
        for result in &self.results {
            let file_name = format!("{}-result.json", result.uuid);
            let path = self.output_dir.join(file_name);
            if let Ok(file) = fs::File::create(path) {
                let _ = serde_json::to_writer(file, result);
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct AllureResult {
    uuid: String,
    name: String,
    status: String,
    description: Option<String>,
    start: u64,
    stop: u64,
    labels: Vec<Label>,
    status_details: Option<StatusDetails>,
}

#[derive(Serialize)]
struct Label {
    name: String,
    value: String,
}

#[derive(Serialize)]
struct StatusDetails {
    message: Option<String>,
    trace: Option<String>,
}

pub struct AllureTestBuilder {
    result: AllureResult,
}

impl AllureTestBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            result: AllureResult {
                uuid: Uuid::new_v4().to_string(),
                name: name.to_string(),
                status: "broken".to_string(),
                description: None,
                start: chrono::Utc::now().timestamp_millis() as u64,
                stop: 0,
                labels: Vec::new(),
                status_details: None,
            }
        }
    }

    pub fn full_name(self, _name: &str) -> Self { self }
    pub fn severity(self, _severity: AllureSeverity) -> Self { self }
    pub fn epic(self, _v: &str) -> Self { self }
    pub fn feature(self, _v: &str) -> Self { self }
    pub fn story(self, _v: &str) -> Self { self }
    pub fn suite(self, _v: &str) -> Self { self }
    pub fn sub_suite(self, _v: &str) -> Self { self }
    pub fn tag(self, _v: &str) -> Self { self }
    pub fn parameter(self, _k: &str, _v: &str) -> Self { self }

    pub fn description(mut self, desc: &str) -> Self {
        self.result.description = Some(desc.to_string());
        self
    }

    pub fn attach_text(self, _name: &str, _content: &str, _dir: &Path) -> Self { self }

    pub fn passed(mut self) -> Self {
        self.result.status = "passed".to_string();
        self
    }

    pub fn failed(mut self, message: &str, trace: Option<&String>) -> Self {
        self.result.status = "failed".to_string();
        self.result.status_details = Some(StatusDetails {
            message: Some(message.to_string()),
            trace: trace.cloned(),
        });
        self
    }

    pub fn build(mut self) -> AllureResult {
        self.result.stop = chrono::Utc::now().timestamp_millis() as u64;
        self.result
    }
}
