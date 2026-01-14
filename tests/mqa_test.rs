use std::path::{Path, PathBuf};
use colorful::Colorful;
use walkdir::WalkDir;
use std::process::Command;

#[test]
fn test_mqa_detection() {
    let mqa_dir = Path::new("MQA");
    if !mqa_dir.exists() {
        println!("MQA directory not found, skipping tests");
        return;
    }

    let binary_path = Path::new("target/release/audiocheckr");
    if !binary_path.exists() {
        panic!("Binary not found at {:?}. Run 'cargo build --release' first.", binary_path);
    }

    let mut passed = 0;
    let mut failed = 0;

    for entry in WalkDir::new(mqa_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "flac") {
            let output = Command::new(binary_path)
                .arg(path)
                .arg("--json")
                .output()
                .expect("Failed to execute command");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let file_name = path.file_name().unwrap().to_string_lossy();

            if stdout.contains("MqaEncoded") {
                println!("{} {} - PASSED", "[OK]".bg_green(), file_name);
                passed += 1;
            } else {
                println!("{} {} - FAILED", "[FAIL]".bg_red(), file_name);
                failed += 1;
            }
        }
    }

    println!("\nResults: {} passed, {} failed", passed, failed);
    assert_eq!(failed, 0, "Some MQA files were not detected");
}
