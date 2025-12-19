// tests/dithering_resampling_test.rs
//
// Dithering & Resampling Detection Test Suite
// Tests AudioCheckr's ability to detect various dithering algorithms and resampling engines
//
// Test Files Structure:
//   dithering_tests/
//     - input176.flac (control: 24-bit 176.4kHz original)
//     - output_{algorithm}_scale{0.5|1.0|1.5}.flac
//
//   resampling_tests/
//     - input176.flac (control: 24-bit 176.4kHz original)
//     - output_{samplerate}Hz_{engine}_{params}.flac
//
// Expected Behavior:
//   - Control files (input176.flac) → CLEAN (no defects)
//   - Dithered files → DitheringDetected (24→16 bit reduction with noise shaping)
//   - Downsampled files (176.4→44.1/48/88.2/96kHz) → ResamplingDetected (downsampling)
//   - Upsampled files (176.4→192kHz) → ResamplingDetected (upsampling)
//
// Jenkins Integration:
//   - Test files downloaded from MinIO as zip archives
//   - Manual trigger pipeline for comprehensive DSP validation
//
// v2: Fixed for Jenkins CI - reduced parallelism, added heartbeat output

mod test_utils;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use test_utils::{
    AllureEnvironment, AllureSeverity, AllureTestBuilder, AllureTestSuite,
    default_audiocheckr_categories, write_categories,
};

// ============================================================================
// Test Case Structures
// ============================================================================

