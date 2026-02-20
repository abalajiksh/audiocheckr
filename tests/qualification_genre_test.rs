// tests/qualification_genre_test.rs

// QUALIFICATION Genre Test Suite - Compact subset for CI/CD quick validation
// Uses GenreTestSuiteLite (~50 files) for fast validation on every push
//
// Now with Allure reporting support for better visualization
//
// Purpose: Quick sanity check with real music samples
// - Tests representative samples from Control and major defect categories
// - Focuses on high-confidence detection scenarios
// - Parallel execution (4 threads) for faster CI/CD
//
// v4: Fixed validation logic to check SPECIFIC defect types, not just defective/clean status
//     - Expected CLEAN + got CLEAN → PASS
//     - Expected CLEAN + got DEFECTIVE → FAIL (false positive)
//     - Expected DEFECTIVE + got CLEAN → FAIL (false negative)
//     - Expected DEFECTIVE + got correct defect type(s) → PASS
//     - Expected DEFECTIVE + got correct + extra defects → PASS (with warning)
//     - Expected DEFECTIVE + got ONLY wrong defect types → FAIL (wrong detection)
// v5: Fixed type mismatch in description formatting (line 181)
// v6: STRICTER validation logic
//     - Extra/wrong defects detected → FAIL (not pass), reported as warning
//     - Multiple expected defects with partial match → PASS with warning (acceptable)
// v7: Migrated from text-scraping to --format both for structured defect extraction
//     - Text output → stderr (captured as Allure "Analysis Output" attachment)
//     - JSON output → stdout (parsed via serde_json for defect detection)
//     - Replaces fragile parse_defects_from_output string matching
//     - Defects extracted directly from detections[].defect_type JSON field
//     - is_clean driven by verdict.genuine from JSON (matches app logic exactly)

mod test_utils;

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use test_utils::{
    default_audiocheckr_categories, get_binary_path, run_audiocheckr, write_categories,
    AllureEnvironment, AllureSeverity, AllureTestBuilder, AllureTestSuite,
};

// ── JSON deserialization structs ─────────────────────────────────────────────
// Mirrors the EnrichedOutput that --format both / --format json writes to stdout.
// Only the fields we actually need are declared; unknown fields are ignored by serde.

/// Top-level JSON report shape from audiocheckr --format both (stdout)
#[derive(Deserialize)]
struct JsonReport {
    verdict: JsonVerdict,
    detections: Vec<JsonDetection>,
}

/// The verdict block: `{ "genuine": bool, "quality_score": f64, ... }`
#[derive(Deserialize)]
struct JsonVerdict {
    genuine: bool,
}

/// One entry in the detections array.
/// `defect_type` is an externally-tagged serde enum, e.g.
///   `{ "LossyTranscode": { "codec": "MP3", "estimated_bitrate": 128, "cutoff_hz": 16000 } }`
///   `{ "BitDepthInflated": { "actual_bits": 16, "claimed_bits": 24 } }`
/// We keep it as a raw Value to avoid duplicating the entire DefectType enum here.
#[derive(Deserialize)]
struct JsonDetection {
    defect_type: serde_json::Value,
    severity: String,
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct GenreTestCase {
    file_path: String,
    should_pass: bool,
    expected_defects: Vec<String>,
    description: String,
    genre: String,
    defect_category: String,
}

/// Result of validating a test
#[derive(Debug, Clone, Copy, PartialEq)]
enum ValidationResult {
    /// Test passed - correct detection (clean==clean, or all expected defects found with no extras)
    Pass,
    /// Test passed with warning - expected defects found but some expected defects missing
    /// (partial match is OK, can fine-tune later)
    PassWithWarning,
    /// Test failed - false positive (clean file flagged as defective)
    FalsePositive,
    /// Test failed - false negative (defective file marked as clean)
    FalseNegative,
    /// Test failed - wrong defect type detected (none of the expected defects found)
    WrongDefectType,
    /// Test failed - extra defects detected beyond expected (wrong additional detection)
    ExtraDefects,
}

#[derive(Debug)]
struct TestResult {
    passed: bool,   // Whether file was detected as clean
    expected: bool, // Whether file should be clean
    defects_found: Vec<String>,
    expected_defects: Vec<String>, // Store expected defects for validation
    validation_result: ValidationResult, // Detailed validation result
    extra_defects: Vec<String>,    // Defects detected beyond expected
    missing_defects: Vec<String>,  // Expected defects not detected
    description: String,
    category: String,
    file: String,
    genre: String,
    stdout: String, // Human-readable text from stderr (used for Allure attachment)
    duration_ms: u64,
}

#[test]
fn test_qualification_genre_suite() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let allure_results_dir = project_root.join("target").join("allure-results");

