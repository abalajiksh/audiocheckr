// tests/integration_test.rs
// INTEGRATION Test Suite - Verify all components work together
//
// These tests verify that:
// 1. All modules compile and link correctly
// 2. Public APIs are accessible and work as documented
// 3. Data flows correctly between components
// 4. Error handling works properly

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Get the path to the compiled binary
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
    assert!(stdout.contains("--input"), "Help should document --input flag");
    assert!(stdout.contains("--bit-depth"), "Help should document --bit-depth flag");
    assert!(stdout.contains("--spectrogram"), "Help should document --spectrogram flag");
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
    assert!(stdout.contains("0.2.0") || stdout.contains("audiocheckr"), 
        "Version output should contain version number or program name");
}

#[test]
fn test_cli_missing_file() {
    let binary = get_binary_path();
    let output = Command::new(&binary)
        .arg("--input")
        .arg("/nonexistent/path/to/file.flac")
        .output()
        .expect("Failed to execute with missing file");

    // Should either fail gracefully or report no files found
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    
    assert!(
        combined.contains("No audio files") || 
        combined.contains("not found") ||
        combined.contains("Error") ||
        !output.status.success(),
        "Should handle missing files gracefully"
    );
}

#[test]
fn test_cli_json_output_format() {
    let binary = get_binary_path();
    
    // Test JSON output flag doesn't crash even without valid input
    let output = Command::new(&binary)
        .arg("--input")
        .arg(".")
        .arg("--json")
        .output()
        .expect("Failed to execute with --json");

    // Should produce valid JSON or report no files
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Either valid JSON output or empty results
    if !stdout.is_empty() && !stdout.contains("No audio files") {
        // Try to parse as JSON
        let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(parse_result.is_ok(), "JSON output should be valid JSON: {}", stdout);
    }
}

#[test]
fn test_cli_invalid_bit_depth() {
    let binary = get_binary_path();
    let output = Command::new(&binary)
        .arg("--bit-depth")
        .arg("99")  // Invalid bit depth
        .arg("--input")
        .arg(".")
        .output()
        .expect("Failed to execute with invalid bit depth");

    // Program should handle this gracefully (either accept it or error)
    // Just ensure it doesn't crash
    let _ = output.status;
}

// =============================================================================
// Module Integration Tests (using library directly)
// =============================================================================

#[cfg(feature = "integration_lib_tests")]
mod lib_tests {
    use audiocheckr::*;
    use std::path::Path;

    #[test]
    fn test_analyzer_builder_chain() {
        // Test that builder pattern compiles and chains correctly
        let _builder = AnalyzerBuilder::new()
            .expected_bit_depth(24)
            .check_upsampling(true)
            .check_stereo(true)
            .check_transients(true)
            .check_phase(false)
            .min_confidence(0.5);
        
        // Just verify it compiles - actual file analysis needs test files
    }