#[derive(Debug, Clone)]
struct DspTestCase {
    file_path: String,
    filename: String,
    test_type: DspTestType,
    should_be_clean: bool,
    expected_defects: Vec<String>,
    description: String,
    // Dithering-specific
    dither_algorithm: Option<String>,
    dither_scale: Option<f32>,
    // Resampling-specific
    resampler_engine: Option<String>,
    target_sample_rate: Option<u32>,
    source_sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DspTestType {
    Control,
    Dithering,
    Downsampling,
    Upsampling,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ValidationResult {
    Pass,
    PassWithWarning,
    FalsePositive,
    FalseNegative,
    WrongDefectType,
    ExtraDefects,
}

#[derive(Debug)]
struct TestResult {
    #[allow(dead_code)]
    test_case: DspTestCase,
    is_clean: bool,
    defects_found: Vec<String>,
    validation_result: ValidationResult,
    missing_defects: Vec<String>,
    extra_defects: Vec<String>,
    stdout: String,
    duration_ms: u64,
}

// ============================================================================
// Dithering Algorithm Metadata
// ============================================================================

/// Maps dithering algorithm names to human-readable descriptions
fn get_dither_algorithm_info(algorithm: &str) -> (&'static str, &'static str) {
    match algorithm {
        "rectangular" => ("Rectangular (RPDF)", "Uniform distribution, no noise shaping"),
        "triangular" => ("Triangular (TPDF)", "Standard triangular PDF dither"),
        "triangular_hp" => ("Triangular High-Pass", "TPDF with high-pass filtering"),
        "shibata" => ("Shibata", "Psychoacoustic noise shaping"),
        "low_shibata" => ("Low Shibata", "Shibata optimized for low frequencies"),
        "high_shibata" => ("High Shibata", "Shibata optimized for high frequencies"),
        "lipshitz" => ("Lipshitz", "Minimized-audibility noise shaping"),
        "f_weighted" => ("F-Weighted", "Frequency-weighted noise shaping"),
        "modified_e_weighted" => ("Modified E-Weighted", "Modified equal-loudness weighting"),
        "improved_e_weighted" => ("Improved E-Weighted", "Improved equal-loudness weighting"),
        _ => ("Unknown", "Unknown dithering algorithm"),
    }
}

// ============================================================================
// Resampler Engine Metadata
// ============================================================================

/// Maps resampler engine names to human-readable descriptions
fn get_resampler_info(engine: &str, params: &str) -> (&'static str, &'static str) {
    match (engine, params) {
        ("soxr", "default") => ("SoXR Default", "SoX Resampler with default quality"),
        ("soxr", "vhq") => ("SoXR VHQ", "SoX Resampler Very High Quality"),
        ("soxr", "cutoff_91") => ("SoXR Cutoff 91%", "SoX Resampler with 91% passband"),
        ("swr", "default") => ("SWR Default", "FFmpeg SWResample default"),
        ("swr", "blackman_nuttall") => ("SWR Blackman-Nuttall", "SWR with Blackman-Nuttall window"),
        ("swr", "kaiser_beta12") => ("SWR Kaiser β=12", "SWR with Kaiser window β=12"),
        ("swr", "filter_size_16") => ("SWR Filter 16", "SWR with filter_size=16 (lower quality)"),
        _ => ("Unknown", "Unknown resampler configuration"),
    }
}

// ============================================================================
// CI Environment Detection
// ============================================================================

/// Detect if running in CI environment (Jenkins, GitHub Actions, etc.)
fn is_ci_environment() -> bool {
    std::env::var("JENKINS_HOME").is_ok() 
        || std::env::var("CI").is_ok()
        || std::env::var("BUILD_NUMBER").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
}

/// Get appropriate number of parallel threads for the environment
fn get_parallel_threads(requested: usize) -> usize {
    if is_ci_environment() {
        // CI environments often have memory constraints
        // Large audio files (150-300MB each) can exhaust memory with too many threads
        println!("CI environment detected - reducing parallelism for stability");
        let _ = std::io::stdout().flush();
        requested.min(2)
    } else {
        requested
    }
}

// ============================================================================
// Main Test Entry Points
// ============================================================================

#[test]
fn test_dithering_detection() {
    run_dsp_test_suite("dithering_tests", DspTestType::Dithering);
}

#[test]
fn test_resampling_detection() {
    run_dsp_test_suite("resampling_tests", DspTestType::Downsampling);
}

/// Combined test for both dithering and resampling (for full validation)
#[test]
fn test_dsp_full_suite() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let allure_results_dir = project_root.join("target").join("allure-results");

    // Setup Allure
    setup_allure_environment(&allure_results_dir, "DSP Full Suite");
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    let mut all_cases = Vec::new();

    // Collect dithering tests
    let dither_dir = project_root.join("dithering_tests");
    if dither_dir.exists() {
        all_cases.extend(scan_dithering_tests(&dither_dir));
    }

    // Collect resampling tests
    let resample_dir = project_root.join("resampling_tests");
    if resample_dir.exists() {
        all_cases.extend(scan_resampling_tests(&resample_dir));
    }

    if all_cases.is_empty() {
        println!("No DSP test files found. Ensure dithering_tests/ and resampling_tests/ directories exist.");
        return;
    }

    println!("\n{}", "=".repeat(80));
    println!("DSP FULL TEST SUITE");
    println!("Total test cases: {}", all_cases.len());
    println!("{}\n", "=".repeat(80));
    let _ = std::io::stdout().flush();

    let results = run_tests_parallel(&binary_path, all_cases.clone(), 4);
    let mut allure_suite = AllureTestSuite::new("DSP Full Tests", &allure_results_dir);

    let stats = analyze_and_report_results(&results, &all_cases, &mut allure_suite, &allure_results_dir);

    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\nAllure results written to: {}", allure_results_dir.display());

    assert_eq!(
        stats.failed, 0,
        "DSP tests failed: {} test(s) did not pass",
        stats.failed
    );
}

// ============================================================================
// Test Suite Runner
// ============================================================================

fn run_dsp_test_suite(test_dir_name: &str, _primary_type: DspTestType) {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join(test_dir_name);
    let allure_results_dir = project_root.join("target").join("allure-results");

    if !test_base.exists() {
        panic!(
            "{} directory not found at: {}. \
             Download from MinIO for DSP tests.",
            test_dir_name,
            test_base.display()
        );
    }

    let suite_name = match test_dir_name {
        "dithering_tests" => "Dithering Detection",
        "resampling_tests" => "Resampling Detection",
        _ => "DSP Tests",
    };

    println!("\n{}", "=".repeat(80));
    println!("{} TEST SUITE", suite_name.to_uppercase());
    println!("Using: {}", test_base.display());
    println!("Allure results: {}", allure_results_dir.display());
    println!("{}\n", "=".repeat(80));
    let _ = std::io::stdout().flush();

    setup_allure_environment(&allure_results_dir, suite_name);
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    let test_cases = match test_dir_name {
        "dithering_tests" => scan_dithering_tests(&test_base),
        "resampling_tests" => scan_resampling_tests(&test_base),
        _ => Vec::new(),
    };

    if test_cases.is_empty() {
        println!("WARNING: No test files found in {}", test_base.display());
        return;
    }

    println!("Found {} test files\n", test_cases.len());
    let _ = std::io::stdout().flush();

    let results = run_tests_parallel(&binary_path, test_cases.clone(), 4);
    let mut allure_suite = AllureTestSuite::new(&format!("{} Tests", suite_name), &allure_results_dir);

    let stats = analyze_and_report_results(&results, &test_cases, &mut allure_suite, &allure_results_dir);

    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\nAllure results written to: {}", allure_results_dir.display());

    assert_eq!(
        stats.failed, 0,
        "{} tests failed: {} test(s) did not pass",
        suite_name, stats.failed
    );
}

// ============================================================================
// Test Case Scanners
// ============================================================================

fn scan_dithering_tests(base: &Path) -> Vec<DspTestCase> {
    let mut cases = Vec::new();
    let source_sample_rate = 176400; // 176.4 kHz original

    let entries = match fs::read_dir(base) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to read dithering_tests: {}", e);
            return cases;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("flac") {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        if filename == "input176.flac" {
            // Control file
            cases.push(DspTestCase {
                file_path: path.to_string_lossy().to_string(),
                filename: filename.clone(),
                test_type: DspTestType::Control,
                should_be_clean: true,
                expected_defects: vec![],
                description: "Control: 24-bit 176.4kHz original (no processing)".to_string(),
                dither_algorithm: None,
                dither_scale: None,
                resampler_engine: None,
                target_sample_rate: None,
                source_sample_rate,
            });
        } else if filename.starts_with("output_") {
            // Parse dithering output filename
            // Pattern: output_{algorithm}_scale{scale}.flac
            if let Some((algorithm, scale)) = parse_dither_filename(&filename) {
                let (algo_name, algo_desc) = get_dither_algorithm_info(&algorithm);
                cases.push(DspTestCase {
                    file_path: path.to_string_lossy().to_string(),
                    filename: filename.clone(),
                    test_type: DspTestType::Dithering,
                    should_be_clean: false,
                    expected_defects: vec!["DitheringDetected".to_string()],
                    description: format!(
                        "Dithering: {} (scale {:.1}) - {}",
                        algo_name, scale, algo_desc
                    ),
                    dither_algorithm: Some(algorithm),
                    dither_scale: Some(scale),
                    resampler_engine: None,
                    target_sample_rate: None,
                    source_sample_rate,
                });
            }
        }
    }

    // Sort by algorithm then scale
    cases.sort_by(|a, b| {
        a.dither_algorithm
            .cmp(&b.dither_algorithm)
            .then(a.dither_scale.partial_cmp(&b.dither_scale).unwrap_or(std::cmp::Ordering::Equal))
    });

    cases
}

fn scan_resampling_tests(base: &Path) -> Vec<DspTestCase> {
    let mut cases = Vec::new();
    let source_sample_rate = 176400; // 176.4 kHz original

    let entries = match fs::read_dir(base) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to read resampling_tests: {}", e);
            return cases;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("flac") {
            continue;
        }

        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        if filename == "input176.flac" {
            // Control file
            cases.push(DspTestCase {
                file_path: path.to_string_lossy().to_string(),
                filename: filename.clone(),
                test_type: DspTestType::Control,
                should_be_clean: true,
                expected_defects: vec![],
                description: "Control: 24-bit 176.4kHz original (no processing)".to_string(),
                dither_algorithm: None,
                dither_scale: None,
                resampler_engine: None,
                target_sample_rate: None,
                source_sample_rate,
            });
        } else if filename.starts_with("output_") {
            // Parse resampling output filename
            // Pattern: output_{samplerate}Hz_{engine}_{params}.flac
            if let Some((target_rate, engine, params)) = parse_resample_filename(&filename) {
                let (engine_name, engine_desc) = get_resampler_info(&engine, &params);
                let is_upsampling = target_rate > source_sample_rate;
                let test_type = if is_upsampling {
                    DspTestType::Upsampling
                } else {
                    DspTestType::Downsampling
                };

                let direction = if is_upsampling { "↑" } else { "↓" };
                cases.push(DspTestCase {
                    file_path: path.to_string_lossy().to_string(),
                    filename: filename.clone(),
                    test_type,
                    should_be_clean: false,
                    expected_defects: vec!["ResamplingDetected".to_string()],
                    description: format!(
                        "Resampling: {} Hz {} {} Hz using {} - {}",
                        source_sample_rate, direction, target_rate, engine_name, engine_desc
                    ),
                    dither_algorithm: None,
                    dither_scale: None,
                    resampler_engine: Some(format!("{}_{}", engine, params)),
                    target_sample_rate: Some(target_rate),
                    source_sample_rate,
                });
            }
        }
    }

