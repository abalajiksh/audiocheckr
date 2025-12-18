// tests/qualification_test.rs
//
// QUALIFICATION Test Suite - Quick sanity check for CI/CD
// Uses TestFiles directory for basic validation
//
// Now with Allure reporting support and stricter defect type validation
//
// v4: Fixed validation logic to check SPECIFIC defect types
// v5: STRICTER validation - extra defects cause test failure
//
// Purpose: Quick sanity check with synthetic test files
// - Tests representative samples from Control and major defect categories  
// - Focuses on high-confidence detection scenarios
// - Parallel execution (4 threads) for faster CI/CD

mod test_utils;

use std::env;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::{HashMap, HashSet};

use test_utils::{
    AllureTestBuilder, AllureTestSuite, AllureEnvironment, AllureSeverity,
    write_categories, default_audiocheckr_categories,
};

#[derive(Clone)]
struct TestCase {
    file_path: String,
    should_pass: bool,
    expected_defects: Vec<String>,
    category: String,
    description: String,
}

/// Result of validating a test
#[derive(Debug, Clone, Copy, PartialEq)]
enum ValidationResult {
    /// Test passed - correct detection
    Pass,
    /// Test passed with warning - partial match of expected defects (some missing, no extras)
    PassWithWarning,
    /// Test failed - false positive (clean file flagged as defective)
    FalsePositive,
    /// Test failed - false negative (defective file marked as clean)
    FalseNegative,
    /// Test failed - wrong defect type detected (none of expected found)
    WrongDefectType,
    /// Test failed - extra defects detected beyond expected
    ExtraDefects,
}

#[derive(Debug)]
struct TestResult {
    passed: bool,
    expected: bool,
    defects_found: Vec<String>,
    expected_defects: Vec<String>,
    validation_result: ValidationResult,
    extra_defects: Vec<String>,
    missing_defects: Vec<String>,
    file: String,
    stdout: String,
    #[allow(dead_code)]
    duration_ms: u64,
}

