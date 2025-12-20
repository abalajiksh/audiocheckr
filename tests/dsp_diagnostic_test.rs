// tests/dsp_diagnostic_test.rs
//
// DSP Diagnostic Test Suite for AudioCheckr
//
// This test provides detailed diagnostic output showing exactly what each
// detector sees when analyzing DSP-processed files (dithered, resampled).
//
// Purpose:
// - Diagnose why detectors fire incorrectly on DSP files
// - Show spectral characteristics of dithered/resampled audio
// - Identify threshold and algorithm issues
// - Generate detailed analysis reports for debugging
//
// Usage:
//   cargo test --test dsp_diagnostic_test -- --nocapture
//   cargo test --test dsp_diagnostic_test diagnostic_single_file -- --nocapture

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// ============================================================================
// Configuration
// ============================================================================

const DITHERING_TEST_DIR: &str = "dithering_tests";
const RESAMPLING_TEST_DIR: &str = "resampling_tests";
const DIAGNOSTIC_OUTPUT_DIR: &str = "target/dsp-diagnostics";

// ============================================================================
// Diagnostic Test Entry Points
// ============================================================================

/// Run diagnostic on a single dithering file
#[test]
fn diagnostic_dithering_sample() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dither_dir = project_root.join(DITHERING_TEST_DIR);
    
    if !dither_dir.exists() {
        println!("Dithering test directory not found: {}", dither_dir.display());
        println!("Skipping diagnostic (download test files first)");
        return;
    }
    
    // Test control file first
    let control = dither_dir.join("input176.flac");
    if control.exists() {
        println!("\n{}", "=".repeat(80));
        println!("CONTROL FILE DIAGNOSTIC");
        println!("{}\n", "=".repeat(80));
        run_verbose_analysis(&control);
    }
    
    // Test one dithered file
    let sample_files = [
        "output_triangular_scale1_0.flac",
        "output_shibata_scale1_0.flac",
        "output_rectangular_scale1_0.flac",
    ];
    
    for filename in &sample_files {
        let path = dither_dir.join(filename);
        if path.exists() {
            println!("\n{}", "=".repeat(80));
            println!("DITHERED FILE DIAGNOSTIC: {}", filename);
            println!("{}\n", "=".repeat(80));
            run_verbose_analysis(&path);
        }
    }
}

/// Run diagnostic on a single resampling file  
#[test]
fn diagnostic_resampling_sample() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resample_dir = project_root.join(RESAMPLING_TEST_DIR);
    
    if !resample_dir.exists() {
        println!("Resampling test directory not found: {}", resample_dir.display());
        println!("Skipping diagnostic (download test files first)");
        return;
    }
    
    // Test control file first
    let control = resample_dir.join("input176.flac");
    if control.exists() {
        println!("\n{}", "=".repeat(80));
        println!("CONTROL FILE DIAGNOSTIC");
        println!("{}\n", "=".repeat(80));
        run_verbose_analysis(&control);
    }
    
    // Test various resampled files
    let sample_files = [
        // SoXR (false negatives in tests)
        "output_44100Hz_soxr_default.flac",
        "output_96000Hz_soxr_vhq.flac",
        // SWR (wrong defect type in tests)
        "output_44100Hz_swr_default.flac",
        "output_192000Hz_swr_default.flac",  // Upsampling
    ];
    
    for filename in &sample_files {
        let path = resample_dir.join(filename);
        if path.exists() {
            println!("\n{}", "=".repeat(80));
            println!("RESAMPLED FILE DIAGNOSTIC: {}", filename);
            println!("{}\n", "=".repeat(80));
            run_verbose_analysis(&path);
        }
    }
}

/// Full diagnostic on all dithering files (slower)
#[test]
#[ignore] // Run with: cargo test diagnostic_all_dithering -- --ignored --nocapture
fn diagnostic_all_dithering() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dither_dir = project_root.join(DITHERING_TEST_DIR);
    let output_dir = project_root.join(DIAGNOSTIC_OUTPUT_DIR);
    
    if !dither_dir.exists() {
        println!("Dithering test directory not found");
        return;
    }
    
    std::fs::create_dir_all(&output_dir).ok();
    
    let mut report = String::new();
    report.push_str("# Dithering Detection Diagnostic Report\n\n");
    
    let entries: Vec<_> = std::fs::read_dir(&dither_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("flac"))
        .collect();
    
    println!("Analyzing {} dithering test files...\n", entries.len());
    
    for entry in entries {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        
        println!("[*] Analyzing: {}", filename);
        
        let analysis = run_diagnostic_analysis(&path);
        
        report.push_str(&format!("## {}\n\n", filename));
        report.push_str(&format!("```\n{}\n```\n\n", analysis));
    }
    
    let report_path = output_dir.join("dithering_diagnostic.md");
    std::fs::write(&report_path, report).expect("Failed to write report");
    println!("\nReport written to: {}", report_path.display());
}