    // Sort by sample rate then engine
    cases.sort_by(|a, b| {
        a.target_sample_rate
            .cmp(&b.target_sample_rate)
            .then(a.resampler_engine.cmp(&b.resampler_engine))
    });

    cases
}

// ============================================================================
// Filename Parsers
// ============================================================================

/// Parse dithering filename: output_{algorithm}_scale{scale}.flac
fn parse_dither_filename(filename: &str) -> Option<(String, f32)> {
    let name = filename.strip_prefix("output_")?.strip_suffix(".flac")?;

    // Find the last occurrence of "_scale"
    let scale_idx = name.rfind("_scale")?;
    let algorithm = &name[..scale_idx];
    let scale_str = &name[scale_idx + 6..]; // Skip "_scale"

    // Parse scale (e.g., "0_5" -> 0.5, "1_0" -> 1.0, "1_5" -> 1.5)
    let scale: f32 = scale_str.replace('_', ".").parse().ok()?;

    Some((algorithm.to_string(), scale))
}

/// Parse resampling filename: output_{samplerate}Hz_{engine}_{params}.flac
fn parse_resample_filename(filename: &str) -> Option<(u32, String, String)> {
    let name = filename.strip_prefix("output_")?.strip_suffix(".flac")?;

    // Find Hz marker
    let hz_idx = name.find("Hz_")?;
    let rate_str = &name[..hz_idx];
    let rate: u32 = rate_str.parse().ok()?;

    let remainder = &name[hz_idx + 3..]; // Skip "Hz_"

    // Split engine and params
    // Pattern: soxr_default, soxr_vhq, soxr_cutoff_91, swr_default, swr_blackman_nuttall, etc.
    let parts: Vec<&str> = remainder.splitn(2, '_').collect();
    if parts.len() < 2 {
        return None;
    }

    let engine = parts[0].to_string();
    let params = parts[1].to_string();

    Some((rate, engine, params))
}

