// tests/diagnostic_test.rs
// DIAGNOSTIC Test - Analyze spectral characteristics of control files
// to understand why false positives are occurring
//
// Run with: cargo test --test diagnostic_test -- --nocapture

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

// We'll call the binary with verbose output and parse the results
use std::process::Command;

#[derive(Debug, Clone)]
struct DiagnosticResult {
    filename: String,
    sample_rate: u32,
    bit_depth: u32,
    frequency_cutoff: f32,
    nyquist: f32,
    cutoff_ratio: f32,
    rolloff_steepness: f32,
    has_brick_wall: bool,
    spectral_flatness: f32,
    quality_score: f32,
    detected_defects: Vec<String>,
    is_false_positive: bool,
}

#[test]
fn diagnose_control_files() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestSuite").join("Control_Original");
    
    if !test_base.exists() {
        println!("TestSuite/Control_Original not found. Skipping diagnostic.");
        return;
    }
    
    println!("\n{}", "=".repeat(100));
    println!("DIAGNOSTIC ANALYSIS: Control_Original Files");
    println!("Purpose: Understand why genuine files are being flagged as transcodes");
    println!("{}\n", "=".repeat(100));
    
    let mut results: Vec<DiagnosticResult> = Vec::new();
    
    // Scan all FLAC files in Control_Original
    let entries = fs::read_dir(&test_base).expect("Failed to read Control_Original directory");
    
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("flac") {
            continue;
        }
        
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        
        // Run with verbose output
        let output = Command::new(&binary_path)
            .arg("--input")
            .arg(&path)
            .arg("--bit-depth")
            .arg("24")
            .arg("--check-upsampling")
            .arg("--verbose")
            .output()
            .expect("Failed to execute binary");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = parse_verbose_output(&filename, &stdout);
        results.push(result);
    }
    
    // Sort by cutoff ratio (lowest first - most likely to be flagged)
    results.sort_by(|a, b| a.cutoff_ratio.partial_cmp(&b.cutoff_ratio).unwrap());
    
    // Print detailed analysis
    println!("\n{}", "-".repeat(100));
    println!("DETAILED RESULTS (sorted by cutoff ratio - lowest first)");
    println!("{}", "-".repeat(100));
    println!("{:<50} {:>8} {:>8} {:>10} {:>8} {:>10} {:>12}",
        "Filename", "SR(kHz)", "Bits", "Cutoff", "Ratio", "Steepness", "Defects");
    println!("{}", "-".repeat(100));
    
    for r in &results {
        let defects_str = if r.detected_defects.is_empty() {
            "CLEAN".to_string()
        } else {
            r.detected_defects.join(", ")
        };
        
        let status = if r.is_false_positive { "⚠️" } else { "✓" };
        
        println!("{} {:<48} {:>6.1} {:>8} {:>8.0}Hz {:>7.1}% {:>8.1}dB/oct  {}",
            status,
            truncate_filename(&r.filename, 48),
            r.sample_rate as f32 / 1000.0,
            r.bit_depth,
            r.frequency_cutoff,
            r.cutoff_ratio * 100.0,
            r.rolloff_steepness,
            defects_str
        );
    }
    
    // Statistical analysis
    println!("\n{}", "=".repeat(100));
    println!("STATISTICAL ANALYSIS");
    println!("{}", "=".repeat(100));
    
    let false_positives: Vec<_> = results.iter().filter(|r| r.is_false_positive).collect();
    let clean: Vec<_> = results.iter().filter(|r| !r.is_false_positive).collect();
    
    println!("\nTotal files analyzed: {}", results.len());
    println!("False positives: {} ({:.1}%)", false_positives.len(), 
        (false_positives.len() as f32 / results.len() as f32) * 100.0);
    println!("Clean (correct): {}", clean.len());
    
    // Cutoff ratio analysis
    println!("\n--- Cutoff Ratio Analysis ---");
    if !false_positives.is_empty() {
        let fp_ratios: Vec<f32> = false_positives.iter().map(|r| r.cutoff_ratio).collect();
        let fp_min = fp_ratios.iter().cloned().fold(f32::INFINITY, f32::min);
        let fp_max = fp_ratios.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let fp_avg = fp_ratios.iter().sum::<f32>() / fp_ratios.len() as f32;
        
        println!("False Positives - Cutoff Ratio:");
        println!("  Min: {:.1}%  Max: {:.1}%  Avg: {:.1}%", fp_min * 100.0, fp_max * 100.0, fp_avg * 100.0);
    }
    
    if !clean.is_empty() {
        let clean_ratios: Vec<f32> = clean.iter().map(|r| r.cutoff_ratio).collect();
        let clean_min = clean_ratios.iter().cloned().fold(f32::INFINITY, f32::min);
        let clean_max = clean_ratios.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let clean_avg = clean_ratios.iter().sum::<f32>() / clean_ratios.len() as f32;
        
        println!("Clean files - Cutoff Ratio:");
        println!("  Min: {:.1}%  Max: {:.1}%  Avg: {:.1}%", clean_min * 100.0, clean_max * 100.0, clean_avg * 100.0);
    }
    
    // Rolloff steepness analysis
    println!("\n--- Rolloff Steepness Analysis ---");
    if !false_positives.is_empty() {
        let fp_steep: Vec<f32> = false_positives.iter().map(|r| r.rolloff_steepness).collect();
        let fp_min = fp_steep.iter().cloned().fold(f32::INFINITY, f32::min);
        let fp_max = fp_steep.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let fp_avg = fp_steep.iter().sum::<f32>() / fp_steep.len() as f32;
        
        println!("False Positives - Rolloff Steepness:");
        println!("  Min: {:.1} dB/oct  Max: {:.1} dB/oct  Avg: {:.1} dB/oct", fp_min, fp_max, fp_avg);
    }
    
    // Defect type distribution
    println!("\n--- Defect Type Distribution ---");
    let mut defect_counts: HashMap<String, usize> = HashMap::new();
    for r in &false_positives {
        for defect in &r.detected_defects {
            *defect_counts.entry(defect.clone()).or_insert(0) += 1;
        }
    }
    for (defect, count) in &defect_counts {
        println!("  {}: {} files", defect, count);
    }
    
    // Recommendations
    println!("\n{}", "=".repeat(100));
    println!("RECOMMENDATIONS");
    println!("{}", "=".repeat(100));
    
    if !false_positives.is_empty() {
        let fp_ratios: Vec<f32> = false_positives.iter().map(|r| r.cutoff_ratio).collect();
        let fp_min = fp_ratios.iter().cloned().fold(f32::INFINITY, f32::min);
        let fp_avg = fp_ratios.iter().sum::<f32>() / fp_ratios.len() as f32;
        
        println!("\n1. CUTOFF RATIO THRESHOLD:");
        println!("   Current threshold appears to be ~85% (0.85)");
        println!("   False positives have cutoff ratios as low as {:.1}%", fp_min * 100.0);
        println!("   Average false positive cutoff ratio: {:.1}%", fp_avg * 100.0);
        
        if fp_min < 0.85 {
            let suggested = (fp_min * 0.95).max(0.60);  // 5% below minimum, but not below 60%
            println!("   SUGGESTION: Lower threshold to {:.0}% or require additional evidence", suggested * 100.0);
        }
        
        let fp_steep: Vec<f32> = false_positives.iter().map(|r| r.rolloff_steepness).collect();
        let fp_steep_avg = fp_steep.iter().sum::<f32>() / fp_steep.len() as f32;
        
        println!("\n2. ROLLOFF STEEPNESS:");
        println!("   Average steepness in false positives: {:.1} dB/oct", fp_steep_avg);
        if fp_steep_avg < 30.0 {
            println!("   SUGGESTION: Require steepness > 30 dB/oct for MP3 detection (brick-wall)");
        }
        
        println!("\n3. MULTI-SIGNAL REQUIREMENT:");
        println!("   Consider requiring BOTH cutoff ratio AND brick-wall/steepness");
        println!("   to reduce false positives on naturally band-limited content");
        
        // Check for brick wall in false positives
        let fp_brick_wall_count = false_positives.iter().filter(|r| r.has_brick_wall).count();
        println!("\n4. BRICK WALL ANALYSIS:");
        println!("   False positives with brick-wall: {} / {}", fp_brick_wall_count, false_positives.len());
        if fp_brick_wall_count < false_positives.len() / 2 {
            println!("   SUGGESTION: Require brick-wall=true for high-confidence MP3 detection");
        }
    }
    
    // Sample rate distribution
    println!("\n--- Sample Rate Distribution of False Positives ---");
    let mut sr_counts: HashMap<u32, usize> = HashMap::new();
    for r in &false_positives {
        *sr_counts.entry(r.sample_rate).or_insert(0) += 1;
    }
    for (sr, count) in &sr_counts {
        println!("  {} Hz: {} files", sr, count);
    }
    
    // Bit depth distribution
    println!("\n--- Bit Depth Distribution of False Positives ---");
    let mut bd_counts: HashMap<u32, usize> = HashMap::new();
    for r in &false_positives {
        *bd_counts.entry(r.bit_depth).or_insert(0) += 1;
    }
    for (bd, count) in &bd_counts {
        println!("  {}-bit: {} files", bd, count);
    }
    
    println!("\n{}", "=".repeat(100));
}

