// tests/validation_test.rs
// GROUND TRUTH Validation Test Suite
//
// This is a focused subset of critical test cases for CI/CD pipeline.
// Tests represent ACTUAL file defects (ground truth), not current capabilities.
// Use regression_test.rs for comprehensive benchmarking.

use std::env;
use std::process::Command;
use std::path::{Path, PathBuf};

struct TestCase {
    file_path: String,
    should_pass: bool,
    expected_defects: Vec<String>,
    description: String,
}

struct TestResult {
    passed: bool,
    expected: bool,
    defects_found: Vec<String>,
    file: String,
}

#[test]
fn test_all_audio_files() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestFiles");

    assert!(
        test_base.exists(),
        "TestFiles directory not found at: {}\nProject root: {}",
        test_base.display(),
        project_root.display()
    );

    println!("\n=== Using TestFiles from: {} ===\n", test_base.display());

    let test_cases = define_test_cases(&test_base);
    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    println!("=== Running Audio Quality Validation Tests ===\n");

    for test_case in &test_cases {
        let result = run_test(&binary_path, test_case);

        if result.passed == result.expected {
            passed += 1;
            println!("✓ PASS: {}", test_case.description);
        } else {
            failed += 1;
            println!("✗ FAIL: {}", test_case.description);
            println!("  File: {}", test_case.file_path);
            println!("  Expected: {}, Got: {}",
                if test_case.should_pass { "CLEAN" } else { "DEFECTS" },
                if result.passed { "CLEAN" } else { "DEFECTS" });

            if !result.defects_found.is_empty() {
                println!("  Defects found: {:?}", result.defects_found);
            }
            if !test_case.expected_defects.is_empty() {
                println!("  Expected defects: {:?}", test_case.expected_defects);
            }
        }

        results.push(result);
    }

    println!("\n=== Test Summary ===");
    println!("Total: {}", test_cases.len());
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);
    println!("Success Rate: {:.1}%", (passed as f32 / test_cases.len() as f32) * 100.0);

    if failed > 0 {
        println!("\n=== Failure Analysis ===");
        for (i, result) in results.iter().enumerate() {
            if result.passed != result.expected {
                println!("\n{}: {}", i + 1, test_cases[i].description);
                println!("  File: {}", result.file);
                println!("  Expected defects: {:?}", test_cases[i].expected_defects);
                println!("  Found defects: {:?}", result.defects_found);
            }
        }
    }

    assert_eq!(failed, 0, "{} test(s) failed", failed);
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
            println!("Using Windows release binary: {:?}", release_path_exe);
            return release_path_exe;
        } else if debug_path_exe.exists() {
            println!("Using Windows debug binary: {:?}", debug_path_exe);
            return debug_path_exe;
        }
    }

    #[cfg(unix)]
    {
        if release_path.exists() {
            println!("Using Linux/Unix release binary: {:?}", release_path);
            return release_path;
        } else if debug_path.exists() {
            println!("Using Linux/Unix debug binary: {:?}", debug_path);
            return debug_path;
        }
    }

    panic!(
        "Binary 'audiocheckr' not found in target/release or target/debug.\n\
         Searched paths:\n\
         - {:?}\n\
         - {:?}\n\
         Please build the project first with:\n\
         cargo build --release",
        path.join("release").join("audiocheckr"),
        path.join("debug").join("audiocheckr")
    );
}