// ============================================================================
// Parallel Test Executor (CI-aware)
// ============================================================================

fn run_tests_parallel(binary: &Path, test_cases: Vec<DspTestCase>, num_threads: usize) -> Vec<TestResult> {
    let binary = binary.to_path_buf();
    let test_cases = Arc::new(test_cases);
    let results = Arc::new(Mutex::new(Vec::new()));
    let index = Arc::new(Mutex::new(0usize));
    let total = test_cases.len();
    let mut handles = Vec::new();

    // Reduce parallelism for CI environments to avoid memory/I/O issues
    let effective_threads = get_parallel_threads(num_threads);

    println!("Running tests with {} parallel threads...\n", effective_threads);
    let _ = std::io::stdout().flush();

    for _ in 0..effective_threads {
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
                
                // Print progress for EVERY test to keep Jenkins heartbeat alive
                // Large audio files (150-300MB) can take 30+ seconds each
                println!("[{}/{}] Testing: {}", current_idx + 1, total, test_case.filename);
                let _ = std::io::stdout().flush();
                
                let result = run_single_test(&binary, test_case);

                // Print completion status immediately
                let status = match result.validation_result {
                    ValidationResult::Pass => "✓ PASS",
                    ValidationResult::PassWithWarning => "⚠ PASS (partial)",
                    ValidationResult::FalsePositive => "✗ FALSE POSITIVE",
                    ValidationResult::FalseNegative => "✗ FALSE NEGATIVE", 
                    ValidationResult::WrongDefectType => "✗ WRONG DEFECT",
                    ValidationResult::ExtraDefects => "✗ EXTRA DEFECTS",
                };
                println!("[{}/{}] {} - {} ({}ms)", 
                    current_idx + 1, total, status, test_case.filename, result.duration_ms);
                let _ = std::io::stdout().flush();

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

fn run_single_test(binary: &Path, test_case: &DspTestCase) -> TestResult {
    let start = std::time::Instant::now();

    // Build command with appropriate flags for DSP detection
    let output = Command::new(binary)
        .arg("--input")
        .arg(&test_case.file_path)
        .arg("--bit-depth")
        .arg("24") // Expect 24-bit container
        .arg("--check-upsampling")
        .arg("--verbose")
        .output()
        .expect("Failed to execute binary");

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let defects_found = parse_defects_from_output(&stdout);
    let is_clean = defects_found.is_empty();

    let (validation_result, missing_defects, extra_defects) = validate_test_result(
        is_clean,
        test_case.should_be_clean,
        &test_case.expected_defects,
        &defects_found,
    );

    TestResult {
        test_case: test_case.clone(),
        is_clean,
        defects_found,
        validation_result,
        missing_defects,
        extra_defects,
        stdout,
        duration_ms,
    }
}

// ============================================================================
// Output Parser
// ============================================================================

fn parse_defects_from_output(stdout: &str) -> Vec<String> {
    let mut defects = Vec::new();
    let stdout_lower = stdout.to_lowercase();

    // Dithering detection
    if stdout_lower.contains("dithering detected")
        || stdout_lower.contains("ditheringdetected")
        || stdout_lower.contains("dither")
            && (stdout_lower.contains("detected") || stdout_lower.contains("found"))
    {
        defects.push("DitheringDetected".to_string());
    }

    // Resampling detection
    if stdout_lower.contains("resampling detected")
        || stdout_lower.contains("resamplingdetected")
        || stdout_lower.contains("resample")
            && (stdout_lower.contains("detected") || stdout_lower.contains("found"))
    {
        defects.push("ResamplingDetected".to_string());
    }

    // Also check for upsampling specifically
    if stdout_lower.contains("upsampled")
        || stdout_lower.contains("upsampling")
            && !stdout_lower.contains("not upsampled")
    {
        if !defects.contains(&"ResamplingDetected".to_string()) {
            defects.push("ResamplingDetected".to_string());
        }
    }

    // Bit depth mismatch (might co-occur with dithering)
    if stdout_lower.contains("bit depth mismatch")
        || stdout_lower.contains("bitdepthmismatch")
    {
        defects.push("BitDepthMismatch".to_string());
    }

    // Transcode artifacts (shouldn't appear but check anyway)
    if stdout_lower.contains("mp3") && stdout_lower.contains("transcode") {
        defects.push("Mp3Transcode".to_string());
    }
    if stdout_lower.contains("aac") && stdout_lower.contains("transcode") {
        defects.push("AacTranscode".to_string());
    }

    defects
}

// ============================================================================
// Validation Logic
// ============================================================================

fn validate_test_result(
    is_clean: bool,
    should_be_clean: bool,
    expected_defects: &[String],
    defects_found: &[String],
) -> (ValidationResult, Vec<String>, Vec<String>) {
    let expected_set: HashSet<&String> = expected_defects.iter().collect();
    let found_set: HashSet<&String> = defects_found.iter().collect();

    let missing: Vec<String> = expected_defects
        .iter()
        .filter(|d| !found_set.contains(d))
        .cloned()
        .collect();

    let extra: Vec<String> = defects_found
        .iter()
        .filter(|d| !expected_set.contains(d))
        .cloned()
        .collect();

    // Case 1: Expected CLEAN
    if should_be_clean {
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
        (ValidationResult::Pass, missing, extra)
    } else {
        (ValidationResult::PassWithWarning, missing, extra)
    }
}

// ============================================================================
// Results Analysis & Reporting
// ============================================================================

struct TestStats {
    total: usize,
    passed: usize,
    passed_with_warning: usize,
    failed: usize,
    false_positives: usize,
    false_negatives: usize,
    wrong_defect_type: usize,
    extra_defects: usize,
}

fn analyze_and_report_results(
    results: &[TestResult],
    test_cases: &[DspTestCase],
    allure_suite: &mut AllureTestSuite,
    allure_results_dir: &Path,
) -> TestStats {
    let mut stats = TestStats {
        total: results.len(),
        passed: 0,
        passed_with_warning: 0,
        failed: 0,
        false_positives: 0,
        false_negatives: 0,
        wrong_defect_type: 0,
        extra_defects: 0,
    };

    let mut results_by_type: HashMap<DspTestType, Vec<&TestResult>> = HashMap::new();

    println!("\n{}", "=".repeat(80));
    println!("TEST RESULTS SUMMARY");
    println!("{}", "=".repeat(80));
    let _ = std::io::stdout().flush();

    for (idx, result) in results.iter().enumerate() {
        let test_case = &test_cases[idx];

        results_by_type
            .entry(test_case.test_type)
            .or_default()
            .push(result);

        // Build Allure test result
        let severity = match test_case.test_type {
            DspTestType::Control => AllureSeverity::Critical,
            DspTestType::Dithering => AllureSeverity::Normal,
            DspTestType::Downsampling => AllureSeverity::Normal,
            DspTestType::Upsampling => AllureSeverity::Normal,
        };

        let feature = match test_case.test_type {
            DspTestType::Control => "Control Files",
            DspTestType::Dithering => "Dithering Detection",
            DspTestType::Downsampling => "Downsampling Detection",
            DspTestType::Upsampling => "Upsampling Detection",
        };

        let mut builder = AllureTestBuilder::new(&test_case.description)
            .full_name(&format!(
                "dsp_test::{}::{}",
                feature.to_lowercase().replace(' ', "_"),
                sanitize_name(&test_case.filename)
            ))
            .severity(severity)
            .epic("AudioCheckr")
            .feature(feature)
            .story(&test_case.filename)
            .suite("DSP Tests")
            .sub_suite(feature)
            .tag("dsp")
            .parameter("file", &test_case.filename)
            .parameter("test_type", &format!("{:?}", test_case.test_type))
            .parameter("expected_clean", &test_case.should_be_clean.to_string())
            .parameter("defects_found", &format!("{:?}", result.defects_found))
            .parameter("expected_defects", &format!("{:?}", test_case.expected_defects))
            .parameter("duration_ms", &result.duration_ms.to_string());

        // Add dithering-specific parameters
        if let Some(ref algo) = test_case.dither_algorithm {
            builder = builder.parameter("dither_algorithm", algo);
        }
        if let Some(scale) = test_case.dither_scale {
            builder = builder.parameter("dither_scale", &format!("{:.1}", scale));
        }

        // Add resampling-specific parameters
        if let Some(ref engine) = test_case.resampler_engine {
            builder = builder.parameter("resampler_engine", engine);
        }
        if let Some(rate) = test_case.target_sample_rate {
            builder = builder.parameter("target_sample_rate", &format!("{} Hz", rate));
        }
        builder = builder.parameter("source_sample_rate", &format!("{} Hz", test_case.source_sample_rate));

        let description = format!(
            "**File:** `{}`\n\n\
            **Test Type:** {:?}\n\n\
            **Description:** {}\n\n\
            **Expected:** {}\n\n\
            **Actual:** {}\n\n\
            **Defects Found:** {:?}\n\n\
            **Expected Defects:** {:?}\n\n\
            **Missing Defects:** {:?}\n\n\
            **Extra Defects:** {:?}\n\n\
            **Validation Result:** {:?}\n\n\
            **Duration:** {} ms",
            test_case.filename,
            test_case.test_type,
            test_case.description,
            if test_case.should_be_clean { "CLEAN".to_string() } else { format!("DEFECTIVE with {:?}", test_case.expected_defects) },
            if result.is_clean { "CLEAN" } else { "DEFECTIVE" },
            result.defects_found,
            test_case.expected_defects,
            result.missing_defects,
            result.extra_defects,
            result.validation_result,
            result.duration_ms
        );
        builder = builder.description(&description);

        // Attach stdout
        let _ = builder.attach_text("Analysis Output", &result.stdout, allure_results_dir);

        match result.validation_result {
            ValidationResult::Pass => {
                stats.passed += 1;
                builder = builder.passed();
            }
            ValidationResult::PassWithWarning => {
                stats.passed_with_warning += 1;
                stats.passed += 1;
                builder = builder.passed();
            }
            ValidationResult::FalsePositive => {
                stats.failed += 1;
                stats.false_positives += 1;
                let message = format!(
                    "FALSE POSITIVE: Expected CLEAN but detected defects: {:?}",
                    result.defects_found
                );
                builder = builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::FalseNegative => {
                stats.failed += 1;
                stats.false_negatives += 1;
                let message = format!(
                    "FALSE NEGATIVE: Expected defects {:?} but got CLEAN",
                    test_case.expected_defects
                );
                builder = builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::WrongDefectType => {
                stats.failed += 1;
                stats.wrong_defect_type += 1;
                let message = format!(
                    "WRONG DEFECT TYPE: Expected {:?} but detected {:?}",
                    test_case.expected_defects, result.defects_found
                );
                builder = builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::ExtraDefects => {
                stats.failed += 1;
                stats.extra_defects += 1;
                let message = format!(
                    "EXTRA DEFECTS: Expected {:?} but also detected extra: {:?}",
                    test_case.expected_defects, result.extra_defects
                );
                builder = builder.failed(&message, Some(&result.stdout));
            }
        }

        allure_suite.add_result(builder.build());
    }

    // Print summary
    println!("\nTotal Tests:     {}", stats.total);
    println!(
        "Passed:          {} ({:.1}%)",
        stats.passed,
        (stats.passed as f32 / stats.total as f32) * 100.0
    );
    println!("  - Clean passes: {}", stats.passed - stats.passed_with_warning);
    println!("  - Partial match: {}", stats.passed_with_warning);
    println!("Failed:          {}", stats.failed);
    println!("  - False Positives: {}", stats.false_positives);
    println!("  - False Negatives: {}", stats.false_negatives);
    println!("  - Wrong Defect Type: {}", stats.wrong_defect_type);
    println!("  - Extra Defects: {}", stats.extra_defects);

    // Print breakdown by test type
    println!("\n{}", "-".repeat(80));
    println!("Results by Test Type:");
    println!("{}", "-".repeat(80));

    for test_type in &[DspTestType::Control, DspTestType::Dithering, DspTestType::Downsampling, DspTestType::Upsampling] {
        if let Some(type_results) = results_by_type.get(test_type) {
            let type_passed = type_results
                .iter()
                .filter(|r| matches!(r.validation_result, ValidationResult::Pass | ValidationResult::PassWithWarning))
                .count();
            let type_total = type_results.len();
            println!(
                "{:20} {:3}/{:3} ({:.0}%)",
                format!("{:?}", test_type),
                type_passed,
                type_total,
                if type_total > 0 { (type_passed as f32 / type_total as f32) * 100.0 } else { 0.0 }
            );
        }
    }
    println!("{}", "=".repeat(80));
    let _ = std::io::stdout().flush();

    stats
}

// ============================================================================
// Utility Functions
// ============================================================================

fn setup_allure_environment(results_dir: &Path, suite_name: &str) {
    let mut env = AllureEnvironment::new();

    env.add("OS", std::env::consts::OS);
    env.add("Architecture", std::env::consts::ARCH);
    env.add("Rust Version", env!("CARGO_PKG_VERSION"));
    env.add("Test Suite", suite_name);
    env.add("CI Environment", if is_ci_environment() { "Yes" } else { "No" });

    if let Ok(hostname) = std::env::var("HOSTNAME") {
        env.add("Host", &hostname);
    }
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

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn get_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");

    // Prefer release build (much faster for audio processing)
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

// ============================================================================
// Standalone Tests
// ============================================================================

#[test]
fn test_binary_exists() {
    let binary_path = get_binary_path();
    assert!(binary_path.exists(), "Binary not found at {:?}", binary_path);
}

#[test]
fn test_parse_dither_filename() {
    assert_eq!(
        parse_dither_filename("output_rectangular_scale0_5.flac"),
        Some(("rectangular".to_string(), 0.5))
    );
    assert_eq!(
        parse_dither_filename("output_high_shibata_scale1_0.flac"),
        Some(("high_shibata".to_string(), 1.0))
    );
    assert_eq!(
        parse_dither_filename("output_improved_e_weighted_scale1_5.flac"),
        Some(("improved_e_weighted".to_string(), 1.5))
    );
    assert_eq!(parse_dither_filename("input176.flac"), None);
}

#[test]
fn test_parse_resample_filename() {
    assert_eq!(
        parse_resample_filename("output_44100Hz_soxr_default.flac"),
        Some((44100, "soxr".to_string(), "default".to_string()))
    );
    assert_eq!(
        parse_resample_filename("output_192000Hz_swr_blackman_nuttall.flac"),
        Some((192000, "swr".to_string(), "blackman_nuttall".to_string()))
    );
    assert_eq!(
        parse_resample_filename("output_96000Hz_soxr_cutoff_91.flac"),
        Some((96000, "soxr".to_string(), "cutoff_91".to_string()))
    );
    assert_eq!(parse_resample_filename("input176.flac"), None);
}