fn parse_verbose_output(filename: &str, output: &str) -> DiagnosticResult {
    let mut result = DiagnosticResult {
        filename: filename.to_string(),
        sample_rate: 44100,
        bit_depth: 16,
        frequency_cutoff: 22050.0,
        nyquist: 22050.0,
        cutoff_ratio: 1.0,
        rolloff_steepness: 0.0,
        has_brick_wall: false,
        spectral_flatness: 0.0,
        quality_score: 1.0,
        detected_defects: Vec::new(),
        is_false_positive: false,
    };
    
    for line in output.lines() {
        let line = line.trim();
        
        // Parse sample rate
        if line.contains("Sample Rate:") {
            if let Some(hz_str) = line.split(':').nth(1) {
                let hz_str = hz_str.trim().replace(" Hz", "").replace(",", "");
                if let Ok(sr) = hz_str.parse::<u32>() {
                    result.sample_rate = sr;
                    result.nyquist = sr as f32 / 2.0;
                }
            }
        }
        
        // Parse bit depth
        if line.contains("Bit Depth:") && !line.contains("claimed") {
            if let Some(bits_part) = line.split(':').nth(1) {
                let bits_str = bits_part.trim().split_whitespace().next().unwrap_or("16");
                if let Ok(bd) = bits_str.parse::<u32>() {
                    result.bit_depth = bd;
                }
            }
        }
        
        // Parse frequency cutoff
        if line.contains("Frequency Cutoff:") || line.contains("frequency cutoff") {
            if let Some(hz_str) = extract_number_before(line, "Hz") {
                result.frequency_cutoff = hz_str;
                if result.nyquist > 0.0 {
                    result.cutoff_ratio = result.frequency_cutoff / result.nyquist;
                }
            }
        }
        
        // Parse rolloff steepness
        if line.contains("Spectral rolloff:") || line.contains("rolloff:") {
            if let Some(hz_str) = extract_number_before(line, "Hz") {
                // This might be rolloff frequency, not steepness
            }
        }
        
        if line.contains("Rolloff steepness:") || line.contains("steepness:") {
            if let Some(db_oct) = extract_number_before(line, "dB") {
                result.rolloff_steepness = db_oct;
            }
        }
        
        // Parse brick wall
        if line.contains("Brick-wall") || line.contains("brick-wall") || line.contains("brick wall") {
            result.has_brick_wall = line.contains("Yes") || line.contains("true") || line.contains("detected");
        }
        
        // Parse spectral flatness
        if line.contains("Spectral flatness:") || line.contains("flatness:") {
            if let Some(val) = extract_number_after_colon(line) {
                result.spectral_flatness = val;
            }
        }
        
        // Parse quality score
        if line.contains("Quality Score:") {
            if let Some(pct) = extract_number_before(line, "%") {
                result.quality_score = pct / 100.0;
            }
        }
        
        // Parse defects
        if line.contains("MP3") && (line.contains("transcode") || line.contains("Transcode")) {
            if !result.detected_defects.contains(&"Mp3Transcode".to_string()) {
                result.detected_defects.push("Mp3Transcode".to_string());
            }
        }
        if line.contains("AAC") && (line.contains("transcode") || line.contains("Transcode")) {
            if !result.detected_defects.contains(&"AacTranscode".to_string()) {
                result.detected_defects.push("AacTranscode".to_string());
            }
        }
        if line.contains("Opus") && (line.contains("transcode") || line.contains("Transcode")) {
            if !result.detected_defects.contains(&"OpusTranscode".to_string()) {
                result.detected_defects.push("OpusTranscode".to_string());
            }
        }
        if (line.contains("Vorbis") || line.contains("Ogg")) && (line.contains("transcode") || line.contains("Transcode")) {
            if !result.detected_defects.contains(&"OggVorbisTranscode".to_string()) {
                result.detected_defects.push("OggVorbisTranscode".to_string());
            }
        }
        if line.contains("Bit depth mismatch") || line.contains("BitDepthMismatch") {
            if !result.detected_defects.contains(&"BitDepthMismatch".to_string()) {
                result.detected_defects.push("BitDepthMismatch".to_string());
            }
        }
        if line.contains("Upsampled") && !line.contains("not") {
            if !result.detected_defects.contains(&"Upsampled".to_string()) {
                result.detected_defects.push("Upsampled".to_string());
            }
        }
    }
    
    // These are control files, so any defect is a false positive
    result.is_false_positive = !result.detected_defects.is_empty();
    
    result
}

