# AudioCheckr - Restructured Project

This directory contains the restructured AudioCheckr project with modular organization to support future features.

## New Structure

```
src/
├── lib.rs                    # Library entry point
├── main.rs                   # CLI entry point
├── core/                     # Audio analysis (current functionality)
│   ├── mod.rs
│   ├── analyzer.rs          # High-level API
│   ├── decoder.rs           # Audio decoding (Symphonia)
│   ├── detector.rs          # Quality detection
│   ├── analysis/            # Detection algorithms
│   │   ├── bit_depth.rs     # Fake 24-bit detection
│   │   ├── spectral.rs      # Frequency cutoff analysis
│   │   ├── upsampling.rs    # Upsampling detection
│   │   ├── stereo.rs        # Stereo field analysis
│   │   ├── transients.rs    # Pre-echo detection
│   │   ├── phase.rs         # Phase analysis
│   │   ├── true_peak.rs     # True peak measurement
│   │   └── mfcc.rs          # MFCC fingerprinting
│   ├── dsp/                 # DSP utilities
│   │   ├── fft.rs           # FFT processing
│   │   ├── windows.rs       # Window functions
│   │   ├── filters.rs       # Resampling/filtering
│   │   └── stats.rs         # Statistical functions
│   └── visualization/       # Visual output
│       └── spectrogram.rs   # Spectrogram generation
├── cli/                     # CLI interface
│   ├── args.rs              # Argument parsing
│   └── output.rs            # Output formatting
├── config/                  # Configuration
│   └── profiles.rs          # Genre-aware detection profiles
└── detection/               # Detection results
    └── result.rs            # Profile-aware result types
```

## Migration Instructions

### Option 1: Fresh Start (Recommended)

1. Copy this entire directory to replace your existing `src/` folder
2. Update your `Cargo.toml` with the provided version
3. Run `cargo check` to verify compilation
4. Run `cargo test` to verify functionality

### Option 2: Incremental Migration

Use the provided scripts:

```bash
# 1. Run the restructure script (creates directories, moves files)
chmod +x restructure.sh
./restructure.sh /path/to/your/audiocheckr

# 2. Update imports
python3 update_imports.py /path/to/your/audiocheckr

# 3. Verify
cargo check
cargo test
```

## New Features

### Genre-Aware Detection Profiles

The new profile system reduces false positives for different audio types:

```rust
use audiocheckr::config::{ProfileConfig, ProfilePreset};

// Use a preset
let profile = ProfileConfig::from_preset(ProfilePreset::Electronic);

// Or build a custom profile
let profile = ProfileBuilder::new()
    .name("My Custom Profile")
    .disable_detector(DetectorType::PreEcho)
    .detector_multiplier(DetectorType::SpectralCutoff, 0.7)
    .build();
```

### Available Presets

| Profile | Use Case | Key Adjustments |
|---------|----------|-----------------|
| Standard | General music | Balanced defaults |
| HighRes | Verified hi-res | Reduced cutoff sensitivity |
| Electronic | EDM, synthwave | Tolerates sharp cutoffs |
| Noise | Ambient, drone | Full-spectrum tolerance |
| Classical | Orchestral | Strict dynamic range |
| Podcast | Speech content | Limited detectors |

### CLI Usage

```bash
# Use default profile
audiocheckr -i music.flac

# Use electronic profile
audiocheckr -i edm_track.flac --profile electronic

# Show suppressed findings
audiocheckr -i ambient.flac --profile noise --show-suppressed

# Disable specific detectors
audiocheckr -i file.flac --disable pre_echo,upsampling

# JSON output
audiocheckr -i file.flac --json
```

## Breaking Changes

- Import paths have changed (e.g., `crate::bit_depth` → `crate::core::analysis::bit_depth`)
- The library now exports types from `audiocheckr::*` root for convenience
- `QualityReport` struct has additional fields for profile-aware analysis

## Files Included

- `Cargo.toml` - Updated package configuration
- `src/` - Complete restructured source code
- `restructure.sh` - Shell script to migrate existing project
- `update_imports.py` - Python script to update import paths
- `README.md` - This file

## Next Steps After Migration

1. **Verify all tests pass**: `cargo test`
2. **Update your Jenkinsfile** if using CI/CD
3. **Integrate profiles into your workflow** for genre-specific analysis
4. **Review and customize profiles** for your specific needs

## Support

If you encounter issues during migration, check:

1. All import paths are updated
2. `Cargo.toml` edition is `2021` (not `2024`)
3. No duplicate module declarations in `lib.rs` or `main.rs`