/// Full diagnostic on all resampling files (slower)
#[test]
#[ignore] // Run with: cargo test diagnostic_all_resampling -- --ignored --nocapture
fn diagnostic_all_resampling() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resample_dir = project_root.join(RESAMPLING_TEST_DIR);
    let output_dir = project_root.join(DIAGNOSTIC_OUTPUT_DIR);
    
    if !resample_dir.exists() {
        println!("Resampling test directory not found");
        return;
    }
    
    std::fs::create_dir_all(&output_dir).ok();
    
    let mut report = String::new();
    report.push_str("# Resampling Detection Diagnostic Report\n\n");
    
    let entries: Vec<_> = std::fs::read_dir(&resample_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("flac"))
        .collect();
    
    println!("Analyzing {} resampling test files...\n", entries.len());
    
    for entry in entries {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        
        println!("[*] Analyzing: {}", filename);
        
        let analysis = run_diagnostic_analysis(&path);
        
        report.push_str(&format!("## {}\n\n", filename));
        report.push_str(&format!("```\n{}\n```\n\n", analysis));
    }
    
    let report_path = output_dir.join("resampling_diagnostic.md");
    std::fs::write(&report_path, report).expect("Failed to write report");
    println!("\nReport written to: {}", report_path.display());
}

// ============================================================================
// Core Diagnostic Functions
// ============================================================================

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

fn run_verbose_analysis(file_path: &Path) {
    let binary = get_binary_path();
    
    let output = Command::new(&binary)
        .arg("--input")
        .arg(file_path)
        .arg("--bit-depth")
        .arg("24")
        .arg("--check-upsampling")
        .arg("--verbose")
        .output()
        .expect("Failed to execute audiocheckr");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("{}", stdout);
    
    if !stderr.is_empty() {
        println!("\n--- STDERR ---");
        println!("{}", stderr);
    }
    
    // Parse and highlight key diagnostic info
    println!("\n--- DIAGNOSTIC SUMMARY ---");
    analyze_output(&stdout);
}

