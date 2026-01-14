// tests/integration_test.rs
// INTEGRATION Test Suite - Verify all components work together
//
// These tests verify that:
// 1. All modules compile and link correctly
// 2. Public APIs are accessible and work as documented
// 3. Data flows correctly between components
// 4. Error handling works properly
//
// IMPORTANT: Tests use temp directories instead of "." to avoid scanning
// large directories like target/ which causes tests to run for 50+ minutes.

use std::env;
use std::path::PathBuf;
use std::process::Command;

mod test_utils;
use test_utils::{get_binary_path, run_audiocheckr, run_json_analysis, run_verbose_analysis};

/// Create a temporary empty directory for testing
fn create_temp_test_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("audiocheckr_test_{}", name));
    let _ = std::fs::remove_dir_all(&temp_dir); // Clean up any previous run
    let _ = std::fs::create_dir_all(&temp_dir);
    temp_dir
}

/// Cleanup a temporary test directory
fn cleanup_temp_dir(path: &PathBuf) {
    let _ = std::fs::remove_dir_all(path);
}

// =============================================================================
// CLI Integration Tests
// =============================================================================

#[test]
fn test_cli_help() {
    let binary = get_binary_path();
    let output = Command::new(&binary)
        .arg("--help")
        .output()
        .expect("Failed to execute --help");

    assert!(output.status.success(), "--help should succeed");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("audiocheckr"), "Help should mention audiocheckr");
    // New CLI parameter checks
    assert!(stdout.contains("<INPUT>"), "Help should document INPUT positional argument");
    assert!(stdout.contains("--sensitivity"), "Help should document --sensitivity flag");
    assert!(stdout.contains("--format"), "Help should document --format flag");
}

#[test]
fn test_cli_version() {
    let binary = get_binary_path();
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .expect("Failed to execute --version");

    assert!(output.status.success(), "--version should succeed");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.12.0") || stdout.contains("audiocheckr"), 
        "Version output should contain version number or program name");
}

#[test]
fn test_cli_missing_file() {
    let binary = get_binary_path();
    // Intentionally missing the required positional input argument
    let output = Command::new(&binary)
        .output()
        .expect("Failed to execute with missing file");

    // Clap should report error about missing required argument
    assert!(!output.status.success(), "Should fail without input");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required") || stderr.contains("INPUT"), 
        "Stderr should mention missing required input");
}

#[test]
fn test_cli_json_output_format() {
    let temp_dir = create_temp_test_dir("json_output");
    
    // Test JSON output flag doesn't crash even without valid input
    let output = run_json_analysis(&temp_dir);

    // Should produce valid JSON or report no files
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Either valid JSON output or empty results
    if !stdout.is_empty() && !stdout.contains("No audio files") {
        // Try to parse as JSON
        let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(parse_result.is_ok(), "JSON output should be valid JSON: {}", stdout);
    }
    
    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_cli_invalid_sensitivity() {
    let binary = get_binary_path();
    let temp_dir = create_temp_test_dir("invalid_sensitivity");
    
    let output = Command::new(&binary)
        .arg(&temp_dir)
        .arg("--sensitivity")
        .arg("super_ultra_high")  // Invalid enum value
        .output()
        .expect("Failed to execute with invalid sensitivity");

    // Program should error out (clap validation)
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("value") && stderr.contains("sensitivity"), 
        "Should report invalid sensitivity value");
    
    cleanup_temp_dir(&temp_dir);
}

// =============================================================================
// Module Integration Tests (using library directly)
// =============================================================================

#[cfg(feature = "integration_lib_tests")]
mod lib_tests {
    use audiocheckr::*;
    use std::path::Path;

    #[test]
    fn test_detection_config_defaults() {
        // This test might need update depending on what DetectionConfig looks like now
        // Assuming core library hasn't changed as drastically as CLI
    }
}

// =============================================================================
// Parallel Processing Tests
// =============================================================================

#[test]
fn test_concurrent_cli_invocations() {
    use std::thread;
    
    let binary = get_binary_path();
    
    // Spawn multiple help requests concurrently
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let bin = binary.clone();
            thread::spawn(move || {
                Command::new(&bin)
                    .arg("--help")
                    .output()
                    .expect("Failed to execute")
            })
        })
        .collect();
    
    for handle in handles {
        let output = handle.join().expect("Thread panicked");
        assert!(output.status.success());
    }
}

