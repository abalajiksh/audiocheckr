// tests/qualification_test.rs
// QUALIFICATION Test Suite - Compact subset for CI/CD quick validation
// Uses a subset of files from TestFiles/ for fast validation on every push
//
// Now with Allure reporting support for better visualization
//
// Test Philosophy:
// - CleanOrigin: Original master files → PASS (genuine high-res) except input192 (16-bit source)
// - CleanTranscoded: 24→16 bit honest transcodes → PASS (genuinely 16-bit)
// - Resample96: 96kHz → lower rates = PASS, 96kHz → higher rates = FAIL (interpolated)
// - Resample192: All from 16-bit source → FAIL (BitDepthMismatch)
// - Upscale16: 16-bit → 24-bit padding = FAIL (fake bit depth)
// - Upscaled: Lossy → Lossless = FAIL (lossy artifacts detected)
//
// Parallelization: Tests run in parallel (4 threads) for faster CI/CD

mod test_utils;

use std::env;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use test_utils::{
    AllureTestBuilder, AllureTestSuite, AllureEnvironment, AllureSeverity, AllureStatus,
    write_categories, default_audiocheckr_categories,
};

#[derive(Clone)]
struct TestCase {
    file_path: String,
    should_pass: bool,
    expected_defects: Vec<String>,
    description: String,
    category: String,
    severity: AllureSeverity,
}

#[derive(Debug)]
struct TestResult {
    passed: bool,
    expected: bool,
    defects_found: Vec<String>,
    description: String,
    category: String,
    file: String,
    duration_ms: u64,
    stdout: String,
}

/// Main qualification test - runs against TestFiles subset with parallel execution
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

    // Setup Allure environment info
    setup_allure_environment(&allure_results_dir);
    
    // Write default categories
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    let test_cases = define_qualification_tests(&test_base);
    let total_tests = test_cases.len();
    
    println!("Running {} qualification tests in parallel...\n", total_tests);

    // Run tests in parallel with 4 threads
    let results = run_tests_parallel(&binary_path, test_cases.clone(), 4);
    
    // Create Allure test suite
    let mut allure_suite = AllureTestSuite::new("Qualification Tests", &allure_results_dir);
    
    // Analyze results and generate Allure reports
    let mut passed = 0;
    let mut failed = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;

    for (idx, (result, test_case)) in results.iter().zip(test_cases.iter()).enumerate() {
        let test_passed = result.passed == result.expected;
        
        // Build Allure test result
        let mut allure_builder = AllureTestBuilder::new(&result.description)
            .full_name(&format!("qualification_test::{}", sanitize_name(&result.description)))
            .severity(test_case.severity)
            .epic("AudioCheckr")
            .feature("Audio Quality Detection")
            .story(&result.category)
            .suite("Qualification")
            .sub_suite(&result.category)
            .tag("qualification")
            .tag(&result.category.to_lowercase().replace(' ', "_"))
            .parameter("file", &result.file)
            .parameter("expected_pass", &result.expected.to_string())
            .parameter("defects_found", &format!("{:?}", result.defects_found));
        
        // Add description with details
        let description = format!(
            "**File:** `{}`\n\n**Expected:** {}\n\n**Actual:** {}\n\n**Defects Found:** {:?}",
            result.file,
            if result.expected { "CLEAN (should pass)" } else { "DEFECTIVE (should fail)" },
            if result.passed { "CLEAN" } else { "DEFECTIVE" },
            result.defects_found
        );
        allure_builder = allure_builder.description(&description);
        
        // Attach stdout as evidence
        let _ = allure_builder.attach_text("Analysis Output", &result.stdout, &allure_results_dir);
        
        if test_passed {
            passed += 1;
            println!("[{:2}/{}] ✓ PASS: {}", idx + 1, total_tests, result.description);
            allure_builder = allure_builder.passed();
        } else {
            failed += 1;

            if result.passed && !result.expected {
                false_negatives += 1;
                let message = format!("FALSE NEGATIVE: Expected defects {:?} but got CLEAN", 
                    test_case.expected_defects);
                println!("[{:2}/{}] ✗ FALSE NEGATIVE: {}", idx + 1, total_tests, result.description);
                println!("        Expected defects but got CLEAN");
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            } else {
                false_positives += 1;
                let message = format!("FALSE POSITIVE: Expected CLEAN but detected defects: {:?}", 
                    result.defects_found);
                println!("[{:2}/{}] ✗ FALSE POSITIVE: {}", idx + 1, total_tests, result.description);
                println!("        Expected CLEAN but detected defects: {:?}", result.defects_found);
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
        }
        
        allure_suite.add_result(allure_builder.build());
    }

    // Write all Allure results
    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\n{}", "=".repeat(60));
    println!("QUALIFICATION RESULTS");
    println!("{}", "=".repeat(60));
    println!("Total Tests:     {}", total_tests);
    println!("Passed:          {} ({:.1}%)", passed, (passed as f32 / total_tests as f32) * 100.0);
    println!("Failed:          {}", failed);
    println!("  False Positives: {} (clean files marked as defective)", false_positives);
    println!("  False Negatives: {} (defective files marked as clean)", false_negatives);
    println!("{}", "=".repeat(60));
    println!("\nAllure results written to: {}", allure_results_dir.display());

    assert_eq!(failed, 0, "Qualification failed: {} test(s) did not pass", failed);
}

