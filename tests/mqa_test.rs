// tests/mqa_test.rs
//
// v3: Added --mqa flag to actually enable MQA detection.
// v2: Switched to structured JSON output parsing
//   - `--json` flag replaced with `--format both`
//   - text output → stderr (attached to Allure)
//   - JSON output → stdout (parsed to find MqaEncoded in detections)
//   - MqaEncoded may carry severity "info" — NOT filtered out

use colorful::Colorful;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

mod test_utils;

use test_utils::{
    default_audiocheckr_categories, write_categories, AllureEnvironment, AllureSeverity,
    AllureTestBuilder, AllureTestSuite,
};

// ── JSON deserialization structs ─────────────────────────────────────────────
// Mirrors the enriched JSON written by audiocheckr to stdout in `--format both`.
// Only the fields we need are declared; serde ignores the rest.

#[derive(Deserialize)]
struct JsonReport {
    detections: Vec<JsonDetection>,
}

#[derive(Deserialize)]
struct JsonDetection {
    // Externally-tagged serde enum, e.g. { "MqaEncoded": { ... } }
    defect_type: serde_json::Value,

    // "critical" | "high" | "medium" | "low" | "info"
    // NOTE: MqaEncoded may carry severity "info" — intentionally NOT filtered here.
    #[allow(dead_code)]
    severity: String,
}

/// Returns true if the JSON stdout from `--format both` contains a MqaEncoded detection.
fn parse_is_mqa_from_json(json_str: &str) -> bool {
    let report: JsonReport = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: failed to parse JSON output: {}", e);
            return false;
        }
    };

    report
        .detections
        .iter()
        .any(|det| det.defect_type.get("MqaEncoded").is_some())
}

#[test]
// #[ignore]  <-- Kept commented out so the test runs
fn test_mqa_detection() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mqa_dir = project_root.join("MQA");
    let allure_results_dir = project_root.join("target").join("allure-results");

    if !mqa_dir.exists() {
        println!("MQA directory not found, skipping tests");
        return;
    }

    let binary_path = test_utils::get_binary_path();
    if !binary_path.exists() {
        panic!(
            "Binary not found at {:?}. Run 'cargo build --release' first.",
            binary_path
        );
    }

    // Setup Allure environment
    setup_allure_environment(&allure_results_dir, "MQA Detection");
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    let mut passed = 0;
    let mut failed = 0;

    // Create Allure test suite
    let mut allure_suite = AllureTestSuite::new("MQA Detection Tests", &allure_results_dir);

    for entry in WalkDir::new(&mqa_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "flac") {
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();

            // Use `--format both` + `--mqa`:
            //   JSON  → stdout  (parsed by parse_is_mqa_from_json)
            //   text  → stderr  (attached to Allure as human-readable evidence)
            let output = Command::new(&binary_path)
                .arg(path)
                .arg("--mqa") // <--- ADDED: Enable MQA detection explicitly
                .arg("--format")
                .arg("both")
                .output()
                .expect("Failed to execute command");

            let json_stdout = String::from_utf8_lossy(&output.stdout);
            let text_stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let is_mqa = parse_is_mqa_from_json(&json_stdout);

            // Build Allure test result
            let mut allure_builder = AllureTestBuilder::new(&format!("MQA: {}", file_name))
                .full_name(&format!(
                    "mqa_test::detection::{}",
                    sanitize_name(&file_name)
                ))
                .severity(AllureSeverity::Critical)
                .epic("AudioCheckr")
                .feature("MQA Detection")
                .story("Detect MQA Encoding")
                .suite("MQA Detection")
                .tag("mqa")
                .parameter("file", &file_name);

            let description = format!(
                "**File:** `{}`\n\n\
                **Expected:** MQA Encoded\n\n\
                **Actual:** {}\n\n\
                **Result:** {}",
                file_name,
                if is_mqa {
                    "MQA Encoded"
                } else {
                    "Standard FLAC"
                },
                if is_mqa { "PASS" } else { "FAIL" }
            );
            allure_builder = allure_builder.description(&description);

            // Attach human-readable text output (stderr in v2) as Allure evidence
            allure_builder =
                allure_builder.attach_text("Analysis Output", &text_stderr, &allure_results_dir);

            if is_mqa {
                println!("{} {} - PASSED", "[OK]".bg_green(), file_name);
                passed += 1;
                allure_builder = allure_builder.passed();
            } else {
                println!("{} {} - FAILED", "[FAIL]".bg_red(), file_name);
                failed += 1;
                allure_builder =
                    allure_builder.failed("Failed to detect MQA encoding", Some(&text_stderr));
            }

            allure_suite.add_result(allure_builder.build());
        }
    }

    // Write all Allure results
    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\nResults: {} passed, {} failed", passed, failed);
    println!(
        "Allure results written to: {}",
        allure_results_dir.display()
    );

    assert_eq!(failed, 0, "Some MQA files were not detected");
}

fn setup_allure_environment(results_dir: &Path, suite_name: &str) {
    let mut env = AllureEnvironment::new();
    env.add("OS", std::env::consts::OS);
    env.add("Architecture", std::env::consts::ARCH);
    env.add("Rust Version", env!("CARGO_PKG_VERSION"));
    env.add("Test Suite", suite_name);
    let _ = env.write(results_dir);
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