    // Try GenreTestSuiteLite first, then TestSuite as fallback
    let test_base = if project_root.join("GenreTestSuiteLite").exists() {
        project_root.join("GenreTestSuiteLite")
    } else if project_root.join("TestSuite").exists() {
        project_root.join("TestSuite")
    } else {
        panic!(
            "Neither GenreTestSuiteLite nor TestSuite directory found. \
             Download GenreTestSuiteLite.zip from MinIO for qualification genre tests."
        );
    };

    println!("\n{}", "=".repeat(70));
    println!("QUALIFICATION GENRE TEST SUITE (Parallel Execution)");
    println!("Using: {}", test_base.display());
    println!("Allure results: {}", allure_results_dir.display());
    println!("{}\n", "=".repeat(70));

    // Setup Allure environment
    setup_allure_environment(&allure_results_dir, "Qualification Genre");
    let _ = write_categories(&default_audiocheckr_categories(), &allure_results_dir);

    // Scan categories and build test cases
    let test_cases = scan_and_build_test_cases(&test_base);
    let total_tests = test_cases.len();

    if total_tests == 0 {
        println!("WARNING: No test files found in {}", test_base.display());
        println!("Expected FLAC files in category subdirectories or flat structure.");
        return;
    }

    println!(
        "Found {} files across {} categories\n",
        total_tests,
        count_categories(&test_cases)
    );

    // Run tests in parallel with 4 threads
    let results = run_tests_parallel(test_cases.clone(), 4);

    // Create Allure test suite
    let mut allure_suite = AllureTestSuite::new("Qualification Genre Tests", &allure_results_dir);

    // Analyze results by category
    let mut passed = 0;
    let mut passed_with_warning = 0;
    let mut failed = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;
    let mut wrong_defect_type = 0;
    let mut extra_defects_count = 0;
    let mut results_by_category: HashMap<String, Vec<&TestResult>> = HashMap::new();