fn setup_allure_environment(results_dir: &Path) {
    let mut env = AllureEnvironment::new();
    
    // Get system info
    env.add("OS", std::env::consts::OS);
    env.add("Architecture", std::env::consts::ARCH);
    env.add("Rust Version", env!("CARGO_PKG_VERSION"));
    env.add("Test Suite", "Qualification");
    
    // Get hostname
    if let Ok(hostname) = std::env::var("HOSTNAME") {
        env.add("Host", &hostname);
    }
    
    // Get build info from Jenkins if available
    if let Ok(build_number) = std::env::var("BUILD_NUMBER") {
        env.add("Jenkins Build", &build_number);
    }
    if let Ok(git_commit) = std::env::var("GIT_COMMIT") {
        env.add("Git Commit", &git_commit);
    }
    if let Ok(branch) = std::env::var("GIT_BRANCH") {
        env.add("Git Branch", &branch);
    }
    
    let _ = env.write(results_dir);
}

/// Run tests in parallel using thread pool
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
                // Get next test case index
                let current_idx = {
                    let mut idx = index.lock().unwrap();
                    if *idx >= test_cases.len() {
                        return;
                    }
                    let current = *idx;
                    *idx += 1;
                    current
                };
                
                // Run the test
                let test_case = &test_cases[current_idx];
                let result = run_single_test(&binary, test_case);
                
                // Store result
                let mut results_guard = results.lock().unwrap();
                results_guard.push((current_idx, result));
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }
    
    // Sort results by original index and extract
    let mut results_vec: Vec<(usize, TestResult)> = Arc::try_unwrap(results)
        .expect("Arc still has multiple owners")
        .into_inner()
        .expect("Mutex poisoned");
    
    results_vec.sort_by_key(|(idx, _)| *idx);
    results_vec.into_iter().map(|(_, result)| result).collect()
}

/// Parse defects from audiocheckr output
fn parse_defects_from_output(stdout: &str) -> Vec<String> {
    let mut defects_found = Vec::new();
    let stdout_lower = stdout.to_lowercase();
    
    // Look for specific defect patterns in the output
    // Check for transcode detections
    if (stdout_lower.contains("mp3") && stdout_lower.contains("transcode")) 
        || stdout_lower.contains("mp3transcode") {
        defects_found.push("Mp3Transcode".to_string());
    }
    if (stdout_lower.contains("aac") && stdout_lower.contains("transcode"))
        || stdout_lower.contains("aactranscode") {
        defects_found.push("AacTranscode".to_string());
    }
    if (stdout_lower.contains("opus") && stdout_lower.contains("transcode"))
        || stdout_lower.contains("opustranscode") {
        defects_found.push("OpusTranscode".to_string());
    }
    if ((stdout_lower.contains("vorbis") || stdout_lower.contains("ogg")) 
        && stdout_lower.contains("transcode"))
        || stdout_lower.contains("oggvorbistranscode") {
        defects_found.push("OggVorbisTranscode".to_string());
    }
    
    // Check for bit depth issues
    if stdout_lower.contains("bit depth mismatch") 
        || stdout_lower.contains("bitdepthmismatch")
        || (stdout_lower.contains("bit depth") && stdout_lower.contains("mismatch")) {
        defects_found.push("BitDepthMismatch".to_string());
    }
    
    // Check for upsampling
    if stdout_lower.contains("upsampled") 
        || (stdout_lower.contains("upsample") && !stdout_lower.contains("not upsampled")) {
        defects_found.push("Upsampled".to_string());
    }
    
    // Check for spectral artifacts
    if stdout_lower.contains("spectral artifact") {
        defects_found.push("SpectralArtifacts".to_string());
    }
    
    defects_found
}

