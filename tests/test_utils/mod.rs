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

pub fn run_audiocheckr<P: AsRef<std::ffi::OsStr>>(file_path: P) -> Command {
    let mut cmd = Command::new(get_binary_path());
    cmd.arg(file_path);
    cmd
}

pub fn run_json_analysis<P: AsRef<std::ffi::OsStr>>(file_path: P) -> std::process::Output {
    run_audiocheckr(file_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to execute with json format")
}

pub fn run_verbose_analysis<P: AsRef<std::ffi::OsStr>>(file_path: P) -> std::process::Output {
    run_audiocheckr(file_path)
        .arg("--verbose")
        .output()
        .expect("Failed to execute with verbose flag")
}

pub fn default_audiocheckr_categories() -> Vec<String> {
    vec![
        "Control_Original".to_string(),
        "MP3_128".to_string(),
        "AAC_LC".to_string(),
    ]
}

pub fn write_categories(categories: &[String], output_dir: &Path) -> std::io::Result<()> {
    let _ = fs::create_dir_all(output_dir);
    let categories_file = output_dir.join("categories.json");
    if let Ok(file) = fs::File::create(categories_file) {
        let _ = serde_json::to_writer(file, categories);
    }
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

pub struct AllureEnvironment {
    properties: std::collections::HashMap<String, String>,
}

impl AllureEnvironment {
    pub fn new() -> Self { 
        Self { properties: std::collections::HashMap::new() } 
    }
    
    pub fn add(&mut self, key: &str, value: &str) {
        self.properties.insert(key.to_string(), value.to_string());
    }
    
    pub fn write(&self, output_dir: &Path) -> std::io::Result<()> {
        let _ = fs::create_dir_all(output_dir);
        let path = output_dir.join("environment.properties");
        let mut content = String::new();
        for (key, value) in &self.properties {
            content.push_str(&format!("{}={}\n", key, value));
        }
        fs::write(path, content)
    }
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
    attachments: Vec<Attachment>,
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

#[derive(Serialize)]
struct Attachment {
    name: String,
    source: String,
    #[serde(rename = "type")]
    mime_type: String,
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
                attachments: Vec::new(),
            }
        }
    }

    pub fn full_name(mut self, name: &str) -> Self { 
        self.result.labels.push(Label { name: "fullName".to_string(), value: name.to_string() });
        self 
    }

    pub fn severity(mut self, severity: AllureSeverity) -> Self { 
        self.result.labels.push(Label { name: "severity".to_string(), value: format!("{:?}", severity).to_lowercase() });
        self 
    }

    pub fn epic(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "epic".to_string(), value: v.to_string() });
        self 
    }

    pub fn feature(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "feature".to_string(), value: v.to_string() });
        self 
    }

    pub fn story(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "story".to_string(), value: v.to_string() });
        self 
    }

    pub fn suite(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "suite".to_string(), value: v.to_string() });
        self 
    }

    pub fn sub_suite(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "subSuite".to_string(), value: v.to_string() });
        self 
    }

    pub fn tag(mut self, v: &str) -> Self { 
        self.result.labels.push(Label { name: "tag".to_string(), value: v.to_string() });
        self 
    }

    pub fn parameter(mut self, k: &str, v: &str) -> Self { 
        self.result.labels.push(Label { name: k.to_string(), value: v.to_string() }); // Parameter as label for now, or use specific param struct if needed
        self 
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.result.description = Some(desc.to_string());
        self
    }

    pub fn attach_text(mut self, name: &str, content: &str, dir: &Path) -> Self { 
        let uuid = Uuid::new_v4().to_string();
        let file_name = format!("{}-attachment.txt", uuid);
        let path = dir.join(&file_name);
        if fs::write(&path, content).is_ok() {
            self.result.attachments.push(Attachment {
                name: name.to_string(),
                source: file_name,
                mime_type: "text/plain".to_string(),
            });
        }
        self 
    }

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
