# AudioCheckr v0.3.0

Advanced audio analysis tool for detecting fake lossless files, transcodes, upsampled audio, and various audio quality issues. Uses pure DSP algorithms - no machine learning.

## Features

### Core Detection Capabilities
- **Lossy Transcode Detection**: Identifies MP3, AAC, Vorbis, and Opus transcodes via spectral analysis
- **Bit Depth Analysis**: Detects 16-bit audio masquerading as 24-bit using 4 independent methods
- **Upsampling Detection**: Identifies audio upsampled from lower sample rates
- **Stereo Analysis**: Detects joint stereo encoding artifacts
- **Pre-Echo Detection**: Identifies transform codec artifacts before transients
- **Phase Analysis**: Detects phase discontinuities at codec frame boundaries
- **True Peak Analysis**: ITU-R BS.1770 compliant true peak measurement
- **Spectral Artifacts**: Detects unusual spectral patterns and notches

### v0.3.0 - Major Restructure

This release introduces a **modular architecture** to support future features like web UI, continuous monitoring, and API integrations.

**New Features:**
- **Genre-Aware Detection Profiles**: Reduce false positives with presets for Electronic, Classical, Ambient, Podcast, etc.
- **Modular Codebase**: Clean separation of core analysis, CLI, configuration, and detection result handling
- **Profile Builder API**: Create custom detection profiles programmatically
- **Confidence Modifiers**: Per-detector sensitivity adjustment with suppression options

**Architecture Changes:**
- Reorganized into `core/`, `cli/`, `config/`, and `detection/` modules
- Analysis algorithms split into dedicated files (spectral, bit_depth, upsampling, etc.)
- DSP utilities consolidated in `core/dsp/`
- Visualization tools in `core/visualization/`

### Previous Versions

<details>
<summary>v0.2.x Changelog</summary>

#### v0.2.4
Better detector, results on the way.

#### v0.2.3
Minor parameter adjustments to `detector.rs` to not flag clean source files. Jenkins fixes.

#### v0.2.2
Fixed test case logic with `is_clean || is_lossless` to just `is_clean` for better reporting.

#### v0.2.1
**Fixed false positives on high sample rate files (88.2kHz+)**

The previous version used ratio-based detection which caused false positives on high-res files. The new algorithm uses **absolute frequency thresholds** for high sample rate files and requires **positive codec identification** before flagging.
</details>

---

## Project Structure

```
src/
├── lib.rs                    # Library entry point
├── main.rs                   # CLI entry point
├── core/                     # Audio analysis engine
│   ├── mod.rs
│   ├── analyzer.rs           # High-level API (AudioAnalyzer, AnalyzerBuilder)
│   ├── decoder.rs            # Audio decoding (Symphonia)
│   ├── detector.rs           # Quality detection orchestration
│   ├── analysis/             # Detection algorithms
│   │   ├── bit_depth.rs      # Fake 24-bit detection (4 methods)
│   │   ├── spectral.rs       # Frequency cutoff & codec signatures
│   │   ├── upsampling.rs     # Upsampling detection
│   │   ├── stereo.rs         # Stereo field analysis
│   │   ├── transients.rs     # Pre-echo detection
│   │   ├── phase.rs          # Phase discontinuity analysis
│   │   ├── true_peak.rs      # ITU-R BS.1770 true peak
│   │   └── mfcc.rs           # MFCC codec fingerprinting
│   ├── dsp/                  # DSP utilities
│   │   ├── fft.rs            # FFT processing with windowing
│   │   ├── windows.rs        # Hann, Hamming, Blackman, Kaiser
│   │   ├── filters.rs        # Pre-emphasis, sinc interpolation
│   │   └── stats.rs          # RMS, spectral features, etc.
│   └── visualization/        # Visual output
│       └── spectrogram.rs    # Mel/linear spectrogram generation
├── cli/                      # Command-line interface
│   ├── args.rs               # Argument parsing (clap)
│   └── output.rs             # Report formatting
├── config/                   # Configuration
│   └── profiles.rs           # Genre-aware detection profiles
└── detection/                # Detection results
    └── result.rs             # Profile-aware result types
```

---

## Installation

```bash
cargo install --path .
```

Or build manually:
```bash
cargo build --release
```

---

## Usage

### Basic Usage
```bash
# Analyze a single file
audiocheckr -i audio.flac

# Analyze directory recursively  
audiocheckr -i /path/to/music/

# Generate spectrogram
audiocheckr -i audio.flac -s

# Full analysis with all checks
audiocheckr -i audio.flac -u --stereo --transients --phase -v
```