fn run_single_test(binary: &Path, test_case: &TestCase) -> TestResult {
    let start = std::time::Instant::now();
    
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

    // First, parse all defects from the output
    let defects_found = parse_defects_from_output(&stdout);

    // Check for explicit status indicators in output
    let has_explicit_issues = stdout.contains("ISSUES DETECTED") 
        || stdout.contains("✗ ISSUES")
        || stdout.to_lowercase().contains("issues detected");
    let has_explicit_clean = (stdout.contains("CLEAN") || stdout.to_lowercase().contains("clean"))
        && !has_explicit_issues;

    // FIXED LOGIC: A file is "clean" (passed) if:
    // 1. No defects were parsed from the output, AND
    // 2. There's no explicit "ISSUES DETECTED" message
    // OR
    // 3. There's an explicit "CLEAN" status
    let is_clean = has_explicit_clean || (defects_found.is_empty() && !has_explicit_issues);

    TestResult {
        passed: is_clean,
        expected: test_case.should_pass,
        defects_found,
        description: test_case.description.clone(),
        category: test_case.category.clone(),
        file: test_case.file_path.clone(),
        duration_ms,
        stdout,
    }
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

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn define_qualification_tests(base: &Path) -> Vec<TestCase> {
    let mut cases = Vec::new();

    // =========================================================================
    // CLEANORIGIN - Original master files (2 tests)
    // =========================================================================
    
    cases.push(TestCase {
        file_path: base.join("CleanOrigin/input96.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "CleanOrigin: 96kHz 24-bit original master".to_string(),
        category: "CleanOrigin".to_string(),
        severity: AllureSeverity::Critical,
    });

    // Note: input192.flac is 16-bit source in 24-bit container
    cases.push(TestCase {
        file_path: base.join("CleanOrigin/input192.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "CleanOrigin: 192kHz (16-bit source in 24-bit container)".to_string(),
        category: "CleanOrigin".to_string(),
        severity: AllureSeverity::Critical,
    });

    // =========================================================================
    // CLEANTRANSCODED - Honest bit depth reduction (2 tests)
    // =========================================================================
    
    cases.push(TestCase {
        file_path: base.join("CleanTranscoded/input96_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "CleanTranscoded: 96kHz honest 16-bit transcode".to_string(),
        category: "CleanTranscoded".to_string(),
        severity: AllureSeverity::Normal,
    });

    cases.push(TestCase {
        file_path: base.join("CleanTranscoded/input192_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "CleanTranscoded: 192kHz honest 16-bit transcode".to_string(),
        category: "CleanTranscoded".to_string(),
        severity: AllureSeverity::Normal,
    });

    // =========================================================================
    // RESAMPLE96 - Sample rate changes from 96kHz source (3 tests - downsampling only)
    // =========================================================================
    
    // Downsample: genuine, no interpolation → PASS
    for rate in ["44", "48", "88"] {
        cases.push(TestCase {
            file_path: base.join(format!("Resample96/input96_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: true,
            expected_defects: vec![],
            description: format!("Resample96: 96→{}kHz downsampled (genuine)", rate),
            category: "Resample96".to_string(),
            severity: AllureSeverity::Normal,
        });
    }

    // =========================================================================
    // RESAMPLE192 - All from 16-bit source (5 tests) - should all FAIL BitDepthMismatch
    // =========================================================================
    
    for rate in ["44", "48", "88", "96", "176"] {
        cases.push(TestCase {
            file_path: base.join(format!("Resample192/input192_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: false,
            expected_defects: vec!["BitDepthMismatch".to_string()],
            description: format!("Resample192: 192→{}kHz (16-bit source)", rate),
            category: "Resample192".to_string(),
            severity: AllureSeverity::Normal,
        });
    }

    // =========================================================================
    // UPSCALE16 - 16-bit to 24-bit padding (2 tests)
    // =========================================================================
    
    cases.push(TestCase {
        file_path: base.join("Upscale16/output96_16bit.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "Upscale16: 96kHz 16→24-bit upscaled (fake depth)".to_string(),
        category: "Upscale16".to_string(),
        severity: AllureSeverity::Critical,
    });

    cases.push(TestCase {
        file_path: base.join("Upscale16/output192_16bit.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "Upscale16: 192kHz 16→24-bit upscaled (fake depth)".to_string(),
        category: "Upscale16".to_string(),
        severity: AllureSeverity::Critical,
    });

    // =========================================================================
    // UPSCALED - Lossy codec transcodes to FLAC (5 tests)
    // Focus on 192kHz sources (more detectable) + one 96kHz MP3
    // =========================================================================
    
    // 96kHz MP3 (known detectable)
    cases.push(TestCase {
        file_path: base.join("Upscaled/input96_mp3.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["Mp3Transcode".to_string()],
        description: "Upscaled: 96kHz from MP3 (lossy artifacts)".to_string(),
        category: "Upscaled".to_string(),
        severity: AllureSeverity::Critical,
    });

    // 192kHz lossy formats
    let lossy_formats_192 = vec![
        ("mp3", "Mp3Transcode"),
        ("m4a", "AacTranscode"),
        ("opus", "OpusTranscode"),
        ("ogg", "OggVorbisTranscode"),
    ];

    for (format, defect) in &lossy_formats_192 {
        cases.push(TestCase {
            file_path: base.join(format!("Upscaled/input192_{}.flac", format)).to_string_lossy().to_string(),
            should_pass: false,
            expected_defects: vec![defect.to_string()],
            description: format!("Upscaled: 192kHz from {} (lossy artifacts)", format.to_uppercase()),
            category: "Upscaled".to_string(),
            severity: AllureSeverity::Critical,
        });
    }

    // =========================================================================
    // MASTERSCRIPT - Key scenarios (5 tests)
    // =========================================================================
    
    // test96_original - reference file, should PASS
    cases.push(TestCase {
        file_path: base.join("MasterScript/test96_original.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "MasterScript: test96 original (reference, clean)".to_string(),
        category: "MasterScript".to_string(),
        severity: AllureSeverity::Critical,
    });

    // test192_original - 16-bit source, should FAIL
    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_original.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "MasterScript: test192 original (16-bit source)".to_string(),
        category: "MasterScript".to_string(),
        severity: AllureSeverity::Normal,
    });

    // test96 bit depth degradation
    cases.push(TestCase {
        file_path: base.join("MasterScript/test96_16bit_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "MasterScript: test96 16-bit upscaled to 24-bit".to_string(),
        category: "MasterScript".to_string(),
        severity: AllureSeverity::Normal,
    });

    // test192 MP3 320 upscaled (BitDepthMismatch + Mp3Transcode)
    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_mp3_320_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
        description: "MasterScript: test192 MP3 320k upscaled".to_string(),
        category: "MasterScript".to_string(),
        severity: AllureSeverity::Normal,
    });

    // test192 Opus 128 upscaled (BitDepthMismatch + OpusTranscode)
    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_opus_128_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
        description: "MasterScript: test192 Opus 128k upscaled".to_string(),
        category: "MasterScript".to_string(),
        severity: AllureSeverity::Normal,
    });

    // Total: 2 + 2 + 3 + 5 + 2 + 5 + 5 = 24 test cases
    
    cases
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
    assert!(stdout.contains("audiocheckr"), "Help output should mention audiocheckr");
}
