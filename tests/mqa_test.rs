use std::path::{Path, PathBuf};
use colorful::Colorful;
use walkdir::WalkDir;
use std::process::Command;

mod test_utils;
use test_utils::{
    AllureTestBuilder, AllureTestSuite, AllureEnvironment, AllureSeverity,
    default_audiocheckr_categories, write_categories
};

#[test]
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
        panic!("Binary not found at {:?}. Run 'cargo build --release' first.", binary_path);
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
            
            let output = Command::new(&binary_path)
                .arg(path)
                .arg("--json")
                .output()
                .expect("Failed to execute command");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let is_mqa = stdout.contains("MqaEncoded");

            // Build Allure test result
            let mut allure_builder = AllureTestBuilder::new(&format!("MQA: {}", file_name))
                .full_name(&format!("mqa_test::detection::{}", sanitize_name(&file_name)))
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
                if is_mqa { "MQA Encoded" } else { "Standard FLAC" },
                if is_mqa { "PASS" } else { "FAIL" }
            );
            allure_builder = allure_builder.description(&description);
            
            // Fix: Reassign the result of attach_text back to allure_builder
            allure_builder = allure_builder.attach_text("Analysis Output", &stdout, &allure_results_dir);

            if is_mqa {
                println!("{} {} - PASSED", "[OK]".bg_green(), file_name);
                passed += 1;
                allure_builder = allure_builder.passed();
            } else {
                println!("{} {} - FAILED", "[FAIL]".bg_red(), file_name);
                failed += 1;
                allure_builder = allure_builder.failed("Failed to detect MQA encoding", Some(&stdout.to_string()));
            }
            
            allure_suite.add_result(allure_builder.build());
        }
    }

    // Write all Allure results
    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\nResults: {} passed, {} failed", passed, failed);
    println!("Allure results written to: {}", allure_results_dir.display());
    
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
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}