### Genre-Aware Profiles (New in v0.3.0)

```bash
# Use electronic profile (tolerates sharp cutoffs, disables pre-echo)
audiocheckr -i edm_track.flac --profile electronic

# Use noise/ambient profile (full-spectrum tolerance)
audiocheckr -i drone_album/ --profile noise

# Use classical profile (strict dynamic range checking)
audiocheckr -i symphony.flac --profile classical

# Show findings that were suppressed by the profile
audiocheckr -i file.flac --profile electronic --show-suppressed

# Disable specific detectors manually
audiocheckr -i file.flac --disable pre_echo,upsampling
```

### Available Profiles

| Profile | Use Case | Key Adjustments |
|---------|----------|-----------------|
| `standard` | General music | Balanced defaults |
| `highres` | Verified hi-res sources | Reduced cutoff sensitivity |
| `electronic` | EDM, synthwave, electronic | Tolerates sharp cutoffs, disables pre-echo |
| `noise` | Ambient, drone, noise | Full-spectrum tolerance, disabled upsampling detection |
| `classical` | Orchestral, acoustic | Strict bit depth and dynamic range |
| `podcast` | Speech, voice content | Only bit depth and codec signature active |

### Command Line Options

```
OPTIONS:
    -i, --input <PATH>        Input file or directory [default: .]
    -b, --bit-depth <N>       Expected bit depth (16 or 24) [default: 24]
    -s, --spectrogram         Generate spectrogram images
        --linear-scale        Use linear frequency scale (default: mel)
        --full-spectrogram    Full length instead of first 15 seconds
    -o, --output <MODE>       Output: "source", "current", or path [default: source]
    -u, --check-upsampling    Enable upsampling detection
        --stereo              Enable stereo analysis
        --transients          Enable transient/pre-echo analysis
        --phase               Enable phase analysis (slower)
    -v, --verbose             Detailed output
        --json                Output as JSON
    -q, --quick               Skip slower analyses
        --min-confidence <N>  Minimum confidence threshold [default: 0.5]
        --profile <NAME>      Detection profile (standard, highres, electronic, noise, classical, podcast)
        --disable <LIST>      Disable specific detectors (comma-separated)
        --show-suppressed     Show findings suppressed by profile
```

---

## Library Usage

### Quick Check

```rust
use audiocheckr::core::{AudioAnalyzer, DetectionConfig};
use std::path::Path;

// Simple analysis with defaults
let analyzer = AudioAnalyzer::new(Path::new("audio.flac"))?;
let report = analyzer.analyze()?;

println!("Quality score: {:.0}%", report.quality_score * 100.0);
println!("Likely lossless: {}", report.is_likely_lossless);

for defect in &report.defects {
    println!("Defect: {:?} (confidence: {:.0}%)", 
        defect.defect_type, defect.confidence * 100.0);
}
```

### Builder Pattern

```rust
use audiocheckr::core::{AudioAnalyzer, AnalyzerBuilder};

let analyzer = AnalyzerBuilder::new()
    .expected_bit_depth(24)
    .check_upsampling(true)
    .check_stereo(true)
    .check_transients(true)
    .check_phase(false)  // Skip slower analysis
    .min_confidence(0.6)
    .build(Path::new("audio.flac"))?;

let report = analyzer.analyze()?;
```

### Genre-Aware Profiles

```rust
use audiocheckr::config::{ProfileConfig, ProfilePreset, ProfileBuilder, DetectorType};

// Use a preset
let electronic_profile = ProfileConfig::from_preset(ProfilePreset::Electronic);

// Or build a custom profile
let custom_profile = ProfileBuilder::new()
    .name("My Studio Profile")
    .description("Custom profile for my workflow")
    .global_sensitivity(0.9)
    .min_confidence(0.6)
    .disable_detector(DetectorType::PreEcho)
    .detector_multiplier(DetectorType::SpectralCutoff, 0.7)
    .suppress_detector(DetectorType::StereoField)  // Show but don't affect verdict
    .build();

// Check adjusted confidence
let raw_confidence = 0.8;
let adjusted = custom_profile.adjust_confidence(DetectorType::SpectralCutoff, raw_confidence);
```

### Profile-Aware Results