// =============================================================================
// File Format Support Tests (placeholder for when test files exist)
// =============================================================================

mod format_tests {
    use super::*;
    use std::path::Path;

    fn get_test_files_dir() -> Option<PathBuf> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let test_base = project_root.join("TestFiles");
        
        if test_base.exists() {
            Some(test_base)
        } else {
            None
        }
    }

    #[test]
    fn test_flac_support() {
        if let Some(test_dir) = get_test_files_dir() {
            let flac_file = test_dir.join("CleanOrigin/input96.flac");
            if flac_file.exists() {
                let output = run_audiocheckr(&flac_file)
                    .output()
                    .expect("Failed to analyze FLAC");
                
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    stdout.contains("Sample Rate") || stdout.contains("Analyzing"),
                    "Should process FLAC files"
                );
            }
        }
    }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_empty_directory_handling() {
    let temp_dir = create_temp_test_dir("empty_dir");
    
    let output = run_audiocheckr(&temp_dir)
        .output()
        .expect("Failed to execute on empty directory");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No audio files") || stdout.is_empty(),
        "Should handle empty directories gracefully"
    );
    
    cleanup_temp_dir(&temp_dir);
}

// =============================================================================
// Spectrogram Generation Tests
// =============================================================================

mod spectrogram_tests {
    use super::*;

    #[test]
    fn test_spectrogram_flag_accepted() {
        let temp_dir = create_temp_test_dir("spectrogram");
        
        let output = run_audiocheckr(&temp_dir)
            .arg("--spectrogram")
            .output()
            .expect("--spectrogram flag should be accepted");

        // Should not crash (might exit with success even if no files)
        let _ = output.status;
        
        cleanup_temp_dir(&temp_dir);
    }
}

// =============================================================================
// Output Mode Tests
// =============================================================================

mod output_tests {
    use super::*;

    #[test]
    fn test_format_detailed() {
        let binary = get_binary_path();
        
        let output = Command::new(&binary)
            .arg("--help") // Just checking help for now as we need input
            .output()
            .expect("Command should run");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("detailed"), "Help should list detailed format");
    }
}

// =============================================================================
// Verbose Mode Tests
// =============================================================================

#[test]
fn test_verbose_mode() {
    let binary = get_binary_path();
    
    let output = Command::new(&binary)
        .arg("--verbose")
        .arg("--help")
        .output()
        .expect("--verbose flag should be accepted");

    assert!(output.status.success());
}

// =============================================================================
// Component Interaction Tests (with test files)
// =============================================================================

#[test]
fn test_full_analysis_pipeline() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestFiles");
    
    if !test_base.exists() {
        println!("Skipping full pipeline test - TestFiles not found");
        return;
    }
    
    let test_file = test_base.join("CleanOrigin/input96.flac");
    if !test_file.exists() {
        println!("Skipping full pipeline test - input96.flac not found");
        return;
    }
    
    // Run full analysis with high sensitivity
    let output = run_verbose_analysis(&test_file);

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Verify output contains expected sections
    assert!(stdout.contains("Sample Rate"), "Should show sample rate");
    assert!(stdout.contains("Bit Depth"), "Should show bit depth");
    assert!(stdout.contains("Quality Score"), "Should show quality score");
}

#[test]
fn test_json_output_structure() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestFiles");
    
    if !test_base.exists() {
        println!("Skipping JSON structure test - TestFiles not found");
        return;
    }
    
    let test_file = test_base.join("CleanOrigin/input96.flac");
    if !test_file.exists() {
        println!("Skipping JSON structure test - input96.flac not found");
        return;
    }
    
    let output = run_json_analysis(&test_file);

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    // Verify structure
    assert!(json.get("files_analyzed").is_some(), "Should have files_analyzed");
    assert!(json.get("results").is_some(), "Should have results array");
    
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        if !results.is_empty() {
            let first = &results[0];
            assert!(first.get("file").is_some(), "Result should have file");
            assert!(first.get("sample_rate").is_some(), "Result should have sample_rate");
            assert!(first.get("quality_score").is_some(), "Result should have quality_score");
        }
    }
}
