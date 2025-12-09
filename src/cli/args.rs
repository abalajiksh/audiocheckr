//! CLI argument parsing with profile support

use crate::config::{DetectorType, ProfileBuilder, ProfileConfig, ProfilePreset};

/// Parsed CLI arguments
#[derive(Debug)]
pub struct CliArgs {
    pub files: Vec<String>,
    pub profile: ProfileConfig,
    pub verbose: bool,
    pub show_suppressed: bool,
    pub json_output: bool,
    pub list_profiles: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            profile: ProfileConfig::default(),
            verbose: false,
            show_suppressed: false,
            json_output: false,
            list_profiles: false,
        }
    }
}

/// Parse CLI arguments into CliArgs
/// 
/// Supported flags:
///   --profile=NAME       Use named profile (standard, highres, electronic, noise, classical, podcast)
///   --sensitivity=N      Global sensitivity multiplier (0.1 - 2.0)
///   --disable=DETECTOR   Disable specific detector (can repeat)
///   --enable=DETECTOR    Enable specific detector (can repeat)
///   --threshold=DET:VAL  Set minimum threshold for detector
///   --verbose, -v        Verbose output
///   --show-suppressed    Show findings suppressed by profile
///   --json               JSON output format
///   --list-profiles      List available profiles and exit
///   --help, -h           Show help
pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut result = CliArgs::default();
    let mut builder = ProfileBuilder::new();
    let mut profile_set = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "--help" || arg == "-h" {
            return Err(get_help_text());
        }

        if arg == "--list-profiles" {
            result.list_profiles = true;
            return Ok(result);
        }

        if arg == "--verbose" || arg == "-v" {
            result.verbose = true;
            i += 1;
            continue;
        }

        if arg == "--show-suppressed" {
            result.show_suppressed = true;
            i += 1;
            continue;
        }

        if arg == "--json" {
            result.json_output = true;
            i += 1;
            continue;
        }

        if let Some(profile_name) = arg.strip_prefix("--profile=") {
            let preset = ProfilePreset::from_name(profile_name)
                .ok_or_else(|| format!("Unknown profile: {}", profile_name))?;
            builder = ProfileBuilder::from_preset(preset);
            profile_set = true;
            i += 1;
            continue;
        }

        if let Some(sens) = arg.strip_prefix("--sensitivity=") {
            let sensitivity: f32 = sens
                .parse()
                .map_err(|_| format!("Invalid sensitivity value: {}", sens))?;
            builder = builder.sensitivity(sensitivity);
            i += 1;
            continue;
        }

        if let Some(detector_name) = arg.strip_prefix("--disable=") {
            let detector = DetectorType::from_name(detector_name)
                .ok_or_else(|| format!("Unknown detector: {}", detector_name))?;
            builder = builder.disable(detector);
            i += 1;
            continue;
        }

        if let Some(detector_name) = arg.strip_prefix("--enable=") {
            let detector = DetectorType::from_name(detector_name)
                .ok_or_else(|| format!("Unknown detector: {}", detector_name))?;
            builder = builder.enable(detector);
            i += 1;
            continue;
        }

        if let Some(threshold_spec) = arg.strip_prefix("--threshold=") {
            let parts: Vec<&str> = threshold_spec.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(format!(
                    "Invalid threshold format: {}. Use --threshold=detector:value",
                    threshold_spec
                ));
            }
            let detector = DetectorType::from_name(parts[0])
                .ok_or_else(|| format!("Unknown detector: {}", parts[0]))?;
            let threshold: f32 = parts[1]
                .parse()
                .map_err(|_| format!("Invalid threshold value: {}", parts[1]))?;
            builder = builder.modifier(detector, 1.0, threshold);
            i += 1;
            continue;
        }

        // Not a flag - treat as file path
        if !arg.starts_with('-') {
            result.files.push(arg.clone());
        } else {
            return Err(format!("Unknown option: {}", arg));
        }

        i += 1;
    }

    // Build the profile
    result.profile = if profile_set {
        builder.build()
    } else {
        // Keep standard preset marker if no customizations
        builder.build()
    };

    Ok(result)
}

