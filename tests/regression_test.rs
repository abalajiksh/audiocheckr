// tests/regression_test.rs
// GROUND TRUTH Regression Test Suite

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
fn test_all_audio_files_comprehensive() {
    let binary_path = get_binary_path();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_base = project_root.join("TestFiles");

    assert!(test_base.exists());

    println!("\n=== GROUND TRUTH Regression Test Suite ===");
    println!("Using TestFiles from: {}\n", test_base.display());

    let test_cases = define_test_cases(&test_base);
    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;

    println!("Running {} comprehensive tests...\n", test_cases.len());

    for (idx, test_case) in test_cases.iter().enumerate() {
        let result = run_test(&binary_path, test_case);

        if result.passed == result.expected {
            passed += 1;
            println!("[{}/{}] ✓ PASS: {}", idx + 1, test_cases.len(), test_case.description);
        } else {
            failed += 1;

            if result.passed && !result.expected {
                false_negatives += 1;
                println!("[{}/{}] ✗ FALSE NEGATIVE: {}", idx + 1, test_cases.len(), test_case.description);
            } else {
                false_positives += 1;
                println!("[{}/{}] ✗ FALSE POSITIVE: {}", idx + 1, test_cases.len(), test_case.description);
            }

            println!("  Expected: {}, Got: {}",
                if test_case.should_pass { "CLEAN" } else { "DEFECTS" },
                if result.passed { "CLEAN" } else { "DEFECTS" });
        }

        results.push(result);
    }

    println!("\n{}", "=".repeat(70));
    println!("COMPREHENSIVE TEST RESULTS");
    println!("{}", "=".repeat(70));
    println!("Total: {}", test_cases.len());
    println!("Correct: {} ({:.1}%)", passed, (passed as f32 / test_cases.len() as f32) * 100.0);
    println!("Incorrect: {} ({:.1}%)", failed, (failed as f32 / test_cases.len() as f32) * 100.0);
    println!("  False Negatives: {}", false_negatives);
    println!("  False Positives: {}", false_positives);
    println!("{}", "=".repeat(70));

    if failed > 0 {
        println!("\n⚠️  Detector needs improvement in {} areas", failed);
    } else {
        println!("\n✅ Perfect detection!");
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
            println!("Using Windows release binary");
            return release_path_exe;
        } else if debug_path_exe.exists() {
            println!("Using Windows debug binary");
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

fn define_test_cases(base: &Path) -> Vec<TestCase> {
    let mut cases = Vec::new();

cases.push(TestCase {
    file_path: base.join("CleanOrigin/input192.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("CleanOrigin/input96.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("CleanTranscoded/input192_16bit.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("CleanTranscoded/input96_16bit.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_16bit_44khz_mp3_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 128 upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_16bit_44khz_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_16bit_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_aac_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "AacTranscode".to_string()],
    description: "192kHz AAC 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_aac_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "AacTranscode".to_string()],
    description: "192kHz AAC 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_aac_256_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "AacTranscode".to_string()],
    description: "192kHz AAC 256 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_aac_320_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "AacTranscode".to_string()],
    description: "192kHz AAC 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_256_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 256 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_320_reencoded_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_320_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_to_aac_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string(), "AacTranscode".to_string()],
    description: "192kHz MP3 AAC upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_v0_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 v0 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_v2_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 v2 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_mp3_v4_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3 v4 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_160_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus 160 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_64_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus 64 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_96_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus 96 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_opus_to_mp3_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string(), "OpusTranscode".to_string()],
    description: "192kHz MP3 Opus upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_original.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz original".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_resampled_44.1_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_resampled_48_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_vorbis_q3_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OggVorbisTranscode".to_string()],
    description: "192kHz Vorbis q3 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_vorbis_q5_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OggVorbisTranscode".to_string()],
    description: "192kHz Vorbis q5 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_vorbis_q7_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OggVorbisTranscode".to_string()],
    description: "192kHz Vorbis q7 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test192_vorbis_q9_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OggVorbisTranscode".to_string()],
    description: "192kHz Vorbis q9 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_16bit_44khz_mp3_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "96kHz MP3 128 upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_16bit_44khz_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "96kHz upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_16bit_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "96kHz upscaled 16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_aac_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["AacTranscode".to_string()],
    description: "96kHz AAC 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_aac_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["AacTranscode".to_string()],
    description: "96kHz AAC 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_aac_256_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["AacTranscode".to_string()],
    description: "96kHz AAC 256 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_aac_320_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["AacTranscode".to_string()],
    description: "96kHz AAC 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_256_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 256 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_320_reencoded_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_320_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 320 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_to_aac_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string(), "AacTranscode".to_string()],
    description: "96kHz MP3 AAC upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_v0_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 v0 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_v2_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 v2 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_mp3_v4_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3 v4 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_128_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus 128 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_160_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus 160 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_192_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus 192 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_64_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus 64 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_96_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus 96 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_opus_to_mp3_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string(), "OpusTranscode".to_string()],
    description: "96kHz MP3 Opus upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_original.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz original".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_resampled_44.1_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec![],
    description: "96kHz upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_resampled_48_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec![],
    description: "96kHz upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_vorbis_q3_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OggVorbisTranscode".to_string()],
    description: "96kHz Vorbis q3 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_vorbis_q5_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OggVorbisTranscode".to_string()],
    description: "96kHz Vorbis q5 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_vorbis_q7_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OggVorbisTranscode".to_string()],
    description: "96kHz Vorbis q7 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("MasterScript/test96_vorbis_q9_upscaled.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OggVorbisTranscode".to_string()],
    description: "96kHz Vorbis q9 upscaled".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample192/input192_176.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample192/input192_44.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample192/input192_48.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample192/input192_88.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample192/input192_96.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "192kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample96/input96_176.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample96/input96_192.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample96/input96_44.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample96/input96_48.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Resample96/input96_88.flac").to_string_lossy().to_string(),
    should_pass: true,
    expected_defects: vec![],
    description: "96kHz".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscale16/output192_16bit.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscale16/output96_16bit.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string()],
    description: "16-bit".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input192_m4a.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "AacTranscode".to_string()],
    description: "192kHz AAC".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input192_mp3.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "Mp3Transcode".to_string()],
    description: "192kHz MP3".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input192_ogg.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OggVorbisTranscode".to_string()],
    description: "192kHz Vorbis".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input192_opus.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["BitDepthMismatch".to_string(), "OpusTranscode".to_string()],
    description: "192kHz Opus".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input96_m4a.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["AacTranscode".to_string()],
    description: "96kHz AAC".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input96_mp3.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["Mp3Transcode".to_string()],
    description: "96kHz MP3".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input96_ogg.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OggVorbisTranscode".to_string()],
    description: "96kHz Vorbis".to_string(),
});
cases.push(TestCase {
    file_path: base.join("Upscaled/input96_opus.flac").to_string_lossy().to_string(),
    should_pass: false,
    expected_defects: vec!["OpusTranscode".to_string()],
    description: "96kHz Opus".to_string(),
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

    let has_issues = stdout.contains("✗ ISSUES DETECTED") || stdout.contains("ISSUES DETECTED");
    let is_clean = stdout.contains("✓ CLEAN") || (stdout.contains("CLEAN") && !has_issues);

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
    assert!(binary_path.exists());
}

#[test]
fn test_help_output() {
    let binary_path = get_binary_path();
    let output = Command::new(&binary_path).arg("--help").output().expect("Failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("audiocheckr"));
}