fn extract_number_before(line: &str, suffix: &str) -> Option<f32> {
    if let Some(pos) = line.find(suffix) {
        let before = &line[..pos];
        // Find the last number-like sequence before the suffix
        let chars: Vec<char> = before.chars().collect();
        let mut end = chars.len();
        let mut start = end;
        
        // Walk backwards to find digits
        for i in (0..chars.len()).rev() {
            if chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '-' {
                if start == end {
                    end = i + 1;
                }
                start = i;
            } else if start != end {
                break;
            }
        }
        
        if start < end {
            let num_str: String = chars[start..end].iter().collect();
            return num_str.parse::<f32>().ok();
        }
    }
    None
}

fn extract_number_after_colon(line: &str) -> Option<f32> {
    if let Some(pos) = line.rfind(':') {
        let after = line[pos + 1..].trim();
        // Get first number-like token
        let num_str: String = after.chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        return num_str.parse::<f32>().ok();
    }
    None
}

fn truncate_filename(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}...", &name[..max_len - 3])
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

/// Additional test: Compare control files with known transcoded files
#[test]
fn compare_control_vs_transcoded() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_suite = project_root.join("TestSuite");
    
    if !test_suite.exists() {
        println!("TestSuite not found. Skipping comparison.");
        return;
    }
    
    println!("\n{}", "=".repeat(100));
    println!("COMPARISON: Control vs Known Transcoded Files");
    println!("Looking for distinguishing characteristics");
    println!("{}\n", "=".repeat(100));
    
    // Analyze a few control files
    let control_dir = test_suite.join("Control_Original");
    let mp3_dir = test_suite.join("MP3_128_Boundary");
    
    if !control_dir.exists() || !mp3_dir.exists() {
        println!("Required directories not found. Skipping.");
        return;
    }
    
    println!("Analyzing Control files...");
    let control_results = analyze_directory(&binary_path, &control_dir, 5);
    
    println!("\nAnalyzing MP3_128_Boundary files...");
    let mp3_results = analyze_directory(&binary_path, &mp3_dir, 5);
    
    // Print comparison table
    println!("\n{}", "-".repeat(100));
    println!("COMPARISON TABLE");
    println!("{}", "-".repeat(100));
    println!("{:<40} {:>12} {:>12} {:>15} {:>15}", 
        "File", "Cutoff", "Ratio", "Steepness", "Type");
    println!("{}", "-".repeat(100));
    
    println!("\n--- CONTROL FILES (should be CLEAN) ---");
    for r in &control_results {
        println!("{:<40} {:>10.0}Hz {:>10.1}% {:>12.1}dB/oct {:>15}",
            truncate_filename(&r.filename, 40),
            r.frequency_cutoff,
            r.cutoff_ratio * 100.0,
            r.rolloff_steepness,
            if r.detected_defects.is_empty() { "CLEAN" } else { &r.detected_defects.join(",") }
        );
    }
    
    println!("\n--- MP3 128k FILES (should be DEFECTIVE) ---");
    for r in &mp3_results {
        println!("{:<40} {:>10.0}Hz {:>10.1}% {:>12.1}dB/oct {:>15}",
            truncate_filename(&r.filename, 40),
            r.frequency_cutoff,
            r.cutoff_ratio * 100.0,
            r.rolloff_steepness,
            if r.detected_defects.is_empty() { "CLEAN" } else { &r.detected_defects.join(",") }
        );
    }
    
    // Statistical comparison
    println!("\n{}", "=".repeat(100));
    println!("STATISTICAL COMPARISON");
    println!("{}", "=".repeat(100));
    
    if !control_results.is_empty() && !mp3_results.is_empty() {
        let ctrl_cutoff_avg = control_results.iter().map(|r| r.cutoff_ratio).sum::<f32>() / control_results.len() as f32;
        let mp3_cutoff_avg = mp3_results.iter().map(|r| r.cutoff_ratio).sum::<f32>() / mp3_results.len() as f32;
        
        let ctrl_steep_avg = control_results.iter().map(|r| r.rolloff_steepness).sum::<f32>() / control_results.len() as f32;
        let mp3_steep_avg = mp3_results.iter().map(|r| r.rolloff_steepness).sum::<f32>() / mp3_results.len() as f32;
        
        println!("\nCutoff Ratio (% of Nyquist):");
        println!("  Control average: {:.1}%", ctrl_cutoff_avg * 100.0);
        println!("  MP3 128k average: {:.1}%", mp3_cutoff_avg * 100.0);
        println!("  Difference: {:.1} percentage points", (ctrl_cutoff_avg - mp3_cutoff_avg) * 100.0);
        
        println!("\nRolloff Steepness (dB/octave):");
        println!("  Control average: {:.1} dB/oct", ctrl_steep_avg);
        println!("  MP3 128k average: {:.1} dB/oct", mp3_steep_avg);
        println!("  Difference: {:.1} dB/oct", ctrl_steep_avg - mp3_steep_avg);
        
        // Suggest threshold
        let suggested_cutoff = (ctrl_cutoff_avg + mp3_cutoff_avg) / 2.0;
        let suggested_steep = (ctrl_steep_avg + mp3_steep_avg) / 2.0;
        
        println!("\n--- SUGGESTED THRESHOLDS ---");
        println!("  Cutoff ratio threshold: {:.1}% (midpoint)", suggested_cutoff * 100.0);
        println!("  Steepness threshold: {:.1} dB/oct (midpoint)", suggested_steep);
        println!("\n  For safety margin, consider:");
        println!("  - Cutoff ratio: < {:.1}% to flag as transcode", (suggested_cutoff - 0.05) * 100.0);
        println!("  - Steepness: > {:.1} dB/oct required for MP3", suggested_steep + 5.0);
    }
}

fn analyze_directory(binary: &Path, dir: &Path, limit: usize) -> Vec<DiagnosticResult> {
    let mut results = Vec::new();
    
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    
    for (count, entry) in entries.enumerate() {
        if count >= limit {
            break;
        }
        
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("flac") {
            continue;
        }
        
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        
        let output = Command::new(binary)
            .arg("--input")
            .arg(&path)
            .arg("--bit-depth")
            .arg("24")
            .arg("--check-upsampling")
            .arg("--verbose")
            .output()
            .expect("Failed to execute binary");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        results.push(parse_verbose_output(&filename, &stdout));
    }
    
    results
}