```rust
use audiocheckr::detection::{AnalysisResult, RawDetection, Severity};
use audiocheckr::config::{ProfileConfig, ProfilePreset, DetectorType};

// Raw detections from analysis
let raw_detections = vec![
    RawDetection {
        detector: DetectorType::SpectralCutoff,
        raw_confidence: 0.75,
        severity: Severity::Warning,
        summary: "Frequency cutoff at 16kHz".to_string(),
        evidence: vec!["Sharp rolloff detected".to_string()],
        data: serde_json::Value::Null,
    },
];

// Apply profile to get final result
let profile = ProfileConfig::from_preset(ProfilePreset::Electronic);
let result = AnalysisResult::from_detections(raw_detections, &profile);

println!("Verdict: {:?}", result.verdict);
println!("Active findings: {}", result.active_findings.len());
println!("Suppressed findings: {}", result.suppressed_findings.len());
```

---

## Output Interpretation

### Quality Score
- **90-100%**: Likely true lossless
- **70-90%**: Possibly lossless but has some issues
- **50-70%**: Suspicious - likely transcoded
- **0-50%**: Almost certainly transcoded

### Verdict (New in v0.3.0)

| Verdict | Meaning |
|---------|---------|
| `Lossless` | High confidence genuine lossless |
| `ProbablyLossless` | Likely lossless, minor uncertainty |
| `Uncertain` | Manual review recommended |
| `ProbablyLossy` | Likely transcoded or has issues |
| `Lossy` | High confidence transcoded or fake |

### Defect Types

| Defect | Meaning |
|--------|---------|
| MP3 Transcode | File was encoded from MP3 source |
| AAC Transcode | File was encoded from AAC source |
| Vorbis Transcode | File was encoded from Ogg Vorbis source |
| Opus Transcode | File was encoded from Opus source |
| Bit Depth Mismatch | 16-bit audio padded to appear as 24-bit |
| Upsampled | Audio upsampled from lower sample rate |
| Joint Stereo | Lossy joint stereo encoding detected |
| Pre-Echo | Transform codec artifacts before transients |
| Phase Discontinuities | Codec frame boundary artifacts |
| Spectral Artifacts | Unusual spectral patterns |
| Clipping | Samples at/above full scale |
| Inter-Sample Overs | True peak exceeds 0 dBFS |

---

## Algorithm Details

### Analysis Methods

Each detection uses multiple algorithms with confidence scoring:

#### Spectral Analysis (`core/analysis/spectral.rs`)
- **Multi-frame FFT**: Analyzes 30 frames spread across the track
- **Derivative-based cutoff detection**: Finds where spectrum "falls off a cliff"
- **Rolloff steepness measurement**: dB/octave calculation
- **Brick-wall detection**: Sharp cutoffs characteristic of MP3
- **Shelf pattern detection**: Characteristic of AAC encoding
- **Encoder signature matching**: Compares against known codec signatures

#### Bit Depth Analysis (`core/analysis/bit_depth.rs`)
Four independent detection methods with weighted voting:
1. **LSB Precision Analysis**: Examines trailing zeros in 24-bit scaled samples
2. **Histogram Analysis**: Counts unique values at 16-bit vs 24-bit quantization
3. **Quantization Noise Analysis**: Measures noise floor in quiet sections
4. **Value Clustering Analysis**: Checks if values cluster on 256-multiples

#### Upsampling Detection (`core/analysis/upsampling.rs`)
- **Spectral Method**: Compares frequency cutoff to original Nyquist frequencies
- **Null Test**: Downsample → upsample and compare spectra
- **Inter-sample Peak Analysis**: True high-res has inter-sample peaks

#### Stereo Analysis (`core/analysis/stereo.rs`)
- Stereo width measurement (M/S energy ratio)
- Channel correlation calculation
- Joint stereo detection via HF stereo reduction

#### Pre-Echo Analysis (`core/analysis/transients.rs`)
- Transient detection using envelope following
- Pre-transient energy measurement
- MDCT window size pattern detection

#### Phase Analysis (`core/analysis/phase.rs`)
- Phase coherence between consecutive frames
- Phase discontinuity scoring

#### True Peak Analysis (`core/analysis/true_peak.rs`)
- 4x oversampling via sinc interpolation
- Inter-sample over detection
- ITU-R BS.1770 compliant measurement

### Transcode Detection Logic

**For high sample rate files (88.2kHz+):**

| Cutoff Frequency | Evidence Required | Rationale |
|------------------|-------------------|-----------|
| > 22 kHz | None - pass | Normal high-res content |
| 20-22 kHz | Brick-wall AND >80 dB/oct | Could be MP3 320k or natural |
| 18-20 kHz | Brick-wall OR >60 dB/oct | Suspicious but needs evidence |
| 15-18 kHz | 2+ signals | More suspicious |
| < 15 kHz | Codec-specific signature | Check for codec patterns |