    for (idx, result) in results.iter().enumerate() {
        let test_case = &test_cases[idx];

        results_by_category
            .entry(result.category.clone())
            .or_insert_with(Vec::new)
            .push(result);

        // Build Allure test result
        let severity = match result.category.as_str() {
            "Control_Original" => AllureSeverity::Critical,
            cat if cat.contains("MP3_128") => AllureSeverity::Critical,
            cat if cat.contains("BitDepth") => AllureSeverity::Critical,
            _ => AllureSeverity::Normal,
        };

        let expected_str = if result.expected {
            "CLEAN (should pass)".to_string()
        } else {
            format!("DEFECTIVE with {:?}", result.expected_defects)
        };

        let mut allure_builder = AllureTestBuilder::new(&result.description)
            .full_name(&format!(
                "qualification_genre_test::{}",
                sanitize_name(&result.description)
            ))
            .severity(severity)
            .epic("AudioCheckr")
            .feature("Genre-Based Detection")
            .story(&result.category)
            .suite("Qualification Genre")
            .sub_suite(&result.category)
            .tag("qualification")
            .tag("genre")
            .tag(&result.genre.to_lowercase().replace(' ', "_"))
            .parameter("file", &result.file)
            .parameter("genre", &result.genre)
            .parameter("expected_pass", &result.expected.to_string())
            .parameter("defects_found", &format!("{:?}", result.defects_found))
            .parameter(
                "expected_defects",
                &format!("{:?}", result.expected_defects),
            )
            .parameter(
                "validation_result",
                &format!("{:?}", result.validation_result),
            );

        let description = format!(
            "**File:** `{}`\n\n\
            **Genre:** {}\n\n\
            **Category:** {}\n\n\
            **Expected:** {}\n\n\
            **Actual:** {}\n\n\
            **Defects Found:** {:?}\n\n\
            **Expected Defects:** {:?}\n\n\
            **Missing Defects:** {:?}\n\n\
            **Extra Defects:** {:?}\n\n\
            **Validation Result:** {:?}",
            result.file,
            result.genre,
            result.category,
            expected_str,
            if result.passed { "CLEAN" } else { "DEFECTIVE" },
            result.defects_found,
            result.expected_defects,
            result.missing_defects,
            result.extra_defects,
            result.validation_result
        );
        allure_builder = allure_builder.description(&description);

        // Attach text output (from stderr) as evidence
        allure_builder =
            allure_builder.attach_text("Analysis Output", &result.stdout, &allure_results_dir);

        match result.validation_result {
            ValidationResult::Pass => {
                passed += 1;
                allure_builder = allure_builder.passed();
            }
            ValidationResult::PassWithWarning => {
                passed_with_warning += 1;
                passed += 1;
                println!(
                    "⚠ PASS (partial match) [{}]: {} - Found {:?}, Missing {:?}",
                    result.category,
                    result.description,
                    result.defects_found,
                    result.missing_defects
                );
                allure_builder = allure_builder.passed();
            }
            ValidationResult::FalsePositive => {
                failed += 1;
                false_positives += 1;
                let message = format!(
                    "FALSE POSITIVE: Expected CLEAN but detected defects: {:?}",
                    result.defects_found
                );
                println!(
                    "✗ FALSE POSITIVE [{}]: {} - Found: {:?}",
                    result.category, result.description, result.defects_found
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::FalseNegative => {
                failed += 1;
                false_negatives += 1;
                let message = format!(
                    "FALSE NEGATIVE: Expected defects {:?} but got CLEAN",
                    result.expected_defects
                );
                println!(
                    "✗ FALSE NEGATIVE [{}]: {}",
                    result.category, result.description
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
            ValidationResult::WrongDefectType => {
                failed += 1;
                wrong_defect_type += 1;
                let message = format!(
                    "WRONG DEFECT TYPE: Expected {:?} but detected {:?} (none of expected defects found)",
                    result.expected_defects, result.defects_found
                );
                println!(
                    "✗ WRONG DEFECT [{}]: {} - Expected {:?}, Got {:?}",
                    result.category,
                    result.description,
                    result.expected_defects,
                    result.defects_found
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
                    "✗ EXTRA DEFECTS [{}]: {} - Expected {:?}, Extra {:?}",
                    result.category,
                    result.description,
                    result.expected_defects,
                    result.extra_defects
                );
                allure_builder = allure_builder.failed(&message, Some(&result.stdout));
            }
        }

        allure_suite.add_result(allure_builder.build());
    }

    // Write all Allure results
    if let Err(e) = allure_suite.write_all() {
        eprintln!("Warning: Failed to write Allure results: {}", e);
    }

    println!("\n{}", "=".repeat(70));
    println!("QUALIFICATION GENRE RESULTS");
    println!("{}", "=".repeat(70));
    println!("Total Tests: {}", total_tests);
    println!(
        "Passed: {} ({:.1}%)",
        passed,
        (passed as f32 / total_tests as f32) * 100.0
    );
    println!("  - Clean passes: {}", passed - passed_with_warning);
    println!(
        "  - Passed with warnings (partial match, missing some expected): {}",
        passed_with_warning
    );
    println!("Failed: {}", failed);
    println!("  - False Positives: {}", false_positives);
    println!("  - False Negatives: {}", false_negatives);
    println!("  - Wrong Defect Type: {}", wrong_defect_type);
    println!("  - Extra Defects Detected: {}", extra_defects_count);

    // Category breakdown
    println!("\n{}", "-".repeat(70));
    println!("Results by Category:");
    println!("{}", "-".repeat(70));

    let mut categories: Vec<_> = results_by_category.iter().collect();
    categories.sort_by_key(|(k, _)| k.as_str());

    for (category, cat_results) in categories {
        let cat_passed = cat_results
            .iter()
            .filter(|r| {
                matches!(
                    r.validation_result,
                    ValidationResult::Pass | ValidationResult::PassWithWarning
                )
            })
            .count();
        let cat_total = cat_results.len();
        let cat_wrong = cat_results
            .iter()
            .filter(|r| matches!(r.validation_result, ValidationResult::WrongDefectType))
            .count();
        let cat_extra = cat_results
            .iter()
            .filter(|r| matches!(r.validation_result, ValidationResult::ExtraDefects))
            .count();

        let mut status = String::new();
        if cat_wrong > 0 {
            status.push_str(&format!(" [⚠ {} wrong type]", cat_wrong));
        }
        if cat_extra > 0 {
            status.push_str(&format!(" [⚠ {} extra defects]", cat_extra));
        }

        println!(
            "{:35} {:3}/{:3} ({:.0}%){}",
            category,
            cat_passed,
            cat_total,
            (cat_passed as f32 / cat_total as f32) * 100.0,
            status
        );
    }
    println!("{}", "=".repeat(70));

    println!(
        "\nAllure results written to: {}",
        allure_results_dir.display()
    );

    assert_eq!(
        failed, 0,
        "Qualification genre tests failed: {} test(s) did not pass",
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
    if let Ok(branch) = std::env::var("GIT_BRANCH") {
        env.add("Git Branch", &branch);
    }

    let _ = env.write(results_dir);
}

fn scan_and_build_test_cases(base: &Path) -> Vec<GenreTestCase> {
    let mut cases = Vec::new();

    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read test directory: {}", e);
            return cases;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.is_dir() {
            let category = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let files = match fs::read_dir(&path) {
                Ok(files) => files,
                Err(_) => continue,
            };

            for file_entry in files {
                let file_entry = match file_entry {
                    Ok(f) => f,
                    Err(_) => continue,
                };

                let file_path = file_entry.path();
                if file_path.extension().and_then(|e| e.to_str()) != Some("flac") {
                    continue;
                }

                let filename = match file_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                let (should_pass, expected_defects) = categorize_expected_result(&category);
                let genre_info = extract_genre_from_filename(&filename);

                cases.push(GenreTestCase {
                    file_path: file_path.to_string_lossy().to_string(),
                    should_pass,
                    expected_defects,
                    description: filename.clone(),
                    genre: genre_info,
                    defect_category: category.clone(),
                });
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("flac") {
            let filename = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            let category = if let Some(pos) = filename.find("__") {
                filename[..pos].to_string()
            } else {
                filename.split('_').take(3).collect::<Vec<_>>().join("_")
            };

            let (should_pass, expected_defects) = categorize_expected_result(&category);
            let genre_info = extract_genre_from_filename(&filename);

            cases.push(GenreTestCase {
                file_path: path.to_string_lossy().to_string(),
                should_pass,
                expected_defects,
                description: filename.clone(),
                genre: genre_info,
                defect_category: category.clone(),
            });
        }
    }

    cases.sort_by(|a, b| {
        a.defect_category
            .cmp(&b.defect_category)
            .then(a.description.cmp(&b.description))
    });

    cases
}

fn categorize_expected_result(category: &str) -> (bool, Vec<String>) {
    match category {
        "Control_Original" => (true, vec![]),

        cat if cat.starts_with("MP3_") || cat.contains("MP3") => {
            (false, vec!["Mp3Transcode".to_string()])
        }

        cat if cat.starts_with("AAC_") => (false, vec!["AacTranscode".to_string()]),

        cat if cat.starts_with("Opus_") || cat.contains("Opus") => {
            (false, vec!["OpusTranscode".to_string()])
        }

        cat if cat.starts_with("Vorbis_") => (false, vec!["OggVorbisTranscode".to_string()]),

        "BitDepth_16to24" => (false, vec!["BitDepthMismatch".to_string()]),

        "Combined_16bit_44khz" => (
            false,
            vec!["BitDepthMismatch".to_string(), "Upsampled".to_string()],
        ),

        "Combined_MP3_128_From_CD" => (
            false,
            vec!["Mp3Transcode".to_string(), "BitDepthMismatch".to_string()],
        ),

        cat if cat.starts_with("SampleRate_") => (false, vec!["Upsampled".to_string()]),

        cat if cat.starts_with("Edge_") && cat.contains("Resample") => {
            (false, vec!["Upsampled".to_string()])
        }

        cat if cat.starts_with("Generation_") => {
            if cat.contains("MP3") {
                (false, vec!["Mp3Transcode".to_string()])
            } else if cat.contains("AAC") {
                (false, vec!["AacTranscode".to_string()])
            } else if cat.contains("Opus") {
                (false, vec!["OpusTranscode".to_string()])
            } else {
                (false, vec![])
            }
        }

        _ => (false, vec![]),
    }
}

fn extract_genre_from_filename(filename: &str) -> String {
    if filename.contains("Boogieman") {
        "HipHopRnB".to_string()
    } else if filename.contains("Paranoid_Android") || filename.contains("Instant_Destiny") {
        "Alternative".to_string()
    } else if filename.contains("inconsist") || filename.contains("An_Ending") {
        "AmbientDrone".to_string()
    } else if filename.contains("Different_Masks") || filename.contains("Windowlicker") {
        "ElectronicDance".to_string()
    } else if filename.contains("Could_You_Be_Loved") {
        "ReggaeDub".to_string()
    } else if filename.contains("MALAMENTE") || filename.contains("Chan_Chan") {
        "LatinWorld".to_string()
    } else if filename.contains("Wake_Up") || filename.contains("Punisher") {
        "Indie".to_string()
    } else if filename.contains("Pride_and_Joy") {
        "Blues".to_string()
    } else if filename.contains("Brandenburg") || filename.contains("Missa_Pange") {
        "Classical".to_string()
    } else if filename.contains("Dream_of_Arrakis") || filename.contains("Bene_Gesserit") {
        "SoundtrackScore".to_string()
    } else if filename.contains("Enter_Sandman") || filename.contains("Crack_the_Skye") {
        "Metal".to_string()
    } else if filename.contains("So_What") {
        "Jazz".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn count_categories(cases: &[GenreTestCase]) -> usize {
    let mut categories: HashSet<String> = HashSet::new();
    for case in cases {
        categories.insert(case.defect_category.clone());
    }
    categories.len()
}

fn run_tests_parallel(test_cases: Vec<GenreTestCase>, num_threads: usize) -> Vec<TestResult> {
    let test_cases = Arc::new(test_cases);
    let results = Arc::new(Mutex::new(Vec::new()));
    let index = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::new();

    println!("Running tests with {} parallel threads...\n", num_threads);

    for _ in 0..num_threads {
        let test_cases = Arc::clone(&test_cases);
        let results = Arc::clone(&results);
        let index = Arc::clone(&index);

        let handle = thread::spawn(move || loop {
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
            let result = run_single_test(test_case);

            if current_idx > 0 && current_idx % 10 == 0 {
                println!(
                    "Progress: {}/{} tests completed",
                    current_idx,
                    test_cases.len()
                );
            }

            let mut results_guard = results.lock().unwrap();
            results_guard.push((current_idx, result));
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

/// Parse defect identifiers and the genuine verdict from JSON stdout
/// produced by `--format both` / `--format json`.
///
/// Maps each `DefectType` variant to the same string identifiers used by
/// `categorize_expected_result` so the existing `validate_test_result` logic
/// needs no changes.
///
/// Mapping table:
///   LossyTranscode { codec: "MP3*"  }  → "Mp3Transcode"
///   LossyTranscode { codec: "AAC*"  }  → "AacTranscode"
///   LossyTranscode { codec: "OPUS*" }  → "OpusTranscode"
///   LossyTranscode { codec: "VORBIS*" | "OGG*" } → "OggVorbisTranscode"
///   LossyTranscode { codec: other   }  → "LossyTranscode"  (MFCC-only path)
///   Upsampled / ResamplingDetected     → "Upsampled"
///   BitDepthInflated                   → "BitDepthMismatch"
///   UpsampledLossyTranscode            → "Upsampled" + codec-specific transcode
///
/// Info-tier variants (DitheringDetected, MqaEncoded, SilencePadding) and
/// quality variants (Clipping, LoudnessWarVictim) are intentionally skipped —
/// they do not affect the genuine/suspect classification under test.
fn parse_defects_from_json(json_str: &str) -> (bool, Vec<String>) {
    let report: JsonReport = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: failed to parse JSON output: {}", e);
            // Treat as genuine/clean so the test surfaces as FalseNegative
            // (if a defect was expected) rather than silently passing.
            return (true, vec![]);
        }
    };

    let genuine = report.verdict.genuine;
    let mut defects: Vec<String> = Vec::new();

    for detection in &report.detections {
        // Skip info-level detections — dithering, MQA, etc. are not defects
        // under our qualification criteria.
        if detection.severity == "info" {
            continue;
        }

        let dt = &detection.defect_type;

        // Helper: extract inner object for a named variant
        // e.g. dt = { "LossyTranscode": { "codec": "MP3", ... } }
        if let Some(inner) = dt.get("LossyTranscode") {
            let codec = inner
                .get("codec")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();

            let label = if codec.contains("MP3") {
                "Mp3Transcode"
            } else if codec.contains("AAC") {
                "AacTranscode"
            } else if codec.contains("OPUS") {
                "OpusTranscode"
            } else if codec.contains("VORBIS") || codec.contains("OGG") {
                "OggVorbisTranscode"
            } else {
                // MFCC-based detection ("Unknown (cepstral analysis)")
                "LossyTranscode"
            };
            defects.push(label.to_string());
        } else if dt.get("Upsampled").is_some() || dt.get("ResamplingDetected").is_some() {
            // Both map to the same expected-defect string
            if !defects.contains(&"Upsampled".to_string()) {
                defects.push("Upsampled".to_string());
            }
        } else if dt.get("BitDepthInflated").is_some() {
            defects.push("BitDepthMismatch".to_string());
        } else if let Some(inner) = dt.get("UpsampledLossyTranscode") {
            // Composite: upsampled AND lossy — emit both defect strings
            if !defects.contains(&"Upsampled".to_string()) {
                defects.push("Upsampled".to_string());
            }
            let codec = inner
                .get("codec")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase();
            let label = if codec.contains("MP3") {
                "Mp3Transcode"
            } else if codec.contains("AAC") {
                "AacTranscode"
            } else if codec.contains("OPUS") {
                "OpusTranscode"
            } else if codec.contains("VORBIS") || codec.contains("OGG") {
                "OggVorbisTranscode"
            } else {
                "LossyTranscode"
            };
            defects.push(label.to_string());
        }
        // Clipping, LoudnessWarVictim, SilencePadding, DitheringDetected, MqaEncoded
        // → intentionally not mapped; not part of qualification defect vocabulary.
    }

    (genuine, defects)
}

/// Validate test results with STRICT defect type matching
///
/// Returns:
/// - Pass: Correct detection (clean==clean, or all expected defects found with no extras)
/// - PassWithWarning: Some expected defects found but some missing (partial match OK for fine-tuning)
/// - FalsePositive: Expected clean but got defective
/// - FalseNegative: Expected defective but got clean
/// - WrongDefectType: Expected specific defects but none of them were found
/// - ExtraDefects: Expected defects found but ALSO extra wrong defects detected (FAIL)
fn validate_test_result(
    is_clean: bool,
    should_pass: bool,
    expected_defects: &[String],
    defects_found: &[String],
) -> (ValidationResult, Vec<String>, Vec<String>) {
    let expected_set: HashSet<&String> = expected_defects.iter().collect();
    let found_set: HashSet<&String> = defects_found.iter().collect();

    // Calculate missing and extra defects
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

    // File is defective as expected, now check if correct defects were found

    // If no specific defects expected (just "should be defective"), any defect is OK
    if expected_defects.is_empty() {
        return (ValidationResult::Pass, missing, extra);
    }

    // Check if ANY expected defect was found
    let any_expected_found = expected_defects.iter().any(|d| found_set.contains(d));

    if !any_expected_found {
        return (ValidationResult::WrongDefectType, missing, extra);
    }

    // At least one expected defect was found
    if !extra.is_empty() {
        return (ValidationResult::ExtraDefects, missing, extra);
    }

    if missing.is_empty() {
        return (ValidationResult::Pass, missing, extra);
    } else {
        return (ValidationResult::PassWithWarning, missing, extra);
    }
}

fn run_single_test(test_case: &GenreTestCase) -> TestResult {
    let start = std::time::Instant::now();

    // --format both: Text → stderr (human-readable, captured for Allure)
    //                JSON → stdout (structured, parsed for defect extraction)
    let output = run_audiocheckr(&test_case.file_path)
        .arg("--sensitivity")
        .arg("high")
        .arg("--format")
        .arg("both")
        .output()
        .expect("Failed to execute binary");

    let duration_ms = start.elapsed().as_millis() as u64;

    let json_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let text_stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Parse defects and genuine verdict from structured JSON
    let (is_clean, defects_found) = parse_defects_from_json(&json_stdout);

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
        description: test_case.description.clone(),
        category: test_case.defect_category.clone(),
        file: test_case.file_path.clone(),
        genre: test_case.genre.clone(),
        stdout: text_stderr, // Human-readable text (stderr) — used by Allure attachment
        duration_ms,
    }
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

#[test]
fn test_binary_exists() {
    let binary_path = get_binary_path();
    assert!(
        binary_path.exists(),
        "Binary not found at {:?}",
        binary_path
    );
}