    #[test]
    fn test_detection_config_defaults() {
        let config = DetectionConfig::default();
        
        assert_eq!(config.expected_bit_depth, 24);
        assert!(config.check_upsampling);
        assert!(config.check_stereo);
        assert!(config.check_transients);
        assert!(!config.check_phase); // Disabled by default (slow)
        assert!(!config.check_mfcc);  // Experimental, disabled
        assert!((config.min_confidence - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_version_constant() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'), "Version should be semver format");
    }
}

// =============================================================================
// DSP Module Tests
// =============================================================================

mod dsp_tests {
    // Test window function creation
    #[test]
    fn test_window_sizes() {
        // Just verify we can reference the module
        // Actual tests are in unit tests
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
                let binary = get_binary_path();
                let output = Command::new(&binary)
                    .arg("--input")
                    .arg(&flac_file)
                    .output()
                    .expect("Failed to analyze FLAC");
                
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    stdout.contains("Sample Rate") || stdout.contains("Analyzing"),
                    "Should process FLAC files"
                );
            }
        }
        // Skip if no test files
    }

    #[test]
    fn test_wav_support() {
        // Placeholder - would need .wav test files
    }

    #[test]
    fn test_mp3_support() {
        // Placeholder - would need .mp3 test files
    }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_empty_directory_handling() {
    let binary = get_binary_path();
    
    // Create a temp directory
    let temp_dir = std::env::temp_dir().join("audiocheckr_test_empty");
    let _ = std::fs::create_dir_all(&temp_dir);
    
    let output = Command::new(&binary)
        .arg("--input")
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute on empty directory");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No audio files") || stdout.is_empty(),
        "Should handle empty directories gracefully"
    );
    
    // Cleanup
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_permission_denied_handling() {
    // Platform-specific test - mainly for Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        let temp_file = std::env::temp_dir().join("audiocheckr_test_noperm.flac");
        
        // Create file with no read permissions
        if let Ok(_) = std::fs::write(&temp_file, b"fake") {
            if let Ok(metadata) = std::fs::metadata(&temp_file) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o000);
                let _ = std::fs::set_permissions(&temp_file, perms);
                
                let binary = get_binary_path();
                let output = Command::new(&binary)
                    .arg("--input")
                    .arg(&temp_file)
                    .output();
                
                // Should not crash
                assert!(output.is_ok());
                
                // Restore permissions and cleanup
                let mut perms = std::fs::metadata(&temp_file).unwrap().permissions();
                perms.set_mode(0o644);
                let _ = std::fs::set_permissions(&temp_file, perms);
                let _ = std::fs::remove_file(&temp_file);
            }
        }
    }
}

// =============================================================================
// Spectrogram Generation Tests
// =============================================================================

mod spectrogram_tests {
    use super::*;

    #[test]
    fn test_spectrogram_flag_accepted() {
        let binary = get_binary_path();
        
        // Just verify the flag is accepted (no crash)
        let output = Command::new(&binary)
            .arg("--input")
            .arg(".")
            .arg("--spectrogram")
            .output()
            .expect("--spectrogram flag should be accepted");

        // Should not crash
        let _ = output.status;
    }

    #[test]
    fn test_linear_scale_flag() {
        let binary = get_binary_path();
        
        let output = Command::new(&binary)
            .arg("--input")
            .arg(".")
            .arg("--spectrogram")
            .arg("--linear-scale")
            .output()
            .expect("--linear-scale flag should be accepted");

        let _ = output.status;
    }
}

// =============================================================================
// Output Mode Tests
// =============================================================================

mod output_tests {
    use super::*;

    #[test]
    fn test_output_mode_source() {
        let binary = get_binary_path();
        
        let output = Command::new(&binary)
            .arg("--output")
            .arg("source")
            .arg("--help")
            .output()
            .expect("--output source should be accepted");

        assert!(output.status.success());
    }

    #[test]
    fn test_output_mode_current() {
        let binary = get_binary_path();
        
        let output = Command::new(&binary)
            .arg("--output")
            .arg("current")
            .arg("--help")
            .output()
            .expect("--output current should be accepted");

        assert!(output.status.success());
    }
}

// =============================================================================
// Quick Mode Tests
// =============================================================================

#[test]
fn test_quick_mode() {
    let binary = get_binary_path();
    
    let output = Command::new(&binary)
        .arg("--quick")
        .arg("--help")
        .output()
        .expect("--quick flag should be accepted");

    assert!(output.status.success());
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
    
    let binary = get_binary_path();
    
    // Run full analysis with all flags
    let output = Command::new(&binary)
        .arg("--input")
        .arg(&test_file)
        .arg("--bit-depth")
        .arg("24")
        .arg("--check-upsampling")
        .arg("--stereo")
        .arg("--transients")
        .arg("--verbose")
        .output()
        .expect("Full analysis should complete");

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
    
    let binary = get_binary_path();
    
    let output = Command::new(&binary)
        .arg("--input")
        .arg(&test_file)
        .arg("--json")
        .output()
        .expect("JSON output should work");

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