/// Main qualification test
#[test]
fn test_qualification_suite() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestFiles");
    let allure_results_dir = project_root.join("target").join("allure-results");

    assert!(
        test_base.exists(),
        "TestFiles directory not found at: {}. \
         Download TestFiles.zip from MinIO for qualification tests.",
        test_base.display()
    );

    println!("\n{}", "=".repeat(60));
    println!("QUALIFICATION TEST SUITE (Parallel Execution)");
    println!("Using: {}", test_base.display());
    println!("Allure results: {}", allure_results_dir.display());
    println!("{}\n", "=".repeat(60));

    // Setup Allure environment
    setup_allure_environment(&allure_results_dir, "Qualification");
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    let test_cases = define_qualification_tests(&test_base);
    let total = test_cases.len();

    println!("Running {} qualification tests in parallel...\n", total);

    // Run tests in parallel with 4 threads
    let results = run_tests_parallel(&binary_path, test_cases.clone(), 4);

    // Create Allure test suite
    let mut allure_suite = AllureTestSuite::new("Qualification Tests", &allure_results_dir);

    let mut passed = 0;
    let mut passed_with_warning = 0;
    let mut failed = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;
    let mut wrong_defect_type = 0;
    let mut extra_defects_count = 0;

    for (idx, result) in results.iter().enumerate() {
        let test_case = &test_cases[idx];

        // Build Allure test result
        let severity = match test_case.category.as_str() {
            "CleanOrigin" | "CleanTranscoded" => AllureSeverity::Critical,
            cat if cat.contains("Upscale") => AllureSeverity::Critical,
            _ => AllureSeverity::Normal,
        };

        let expected_str = if result.expected {
            "CLEAN (should pass)".to_string()
        } else {
            format!("DEFECTIVE with {:?}", result.expected_defects)
        };

        let mut allure_builder = AllureTestBuilder::new(&test_case.description)
            .full_name(&format!("qualification_test::{}", sanitize_name(&test_case.description)))
            .severity(severity)
            .epic("AudioCheckr")
            .feature("Qualification")
            .story(&test_case.category)
            .suite("Qualification")
            .sub_suite(&test_case.category)
            .tag("qualification")
            .parameter("file", &result.file)
            .parameter("expected_pass", &result.expected.to_string())
            .parameter("defects_found", &format!("{:?}", result.defects_found))
            .parameter("expected_defects", &format!("{:?}", result.expected_defects))
            .parameter("validation_result", &format!("{:?}", result.validation_result));

        let description = format!(
            "**File:** `{}`\n\n\
            **Category:** {}\n\n\
            **Expected:** {}\n\n\
            **Actual:** {}\n\n\
            **Defects Found:** {:?}\n\n\
            **Expected Defects:** {:?}\n\n\
            **Missing Defects:** {:?}\n\n\
            **Extra Defects:** {:?}\n\n\
            **Validation Result:** {:?}",
            result.file,
            test_case.category,
            expected_str,
            if result.passed { "CLEAN" } else { "DEFECTIVE" },
            result.defects_found,
            result.expected_defects,
            result.missing_defects,
            result.extra_defects,
            result.validation_result
        );
        allure_builder = allure_builder.description(&description);

        // Attach stdout
        let _ = allure_builder.attach_text("Analysis Output", &result.stdout, &allure_results_dir);

        match result.validation_result {
            ValidationResult::Pass => {
                passed += 1;
                println!("[{:2}/{}] ✓ PASS: {}", idx + 1, total, test_case.description);
                allure_builder = allure_builder.passed();
            }
            ValidationResult::PassWithWarning => {
                passed_with_warning += 1;
                passed += 1;
                println!(
                    "[{:2}/{}] ⚠ PASS (partial): {} - Missing {:?}",
                    idx + 1, total, test_case.description, result.missing_defects
                );
                allure_builder = allure_builder.passed();
            }
            ValidationResult::FalsePositive => {
                failed += 1;
                false_positives += 1;
                let message = format!("FALSE POSITIVE: Expected CLEAN but detected defects: {:?}",
                    result.defects_found);
                println!(
                    "[{:2}/{}] ✗ FALSE POSITIVE: {}\n        Expected CLEAN but detected defects: {:?}",
                    idx + 1, total, test_case.description, result.defects_found
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::FalseNegative => {
                failed += 1;
                false_negatives += 1;
                let message = format!("FALSE NEGATIVE: Expected defects {:?} but got CLEAN",
                    result.expected_defects);
                println!(
                    "[{:2}/{}] ✗ FALSE NEGATIVE: {}\n        Expected defects but got CLEAN",
                    idx + 1, total, test_case.description
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::WrongDefectType => {
                failed += 1;
                wrong_defect_type += 1;
                let message = format!(
                    "WRONG DEFECT TYPE: Expected {:?} but detected {:?}",
                    result.expected_defects, result.defects_found
                );
                println!(
                    "[{:2}/{}] ✗ WRONG DEFECT: {} - Expected {:?}, Got {:?}",
                    idx + 1, total, test_case.description, result.expected_defects, result.defects_found
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::ExtraDefects => {
                failed += 1;
                extra_defects_count += 1;
                let message = format!(
                    "EXTRA DEFECTS: Expected {:?} but also detected extra: {:?}",
                    result.expected_defects, result.extra_defects
                );
                println!(
                    "[{:2}/{}] ✗ EXTRA DEFECTS: {} - Expected {:?}, Extra {:?}",
                    idx + 1, total, test_case.description, result.expected_defects, result.extra_defects
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
        }

        allure_suite.add_result(allure_builder.build());
    }

    // Write Allure results
    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\n{}", "=".repeat(60));
    println!("QUALIFICATION RESULTS");
    println!("{}", "=".repeat(60));
    println!("Total Tests:     {}", total);
    println!("Passed:          {} ({:.1}%)", passed, (passed as f32 / total as f32) * 100.0);
    println!("  - Clean passes: {}", passed - passed_with_warning);
    println!("  - Partial match (missing some expected): {}", passed_with_warning);
    println!("Failed:          {}", failed);
    println!("  - False Positives: {} (clean files marked as defective)", false_positives);
    println!("  - False Negatives: {} (defective files marked as clean)", false_negatives);
    println!("  - Wrong Defect Type: {}", wrong_defect_type);
    println!("  - Extra Defects: {}", extra_defects_count);
    println!("{}", "=".repeat(60));

    println!("\nAllure results written to: {}", allure_results_dir.display());

    assert_eq!(
        failed, 0,
        "Qualification failed: {} test(s) did not pass",
        failed
    );
}

fn setup_allure_environment(results_dir: &Path, suite_name: &str) {
    let mut env = AllureEnvironment::new();

    env.add("OS", std::env::consts::OS);
    env.add("Architecture", std::env::consts::ARCH);
    env.add("Rust Version", env!("CARGO_PKG_VERSION"));
    env.add("Test Suite", suite_name);

    if let Ok(hostname) = std::env::var("HOSTNAME") {
        env.add("Host", &hostname);
    }
    if let Ok(build_number) = std::env::var("BUILD_NUMBER") {
        env.add("Jenkins Build", &build_number);
    }
    if let Ok(git_commit) = std::env::var("GIT_COMMIT") {
        env.add("Git Commit", &git_commit);
    }

    let _ = env.write(results_dir);
}

fn define_qualification_tests(base: &Path) -> Vec<TestCase> {
    let mut tests = Vec::new();

    // CleanOrigin - should pass (genuine high-res)
    tests.push(TestCase {
        file_path: base.join("CleanOrigin/input96.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        category: "CleanOrigin".to_string(),
        description: "CleanOrigin: 96kHz 24-bit original master".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("CleanOrigin/input192.flac").to_string_lossy().to_string(),
        should_pass: true,  // This is actually a 16-bit source in 24-bit container
        expected_defects: vec![],
        category: "CleanOrigin".to_string(),
        description: "CleanOrigin: 192kHz (16-bit source in 24-bit container)".to_string(),
    });

    // CleanTranscoded - honest 16-bit transcodes should pass
    tests.push(TestCase {
        file_path: base.join("CleanTranscoded/input96_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        category: "CleanTranscoded".to_string(),
        description: "CleanTranscoded: 96kHz honest 16-bit transcode".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("CleanTranscoded/input192_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        category: "CleanTranscoded".to_string(),
        description: "CleanTranscoded: 192kHz honest 16-bit transcode".to_string(),
    });

    // Resample96 - downsamples should pass, upsamples should fail
    for rate in &["44", "48", "88"] {
        tests.push(TestCase {
            file_path: base.join(format!("Resample96/input96_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: true,  // Downsampling is legitimate
            expected_defects: vec![],
            category: "Resample96".to_string(),
            description: format!("Resample96: 96→{}kHz downsampled (genuine)", rate),
        });
    }

    // Resample192 - all from 16-bit source, should fail
    for rate in &["44", "48", "88", "96", "176"] {
        tests.push(TestCase {
            file_path: base.join(format!("Resample192/input192_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: false,
            expected_defects: vec!["Upsampled".to_string()],
            category: "Resample192".to_string(),
            description: format!("Resample192: 192→{}kHz (16-bit source)", rate),
        });
    }

    // Upscale16 - bit depth padding, should fail
    tests.push(TestCase {
        file_path: base.join("Upscale16/input96_16to24.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        category: "Upscale16".to_string(),
        description: "Upscale16: 96kHz 16→24-bit upscaled (fake depth)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("Upscale16/input192_16to24.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        category: "Upscale16".to_string(),
        description: "Upscale16: 192kHz 16→24-bit upscaled (fake depth)".to_string(),
    });

    // Upscaled - lossy transcodes, should fail with transcode defects
    tests.push(TestCase {
        file_path: base.join("Upscaled/input96_mp3.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["Mp3Transcode".to_string()],
        category: "Upscaled".to_string(),
        description: "Upscaled: 96kHz from MP3 (lossy artifacts)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("Upscaled/input192_mp3.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["Mp3Transcode".to_string()],
        category: "Upscaled".to_string(),
        description: "Upscaled: 192kHz from MP3 (lossy artifacts)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("Upscaled/input192_m4a.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["AacTranscode".to_string()],
        category: "Upscaled".to_string(),
        description: "Upscaled: 192kHz from M4A (lossy artifacts)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("Upscaled/input192_opus.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["OpusTranscode".to_string()],
        category: "Upscaled".to_string(),
        description: "Upscaled: 192kHz from OPUS (lossy artifacts)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("Upscaled/input192_ogg.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["OggVorbisTranscode".to_string()],
        category: "Upscaled".to_string(),
        description: "Upscaled: 192kHz from OGG (lossy artifacts)".to_string(),
    });

    // MasterScript samples
    tests.push(TestCase {
        file_path: base.join("MasterScript/test96/test96_original.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        category: "MasterScript".to_string(),
        description: "MasterScript: test96 original (reference, clean)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("MasterScript/test192/test192_original.flac").to_string_lossy().to_string(),
        should_pass: false,  // 16-bit source in 24-bit container
        expected_defects: vec!["BitDepthMismatch".to_string()],
        category: "MasterScript".to_string(),
        description: "MasterScript: test192 original (16-bit source)".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("MasterScript/test96/test96_16to24.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        category: "MasterScript".to_string(),
        description: "MasterScript: test96 16-bit upscaled to 24-bit".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("MasterScript/test192/test192_mp3_320k.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["Mp3Transcode".to_string()],
        category: "MasterScript".to_string(),
        description: "MasterScript: test192 MP3 320k upscaled".to_string(),
    });

    tests.push(TestCase {
        file_path: base.join("MasterScript/test192/test192_opus_128k.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["OpusTranscode".to_string()],
        category: "MasterScript".to_string(),
        description: "MasterScript: test192 Opus 128k upscaled".to_string(),
    });

    tests
}

fn run_tests_parallel(binary: &Path, test_cases: Vec<TestCase>, num_threads: usize) -> Vec<TestResult> {
    let binary = binary.to_path_buf();
    let test_cases = Arc::new(test_cases);
    let results = Arc::new(Mutex::new(Vec::new()));
    let index = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let binary = binary.clone();
        let test_cases = Arc::clone(&test_cases);
        let results = Arc::clone(&results);
        let index = Arc::clone(&index);

        let handle = thread::spawn(move || {
            loop {
                let current_idx = {
                    let mut idx = index.lock().unwrap();
                    if *idx >= test_cases.len() {
                        return;
                    }
                    let current = *idx;
                    *idx += 1;
                    current
                };

                let test_case = &test_cases[current_idx];
                let result = run_single_test(&binary, test_case);

                let mut results_guard = results.lock().unwrap();
                results_guard.push((current_idx, result));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let mut results_vec: Vec<(usize, TestResult)> = Arc::try_unwrap(results)
        .expect("Arc still has multiple owners")
        .into_inner()
        .expect("Mutex poisoned");
    results_vec.sort_by_key(|(idx, _)| *idx);
    results_vec.into_iter().map(|(_, result)| result).collect()
}

fn parse_defects_from_output(stdout: &str) -> Vec<String> {
    let mut defects_found = Vec::new();
    let stdout_lower = stdout.to_lowercase();

    if (stdout_lower.contains("mp3") && stdout_lower.contains("transcode"))
        || stdout_lower.contains("mp3transcode")
    {
        defects_found.push("Mp3Transcode".to_string());
    }
    if (stdout_lower.contains("aac") && stdout_lower.contains("transcode"))
        || stdout_lower.contains("aactranscode")
    {
        defects_found.push("AacTranscode".to_string());
    }
    if (stdout_lower.contains("opus") && stdout_lower.contains("transcode"))
        || stdout_lower.contains("opustranscode")
    {
        defects_found.push("OpusTranscode".to_string());
    }
    if ((stdout_lower.contains("vorbis") || stdout_lower.contains("ogg"))
        && stdout_lower.contains("transcode"))
        || stdout_lower.contains("oggvorbistranscode")
    {
        defects_found.push("OggVorbisTranscode".to_string());
    }

    if stdout_lower.contains("bit depth mismatch")
        || stdout_lower.contains("bitdepthmismatch")
        || (stdout_lower.contains("bit depth") && stdout_lower.contains("mismatch"))
    {
        defects_found.push("BitDepthMismatch".to_string());
    }

    if stdout_lower.contains("upsampled")
        || (stdout_lower.contains("upsample") && !stdout_lower.contains("not upsampled"))
    {
        defects_found.push("Upsampled".to_string());
    }

    if stdout_lower.contains("spectral artifact") {
        defects_found.push("SpectralArtifacts".to_string());
    }

    defects_found
}

/// Validate test results with STRICT defect type matching
fn validate_test_result(
    is_clean: bool,
    should_pass: bool,
    expected_defects: &[String],
    defects_found: &[String],
) -> (ValidationResult, Vec<String>, Vec<String>) {
    let expected_set: HashSet<&String> = expected_defects.iter().collect();
    let found_set: HashSet<&String> = defects_found.iter().collect();

    let missing: Vec<String> = expected_defects.iter()
        .filter(|d| !found_set.contains(d))
        .cloned()
        .collect();

    let extra: Vec<String> = defects_found.iter()
        .filter(|d| !expected_set.contains(d))
        .cloned()
        .collect();

    // Case 1: Expected CLEAN
    if should_pass {
        if is_clean {
            return (ValidationResult::Pass, missing, extra);
        } else {
            return (ValidationResult::FalsePositive, missing, extra);
        }
    }

    // Case 2: Expected DEFECTIVE
    if is_clean {
        return (ValidationResult::FalseNegative, missing, extra);
    }

    // File is defective as expected
    if expected_defects.is_empty() {
        return (ValidationResult::Pass, missing, extra);
    }

    let any_expected_found = expected_defects.iter().any(|d| found_set.contains(d));

    if !any_expected_found {
        return (ValidationResult::WrongDefectType, missing, extra);
    }

    // At least one expected defect found - check for extras
    if !extra.is_empty() {
        return (ValidationResult::ExtraDefects, missing, extra);
    }

    if missing.is_empty() {
        return (ValidationResult::Pass, missing, extra);
    } else {
        return (ValidationResult::PassWithWarning, missing, extra);
    }
}

fn run_single_test(binary: &Path, test_case: &TestCase) -> TestResult {
    let start = std::time::Instant::now();

    let file_path = PathBuf::from(&test_case.file_path);
    if !file_path.exists() {
        return TestResult {
            passed: false,
            expected: test_case.should_pass,
            defects_found: vec![],
            expected_defects: test_case.expected_defects.clone(),
            validation_result: ValidationResult::FalseNegative,
            extra_defects: vec![],
            missing_defects: test_case.expected_defects.clone(),
            file: test_case.file_path.clone(),
            stdout: format!("File not found: {}", test_case.file_path),
            duration_ms: 0,
        };
    }

    let output = Command::new(binary)
        .arg("--input")
        .arg(&test_case.file_path)
        .arg("--bit-depth")
        .arg("24")
        .arg("--check-upsampling")
        .output()
        .expect("Failed to execute binary");

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let defects_found = parse_defects_from_output(&stdout);
    let is_clean = defects_found.is_empty();

    let (validation_result, missing_defects, extra_defects) = validate_test_result(
        is_clean,
        test_case.should_pass,
        &test_case.expected_defects,
        &defects_found,
    );

    TestResult {
        passed: is_clean,
        expected: test_case.should_pass,
        defects_found,
        expected_defects: test_case.expected_defects.clone(),
        validation_result,
        extra_defects,
        missing_defects,
        file: test_case.file_path.clone(),
        stdout,
        duration_ms,
    }
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn get_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");

    let release_path = path.join("release").join("audiocheckr");
    let debug_path = path.join("debug").join("audiocheckr");

    #[cfg(windows)]
    {
        let release_path_exe = release_path.with_extension("exe");
        let debug_path_exe = debug_path.with_extension("exe");
        if release_path_exe.exists() {
            return release_path_exe;
        } else if debug_path_exe.exists() {
            return debug_path_exe;
        }
    }

    #[cfg(unix)]
    {
        if release_path.exists() {
            return release_path;
        } else if debug_path.exists() {
            return debug_path;
        }
    }

    panic!("Binary not found. Run: cargo build --release");
}

#[test]
fn test_binary_exists() {
    let binary_path = get_binary_path();
    assert!(binary_path.exists(), "Binary not found at {:?}", binary_path);
}

#[test]
fn test_help_output() {
    let binary_path = get_binary_path();
    let output = Command::new(&binary_path)
        .arg("--help")
        .output()
        .expect("Failed to run --help");

    assert!(output.status.success(), "Help command failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("audio") || stdout.contains("Audio"),
        "Help output should mention audio"
    );
}