**For standard sample rates (44.1/48kHz):**

| Cutoff Ratio | Evidence Required |
|--------------|-------------------|
| ≥ 80% | None - pass |
| 70-80% | Brick-wall OR >40 dB/oct |
| < 70% | Flag with ratio-based confidence |

---

## DSP Utilities (`core/dsp/`)

The DSP module provides reusable signal processing functions:

### FFT Processing (`fft.rs`)
```rust
use audiocheckr::core::dsp::{FftProcessor, WindowType};

let mut fft = FftProcessor::new(4096, WindowType::Hann);
let magnitudes = fft.magnitude_spectrum(&samples);
let power_db = fft.power_spectrum_db(&samples);
```

### Window Functions (`windows.rs`)
- Hann, Hamming, Blackman, Blackman-Harris, FlatTop
- Kaiser with configurable beta parameter

### Filters (`filters.rs`)
- Pre-emphasis / de-emphasis
- Sinc interpolation upsampling
- Simple downsampling with anti-aliasing

### Statistical Functions (`stats.rs`)
- RMS, peak amplitude, dB conversion
- Envelope computation, transient detection
- Spectral centroid, spread, flatness, rolloff, flux, contrast
- Zero-crossing rate, autocorrelation

---

## CI/CD Pipeline

AudioCheckr uses Jenkins for continuous integration with multiple test types:

| Test Type | Trigger | Files | Purpose |
|-----------|---------|-------|---------|
| **Qualification** | Every push | ~20 files | Quick validation |
| **Qualification Genre** | Every push | ~50 files | Genre-specific quick tests |
| **Regression** | Weekly | ~80 files | Comprehensive ground truth |
| **Regression Genre** | Manual | ~289 files | Full genre coverage |
| **Diagnostic** | Manual | Full suite | Debug false positives |

### Running Tests Locally

```bash
# Build first
cargo build --release

# Run qualification tests (requires TestFiles/)
cargo test --test qualification_test -- --nocapture

# Run genre tests (requires GenreTestSuiteLite/)
cargo test --test qualification_genre_test -- --nocapture

# Run all tests
cargo test -- --nocapture
```

---

## Technical Notes

### DSP Concepts Used

1. **Short-Time Fourier Transform (STFT)**: Time-frequency analysis
2. **Mel Filterbank**: Perceptually-motivated frequency scale
3. **Windowing**: Hann, Blackman-Harris for spectral leakage reduction
4. **Pre-emphasis**: High-frequency boost for visualization
5. **Sinc Interpolation**: Band-limited upsampling
6. **Hilbert Transform Approximation**: Envelope extraction
7. **ITU-R BS.1770**: True peak measurement standard

### Limitations

- **High-quality transcodes**: Very high bitrate lossy (320kbps MP3) may not be detectable
- **Sophisticated upsampling**: Modern resamplers may fool the detector
- **Short files**: Less reliable on clips under 5 seconds
- **Synthesized audio**: Electronic music may show unusual but legitimate characteristics
- **Naturally band-limited**: Some recordings legitimately have limited HF content

### Performance

- Typical analysis: 2-5 seconds per file
- Phase analysis adds ~1-2 seconds
- Spectrogram generation adds ~1-3 seconds
- Memory usage: ~100MB for typical 5-minute track

---

## Migration from v0.2.x

If upgrading from v0.2.x, note these breaking changes:

1. **Import paths changed**: 
   - Old: `use audiocheckr::bit_depth::*`
   - New: `use audiocheckr::core::analysis::bit_depth::*`

2. **Library re-exports**: Common types are now available at crate root:
   ```rust
   use audiocheckr::{AudioAnalyzer, ProfileConfig, ProfilePreset};
   ```

3. **New dependencies**: The profile system uses `serde_json` for machine-readable data

---

## Building from Source

Requirements:
- Rust 1.70+ (edition 2021)
- ~300MB disk space for dependencies

```bash
git clone https://github.com/yourusername/audiocheckr
cd audiocheckr
cargo build --release
```

---

## License

GNU Affero General Public License v3.0 (AGPL-3.0)

---

## References

- ITU-R BS.1770: Algorithms for loudness and true-peak measurement
- AES17: Standard method for measuring peak level
- Hydrogenaudio Wiki: Lossy codec detection techniques
- Julius O. Smith: Digital Audio Signal Processing
- Udo Zölzer: DAFX - Digital Audio Effects