fn get_help_text() -> String {
    r#"AudioCheckr - Detect fake lossless audio files

USAGE:
    audiocheckr [OPTIONS] <FILES>...

OPTIONS:
    --profile=NAME       Use detection profile
                         (standard, highres, electronic, noise, classical, podcast)
    --sensitivity=N      Global sensitivity multiplier (0.1 - 2.0)
    --disable=DETECTOR   Disable specific detector (can repeat)
    --enable=DETECTOR    Enable specific detector (can repeat)
    --threshold=DET:VAL  Set minimum threshold for detector
    
    -v, --verbose        Verbose output with detector details
    --show-suppressed    Show findings suppressed by profile
    --json               Output results as JSON
    --list-profiles      List available profiles and exit
    -h, --help           Show this help

DETECTORS:
    spectral_cutoff      Detect sharp frequency cutoffs (lossy transcodes)
    pre_echo             Detect MP3/AAC pre-echo artifacts
    bit_depth            Verify actual vs claimed bit depth
    upsampling           Detect upsampled audio
    codec_signature      Identify lossy codec fingerprints
    phase_analysis       Analyze phase coherence
    dynamic_range        Measure dynamic range characteristics

EXAMPLES:
    audiocheckr album/*.flac
    audiocheckr --profile=noise ambient_track.flac
    audiocheckr --profile=electronic --disable=spectral_cutoff track.flac
    audiocheckr --sensitivity=0.5 --threshold=pre_echo:0.7 track.flac
"#.to_string()
}

/// Print available profiles
pub fn print_profiles() {
    println!("Available detection profiles:\n");

    let presets = [
        ProfilePreset::Standard,
        ProfilePreset::HighResAudio,
        ProfilePreset::Electronic,
        ProfilePreset::Noise,
        ProfilePreset::Classical,
        ProfilePreset::Podcast,
    ];

    for preset in presets {
        let config = ProfileConfig::from_preset(preset);
        println!("  {} - {}", preset.name(), preset.description());
        println!("    Sensitivity: {:.1}x", config.global_sensitivity);
        println!(
            "    Spectral tolerance: {} Hz",
            config.spectral_cutoff_tolerance_hz
        );
        println!(
            "    Pre-echo sensitivity: {:.1}",
            config.pre_echo_sensitivity
        );

        // Show modified detectors
        let modified: Vec<_> = config
            .confidence_modifiers
            .iter()
            .map(|(d, m)| format!("{}:{:.1}x", d.name(), m.multiplier))
            .collect();
        if !modified.is_empty() {
            println!("    Modifiers: {}", modified.join(", "));
        }

        // Show disabled detectors
        let all_detectors: std::collections::HashSet<_> = DetectorType::all().iter().collect();
        let enabled: std::collections::HashSet<_> = config.enabled_detectors.iter().collect();
        let disabled: Vec<_> = all_detectors
            .difference(&enabled)
            .map(|d| d.name())
            .collect();
        if !disabled.is_empty() {
            println!("    Disabled: {}", disabled.join(", "));
        }

        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_profile() {
        let args = vec![
            "--profile=noise".to_string(),
            "test.flac".to_string(),
        ];
        let result = parse_args(&args).unwrap();
        assert_eq!(result.profile.preset, ProfilePreset::Custom); // Built profile becomes Custom
        assert_eq!(result.files, vec!["test.flac"]);
    }

    #[test]
    fn test_parse_disable_detector() {
        let args = vec![
            "--disable=pre_echo".to_string(),
            "--disable=spectral_cutoff".to_string(),
            "test.flac".to_string(),
        ];
        let result = parse_args(&args).unwrap();
        assert!(!result.profile.is_detector_enabled(DetectorType::PreEcho));
        assert!(!result.profile.is_detector_enabled(DetectorType::SpectralCutoff));
        assert!(result.profile.is_detector_enabled(DetectorType::BitDepth));
    }

    #[test]
    fn test_parse_sensitivity() {
        let args = vec![
            "--sensitivity=0.5".to_string(),
            "test.flac".to_string(),
        ];
        let result = parse_args(&args).unwrap();
        assert!((result.profile.global_sensitivity - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_threshold() {
        let args = vec![
            "--threshold=pre_echo:0.7".to_string(),
            "test.flac".to_string(),
        ];
        let result = parse_args(&args).unwrap();
        let modifier = result.profile.get_modifier(DetectorType::PreEcho);
        assert!((modifier.min_threshold - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_unknown_profile() {
        let args = vec!["--profile=invalid".to_string()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_unknown_detector() {
        let args = vec!["--disable=invalid".to_string()];
        assert!(parse_args(&args).is_err());
    }
}