fn run_diagnostic_analysis(file_path: &Path) -> String {
    let binary = get_binary_path();
    
    let output = Command::new(&binary)
        .arg("--input")
        .arg(file_path)
        .arg("--bit-depth")
        .arg("24")
        .arg("--check-upsampling")
        .arg("--verbose")
        .output()
        .expect("Failed to execute audiocheckr");
    
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn analyze_output(stdout: &str) {
    let stdout_lower = stdout.to_lowercase();
    
    // Check for each detector type
    let detectors = [
        ("Mp3Transcode", "mp3 transcode", "❌ FALSE POSITIVE - MP3 detector should not fire on DSP files"),
        ("AacTranscode", "aac transcode", "❌ FALSE POSITIVE - AAC detector should not fire on DSP files"),
        ("OggVorbisTranscode", "ogg vorbis transcode", "❌ FALSE POSITIVE - Vorbis detector should not fire on DSP files"),
        ("DitheringDetected", "dithering detected", "✓ Expected for dithered files"),
        ("ResamplingDetected", "resampling detected", "✓ Expected for resampled files"),
        ("BitDepthMismatch", "bit depth mismatch", "⚠ May be expected for dithered files"),
        ("Upsampled", "upsampled", "✓ Expected for upsampled files"),
    ];
    
    println!("Detected defects:");
    let mut any_detected = false;
    
    for (name, pattern, note) in &detectors {
        if stdout_lower.contains(pattern) {
            println!("  • {} - {}", name, note);
            any_detected = true;
        }
    }
    
    if !any_detected {
        println!("  (none detected)");
    }
    
    // Extract key metrics
    println!("\nKey metrics:");
    
    // Sample rate
    if let Some(line) = stdout.lines().find(|l| l.contains("Sample Rate:")) {
        println!("  {}", line.trim());
    }
    
    // Bit depth
    if let Some(line) = stdout.lines().find(|l| l.contains("Bit Depth:")) {
        println!("  {}", line.trim());
    }
    
    // Frequency cutoff
    if let Some(line) = stdout.lines().find(|l| l.contains("Frequency Cutoff:")) {
        println!("  {}", line.trim());
    }
    
    // Quality score
    if let Some(line) = stdout.lines().find(|l| l.contains("Quality Score:")) {
        println!("  {}", line.trim());
    }
    
    // Spectral rolloff
    if let Some(line) = stdout.lines().find(|l| l.contains("Spectral rolloff:")) {
        println!("  {}", line.trim());
    }
    
    // Rolloff steepness
    if let Some(line) = stdout.lines().find(|l| l.contains("Rolloff steepness:")) {
        println!("  {}", line.trim());
    }
    
    // Brick-wall detection
    if let Some(line) = stdout.lines().find(|l| l.contains("Brick-wall")) {
        println!("  {}", line.trim());
    }
    
    // Status
    if let Some(line) = stdout.lines().find(|l| l.contains("Status:")) {
        println!("\n{}", line.trim());
    }
}

// ============================================================================
// Specific Diagnostic Tests
// ============================================================================

/// Test that helps identify why MP3 detector fires on high-res files
#[test]
fn diagnostic_mp3_false_positive_analysis() {
    println!("\n{}", "=".repeat(80));
    println!("MP3 FALSE POSITIVE ANALYSIS");
    println!("{}", "=".repeat(80));
    println!("\nThis test analyzes why the MP3 transcode detector might fire on DSP files.");
    println!("\nPotential causes:");
    println!("1. Spectral rolloff from anti-aliasing filters misinterpreted as MP3 lowpass");
    println!("2. Noise shaping from dithering creating spectral characteristics similar to lossy");
    println!("3. No sample-rate awareness (176.4kHz cannot be MP3 source)");
    println!("4. Overly aggressive thresholds in spectral analysis");
    println!("\nRecommended fixes:");
    println!("1. Add sample-rate checks: if sample_rate > 48000, skip MP3 detection");
    println!("2. Distinguish anti-aliasing rolloff from lossy codec rolloff");
    println!("3. Check for DSP artifacts BEFORE lossy codec detection");
    println!("4. Adjust rolloff steepness thresholds for hi-res content");
    
    // Run on control file to see what triggers false positive
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let control = project_root.join(DITHERING_TEST_DIR).join("input176.flac");
    
    if control.exists() {
        println!("\n--- Control file analysis ---\n");
        run_verbose_analysis(&control);
    } else {
        println!("\nControl file not found. Download test files first.");
    }
}

/// Test spectral characteristics at various sample rates
#[test]
fn diagnostic_sample_rate_awareness() {
    println!("\n{}", "=".repeat(80));
    println!("SAMPLE RATE AWARENESS TEST");
    println!("{}", "=".repeat(80));
    
    println!("\nMP3/AAC/Vorbis maximum sample rates:");
    println!("  MP3: 48 kHz maximum");
    println!("  AAC: 96 kHz maximum (typically 48 kHz)");
    println!("  Vorbis: 192 kHz (rare above 48 kHz)");
    
    println!("\nTest file sample rates:");
    println!("  Original: 176.4 kHz (24-bit)");
    println!("  → Cannot be direct MP3 source (would need downsample first)");
    println!("  → Downsampled to 44.1/48/88.2/96 kHz could theoretically be from MP3");
    println!("  → Upsampled to 192 kHz cannot be direct MP3 source");
    
    println!("\nRecommendation:");
    println!("  For files > 48 kHz: Skip direct MP3/AAC detection");
    println!("  For files > 96 kHz: Skip direct Vorbis detection");
    println!("  Instead, check for upsampling from lower rate");
}

/// Compare spectral signatures of different dithering algorithms
#[test]
#[ignore]
fn diagnostic_dither_spectral_signatures() {
    println!("\n{}", "=".repeat(80));
    println!("DITHERING SPECTRAL SIGNATURE COMPARISON");
    println!("{}", "=".repeat(80));
    
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dither_dir = project_root.join(DITHERING_TEST_DIR);
    
    if !dither_dir.exists() {
        println!("Dithering test directory not found");
        return;
    }
    
    let algorithms = [
        ("rectangular", "Flat spectrum, uniform noise"),
        ("triangular", "Triangular PDF, no shaping"),
        ("triangular_hp", "Triangular + high-pass filter"),
        ("shibata", "Psychoacoustic noise shaping"),
        ("lipshitz", "Minimized audibility shaping"),
        ("f_weighted", "Frequency-weighted shaping"),
    ];
    
    for (algo, description) in &algorithms {
        let filename = format!("output_{}_scale1_0.flac", algo);
        let path = dither_dir.join(&filename);
        
        if path.exists() {
            println!("\n--- {} ---", algo.to_uppercase());
            println!("Description: {}", description);
            println!();
            run_verbose_analysis(&path);
        }
    }
}

/// Compare spectral signatures of different resamplers
#[test]
#[ignore]
fn diagnostic_resampler_spectral_signatures() {
    println!("\n{}", "=".repeat(80));
    println!("RESAMPLER SPECTRAL SIGNATURE COMPARISON");
    println!("{}", "=".repeat(80));
    
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resample_dir = project_root.join(RESAMPLING_TEST_DIR);
    
    if !resample_dir.exists() {
        println!("Resampling test directory not found");
        return;
    }
    
    let resamplers = [
        ("soxr_default", "SoXR default quality"),
        ("soxr_vhq", "SoXR very high quality"),
        ("soxr_cutoff_91", "SoXR with 91% passband"),
        ("swr_default", "FFmpeg SWR default"),
        ("swr_blackman_nuttall", "SWR Blackman-Nuttall window"),
        ("swr_kaiser_beta12", "SWR Kaiser β=12"),
    ];
    
    // Test at 44.1kHz (common target)
    for (resampler, description) in &resamplers {
        let filename = format!("output_44100Hz_{}.flac", resampler);
        let path = resample_dir.join(&filename);
        
        if path.exists() {
            println!("\n--- {} (44.1kHz) ---", resampler.to_uppercase());
            println!("Description: {}", description);
            println!();
            run_verbose_analysis(&path);
        }
    }
}

// ============================================================================
// Unit Tests for Diagnostic Utilities
// ============================================================================

#[test]
fn test_binary_exists() {
    let binary = get_binary_path();
    assert!(binary.exists(), "audiocheckr binary not found at {:?}", binary);
}