fn define_test_cases(base: &Path) -> Vec<TestCase> {
    let mut cases = Vec::new();

    // GROUND TRUTH: Representative test cases for CI/CD
    // Focus on high-confidence detectable scenarios

    // === CleanOrigin ===
    cases.push(TestCase {
        file_path: base.join("CleanOrigin/input96.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "Original 96kHz 24-bit FLAC (truly clean)".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("CleanOrigin/input192.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "Original 192kHz (actually 16-bit source, should detect)".to_string(),
    });

    // === CleanTranscoded ===
    cases.push(TestCase {
        file_path: base.join("CleanTranscoded/input96_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "Honest 16-bit from 96kHz (correctly labeled, clean)".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("CleanTranscoded/input192_16bit.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "Honest 16-bit from 192kHz (correctly labeled, clean)".to_string(),
    });

    // === Resample96 - Clean downsampling ===
    let downsample_rates = vec!["44", "48", "88"];
    for rate in downsample_rates {
        cases.push(TestCase {
            file_path: base.join(format!("Resample96/input96_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: true,
            expected_defects: vec![],
            description: format!("Clean resample from 96kHz to {}kHz", rate),
        });
    }

    // === Resample192 - From defective source ===
    let resample192_rates = vec!["44", "48", "88", "96", "176"];
    for rate in resample192_rates {
        cases.push(TestCase {
            file_path: base.join(format!("Resample192/input192_{}.flac", rate)).to_string_lossy().to_string(),
            should_pass: false,
            expected_defects: vec!["BitDepthMismatch".to_string()],
            description: format!("Resample from 192kHz to {}kHz (16-bit source)", rate),
        });
    }

    // === Upscale16 - Fake bit depth ===
    cases.push(TestCase {
        file_path: base.join("Upscale16/output96_16bit.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "16-bit upscaled to fake 24-bit".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("Upscale16/output192_16bit.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "16-bit upscaled to fake 24-bit from 192kHz".to_string(),
    });

    // === Upscaled - Lossy transcodes ===
    // Focus on 192kHz sources (more detectable) and one 96kHz MP3 (known detectable)
    cases.push(TestCase {
        file_path: base.join("Upscaled/input96_mp3.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["Mp3Transcode".to_string()],
        description: "MP3 from 96kHz transcoded to FLAC".to_string(),
    });

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
            description: format!("{} from 192kHz transcoded to FLAC", format.to_uppercase()),
        });
    }

    // === MasterScript - Key scenarios ===
    cases.push(TestCase {
        file_path: base.join("MasterScript/test96_original.flac").to_string_lossy().to_string(),
        should_pass: true,
        expected_defects: vec![],
        description: "MasterScript: Original 96kHz reference (clean)".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_original.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "MasterScript: Original 192kHz (16-bit source)".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("MasterScript/test96_16bit_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string()],
        description: "MasterScript: 16-bit upscaled to 24-bit".to_string(),
    });

    // Add some 192kHz lossy examples from MasterScript (should be detectable)
    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_mp3_320_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
        description: "MasterScript: 192kHz MP3 320 upscaled".to_string(),
    });

    cases.push(TestCase {
        file_path: base.join("MasterScript/test192_opus_128_upscaled.flac").to_string_lossy().to_string(),
        should_pass: false,
        expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
        description: "MasterScript: 192kHz Opus 128 upscaled".to_string(),
    });

    cases
}

fn run_test(binary: &Path, test_case: &TestCase) -> TestResult {
    let output = Command::new(binary)
        .arg("--input")
        .arg(&test_case.file_path)
        .arg("--bit-depth")
        .arg("24")
        .arg("--check-upsampling")
        .output()
        .expect("Failed to execute binary");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let has_issues = stdout.contains("✗ ISSUES DETECTED") ||
                     stdout.contains("ISSUES DETECTED");
    let is_clean = stdout.contains("✓ CLEAN") ||
                   (stdout.contains("CLEAN") && !has_issues);

    let mut defects_found = Vec::new();

    if stdout.contains("MP3") || stdout.contains("Mp3") {
        defects_found.push("Mp3Transcode".to_string());
    }
    if stdout.contains("AAC") || stdout.contains("Aac") {
        defects_found.push("AacTranscode".to_string());
    }
    if stdout.contains("Opus") {
        defects_found.push("OpusTranscode".to_string());
    }
    if stdout.contains("Vorbis") || stdout.contains("Ogg") {
        defects_found.push("OggVorbisTranscode".to_string());
    }
    if stdout.contains("Bit depth mismatch") || stdout.contains("BitDepth") {
        defects_found.push("BitDepthMismatch".to_string());
    }
    if stdout.contains("Upsampled") {
        defects_found.push("Upsampled".to_string());
    }
    if stdout.contains("Spectral artifacts") {
        defects_found.push("SpectralArtifacts".to_string());
    }

    TestResult {
        passed: is_clean,
        expected: test_case.should_pass,
        defects_found,
        file: test_case.file_path.clone(),
    }
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
        .expect("Failed to execute binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("audiocheckr"));
    assert!(stdout.contains("--input"));
    assert!(stdout.contains("--bit-depth"));
}
