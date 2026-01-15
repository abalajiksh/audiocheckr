# AudioCheckr v0.3.7

Advanced audio analysis tool for detecting fake lossless files, transcodes, upsampled audio, and various audio quality issues. Uses pure DSP algorithms—no machine learning.

## Features

### Core Detection Capabilities

| Feature | Description |
|---------|-------------|
| **Lossy Transcode Detection** | Identifies MP3, AAC, Vorbis, and Opus transcodes via spectral analysis |
| **Bit Depth Analysis** | Detects 16-bit audio masquerading as 24-bit using 4 independent methods |
| **Upsampling Detection** | Identifies audio upsampled from lower sample rates |
| **Dithering Detection** | Recognizes RPDF, TPDF, Shibata, noise-shaping algorithms and scale variants |
| **Resampling Detection** | Identifies SWR/SoXR resamplers with quality tier estimation |
| **MQA Detection** | Detects MQA-encoded files via LSB entropy and spectral analysis |
| **ENF Analysis** | Electrical Network Frequency analysis for authenticity verification and edit detection |
| **Clipping Detection** | Comprehensive clipping analysis with true peak, inter-sample overs, and loudness war detection |
| **Stereo Analysis** | Detects joint stereo encoding artifacts |
| **Pre-Echo Detection** | Identifies transform codec artifacts before transients |
| **Phase Analysis** | Detects phase discontinuities at codec frame boundaries |
| **True Peak Analysis** | ITU-R BS.1770 compliant true peak measurement |

---

## Changelog

### v0.3.7 (Current)

- **16-bit MQA Support**: Fixed detection for legacy 16-bit MQA containers (MQAEncode v2.3.x) often found on MQA-CD rips.
- **MQA Detection Accuracy**:
  - Implemented bit-depth aware integer quantization (handling 16-bit vs 24-bit scales).
  - Fixed entropy calculation bugs that caused 24-bit files to read as 0.000 entropy.
  - Tuned thresholds for "Early Encoder" detection to prevent circular logic failures.
- **Allure Reporting**: Integrated comprehensive Allure reporting for MQA test suites.
- **Test Infrastructure**: Fixed ownership and compilation issues in diagnostic and regression tests.

### v0.3.6

- **Modular DSP Integration**: Fully integrated `dithering_detection` and `resampling_detection` modules into the main pipeline.
- **MQA Overhaul**: Replaced simplified MQA stub with a comprehensive `MqaDetector` supporting multi-metric analysis (Entropy, Noise Floor, Spectral Artifacts).
- **Intelligent CI/CD**:
  - **Smart Test Selection**: Automatically determines which tests to run based on changed files (e.g., runs MQA tests only when MQA modules change).
  - **Binary Caching**: Skips rebuilding the binary in CI if source files haven't changed, speeding up pipeline runs.
- **Defect Types**: Added specific `DitheringDetected` and `ResamplingDetected` defect types to the JSON output.

### v0.3.5

- **Genre-Aware Profiles**: Introduction of detection profiles (Electronic, Classical, Noise, etc.).
- **ENF Analysis**: Added Electrical Network Frequency detection.
- **Clipping Detection**: Added broadcast-standard clipping and true-peak analysis.
- **Architecture**: Major refactor into `core`, `cli`, `config`, and `detection` modules.

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
│   │   ├── dither_detection.rs   # Advanced dithering detection
│   │   ├── resample_detection.rs # Resampling engine identification
│   │   ├── mqa_detection.rs  # MQA encoding detection
│   │   ├── enf_detection.rs  # ENF authenticity analysis (NEW)
│   │   ├── clipping_detection.rs # Comprehensive clipping analysis (NEW)
│   │   ├── detection_pipeline.rs # Sample-rate-aware detection orchestration
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
├── detection/                # Detection results
│   └── result.rs             # Profile-aware result types
└── testgen/                  # Test file generation
    └── mod.rs                # FFmpeg-based test file generator
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
audiocheckr -i audio.flac -u --stereo --transients --phase --dithering --resampling -v
```

### ENF and Clipping Analysis

```bash
# Enable ENF (Electrical Network Frequency) analysis for authenticity
audiocheckr -i recording.wav --enf

# Use sensitive ENF mode for noisy recordings
audiocheckr -i noisy_recording.wav --enf --enf-sensitive

# Specify expected ENF frequency (50Hz or 60Hz)
audiocheckr -i audio.wav --enf --enf-frequency 50

# Enable clipping detection (enabled by default)
audiocheckr -i master.flac

# Use strict clipping detection for broadcast compliance
audiocheckr -i broadcast.wav --clipping-strict

# Disable inter-sample peak analysis (faster)
audiocheckr -i audio.flac --no-inter-sample

# Disable loudness war detection
audiocheckr -i audio.flac --no-loudness

# Full extended analysis
audiocheckr -i audio.flac --enf --enf-sensitive --clipping-strict -v
```

### Genre-Aware Profiles

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
        --dithering           Enable dithering detection (24→16 bit)
        --resampling          Enable resampling detection
        --enf                 Enable ENF analysis for authenticity verification
        --enf-sensitive       Use sensitive ENF detection for noisy recordings
        --enf-frequency <HZ>  Expected ENF frequency (50 or 60)
        --no-clipping         Disable clipping detection
        --clipping-strict     Use strict clipping thresholds (broadcast)
        --no-inter-sample     Disable inter-sample peak analysis
        --no-loudness         Disable loudness war detection
    -v, --verbose             Detailed output
        --json                Output as JSON
    -q, --quick               Skip slower analyses
        --min-confidence <N>  Minimum confidence threshold [default: 0.5]
        --profile <NAME>      Detection profile
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
    .check_phase(false)
    .min_confidence(0.6)
    .build(Path::new("audio.flac"))?;

let report = analyzer.analyze()?;
```

### Dithering Detection

```rust
use audiocheckr::core::{DitherDetector, DitherAlgorithm, DitherScale};

let detector = DitherDetector::new(44100);
let result = detector.analyze(&samples, 24);

if result.is_bit_reduced {
    println!("Bit-reduced: {} → {} bit", 
        result.container_bit_depth, result.effective_bit_depth);
    println!("Algorithm: {} (confidence: {:.0}%)", 
        result.algorithm, result.algorithm_confidence * 100.0);
    println!("Scale: {}", result.scale);
}
```

### Resampling Detection

```rust
use audiocheckr::core::{ResampleDetector, ResampleDirection};

let detector = ResampleDetector::new();
let result = detector.analyze(&samples, 96000);

if result.is_resampled {
    println!("Resampled: {} Hz → {} Hz", 
        result.original_sample_rate.unwrap_or(0), result.current_sample_rate);
    println!("Engine: {} ({} quality)", result.engine, result.quality);
    println!("Direction: {:?}", result.direction);
}
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
    .suppress_detector(DetectorType::StereoField)
    .build();

// Check adjusted confidence
let raw_confidence = 0.8;
let adjusted = custom_profile.adjust_confidence(DetectorType::SpectralCutoff, raw_confidence);
```

---

## Detection Algorithms

### Dithering Detection

Detects various dithering algorithms used in bit depth conversion:

| Algorithm | Description | Detectability |
|-----------|-------------|---------------|
| Rectangular (RPDF) | Flat noise distribution ±0.5 LSB | High |
| Triangular (TPDF) | Triangular noise ±1 LSB (most common) | High |
| Triangular HP | High-pass filtered triangular | High |
| Lipshitz | Moderate noise shaping | Medium |
| Shibata | Sony's standard noise shaping | High |
| Low Shibata | Low-frequency optimized | Medium |
| High Shibata | High-frequency optimized | High |
| F-weighted | Psychoacoustic shaping | Medium |
| Modified E-weighted | Enhanced psychoacoustic | Medium |
| Improved E-weighted | Best psychoacoustic shaping | Medium |

**Scale detection**: 0.5x, 0.75x, 1.0x, 1.25x, 1.5x, 2.0x

### Resampling Detection

Detects various resampling engines and quality settings:

| Engine | Variants | Quality Tier |
|--------|----------|--------------|
| SWR Default | Linear interpolation | Standard |
| SWR Cubic | Cubic interpolation | Standard |
| SWR Blackman-Nuttall | High stopband attenuation | High |
| SWR Kaiser | β=9, 12, 16 | High–Very High |
| SoXR Default | Standard precision | High |
| SoXR HQ | precision=20 | Very High |
| SoXR VHQ | precision=28 | Transparent |
| SoXR VHQ Cheby | Chebyshev passband | Transparent |

### MQA Detection

Detects MQA (Master Quality Authenticated) encoding by analyzing:

- **LSB entropy**: MQA stores encoded data in lower 8 bits, creating high entropy (>0.85)
- **High-frequency noise**: Elevated noise floor above 18kHz
- **Spectral artifacts**: Characteristic patterns from MQA's folding process

**Detection criteria**:
- 24-bit files at 44.1/48kHz only (MQA requirement)
- LSB entropy threshold: 0.85
- Noise floor elevation: +15dB above 18kHz

**Tested MQA Encoders** (all pass detection successfully):
- MQAEncode v1.1, 2.5.0+1239 (24-bit)
- MQAEncode v1.1, 2.5.0+1239 (24-bit)
- MQAEncode v1.1, 2.3.3+800 (16-bit)

The MQA detection algorithm is currently fine-grained and future improvements are planned to enhance detection accuracy and encoder version identification.

### ENF (Electrical Network Frequency) Analysis

Detects power grid frequency signatures embedded in audio recordings for authenticity verification:

**Capabilities**:
- **Base frequency detection**: 50 Hz (Europe, Asia, Africa, Australia) vs 60 Hz (North America)
- **Harmonic analysis**: Up to 8 harmonics tracked
- **Frequency tracking**: 1-second windows with temporal analysis
- **Anomaly detection**: Identifies edits, splices, and synthetic audio

**Anomaly Types Detected**:
| Type | Indication |
|------|------------|
| FrequencyJump | Sudden frequency change (potential splice point) |
| PhaseDiscontinuity | Phase break suggesting edit |
| SignalDropout | ENF disappears (processed/synthetic section) |
| DriftRateChange | Frequency drift pattern change |
| HarmonicAnomaly | Unusual harmonic ratios |

**Geographic Region Estimation**:
- 50 Hz regions: Europe, UK, Asia, Africa, Australia, Japan (east)
- 60 Hz regions: North America, Central America, Japan (west), parts of South America

**Detection Modes**:
- **Default**: 32768 FFT, 3.0 dB min SNR, ±0.5 Hz tolerance
- **Sensitive**: 65536 FFT, 1.5 dB min SNR (for noisy recordings)
- **Fast**: 16384 FFT, reduced accuracy for quick screening

### Clipping Detection

Comprehensive digital clipping analysis with multiple detection methods:

**Analysis Features**:
| Feature | Description |
|---------|-------------|
| Sample Clipping | Samples at digital maximum (±1.0) |
| True Peak (ITU-R BS.1770) | 4x oversampled peak detection |
| Inter-Sample Overs | Peaks between samples exceeding 0 dBFS |
| Soft Clipping | Analog-style saturation detection |
| Limiter Artifacts | Heavy limiting pattern detection |

**Clipping Types**:
| Type | Characteristics |
|------|-----------------|
| HardDigital | Flat-top clipping at digital max |
| SoftAnalog | Rounded saturation (tube/tape-style) |
| Limiter | Short, controlled peaks |
| IntentionalDistortion | Effect processing artifacts |

**Loudness Analysis**:
- Integrated loudness (LUFS approximation)
- Loudness range (LU)
- Dynamic range (DR)
- Crest factor
- Peak-to-loudness ratio (PLR)
- **Loudness war victim detection**: Identifies over-compressed masters

**Restoration Assessment**:
| Clipping Severity | Recommended Method | Expected Recovery |
|-------------------|-------------------|-------------------|
| Minor (<5 samples) | CubicSpline | ~95% |
| Moderate (<20 samples) | SpectralReconstruction | ~80% |
| Significant (<50 samples) | NeuralNetwork | ~60% |
| Severe (>50 samples) | NotRestorable | ~20% |
| Inter-sample only | GainReduction | 100% |

**Detection Modes**:
- **Default**: 0.9999 threshold, 1 sample minimum
- **Strict**: 0.999 threshold (broadcast standards)
- **Lenient**: 1.0 threshold, 3 sample minimum (music production)

### Spectral Analysis

Multi-method frequency cutoff detection:

| Method | Description |
|--------|-------------|
| Energy Drop | Finds where energy drops 25dB below reference |
| Derivative | Edge detection via spectral derivative |
| Noise Floor | Compares signal to noise floor near Nyquist |

**Codec signatures matched**:
- MP3: 64, 96, 128, 160, 192, 224, 256, 320 kbps
- AAC: 96, 128, 160, 192, 256, 320 kbps
- Opus: 48, 64, 96, 128, 192 kbps
- Vorbis: Q3–Q9

### Bit Depth Analysis

Four independent detection methods with weighted voting:

1. **LSB Precision**: Examines trailing zeros in 24-bit scaled samples
2. **Histogram Analysis**: Counts unique values at 16-bit vs 24-bit quantization
3. **Quantization Noise**: Measures noise floor in quiet sections
4. **Value Clustering**: Checks if values cluster on 256-multiples

**Conservative thresholds**: Requires 3+ high-confidence (≥85%) methods to agree before flagging.

---

## Output Interpretation

### Quality Score

| Score | Interpretation |
|-------|----------------|
| 90–100% | Likely true lossless |
| 70–90% | Possibly lossless with minor issues |
| 50–70% | Suspicious—likely transcoded |
| 0–50% | Almost certainly transcoded |

### Verdict

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
| MP3/AAC/Vorbis/Opus Transcode | File encoded from lossy source |
| Bit Depth Mismatch | 16-bit audio padded to 24-bit |
| Upsampled | Audio upsampled from lower sample rate |
| Dithering Detected | Bit depth reduction with dithering applied |
| Resampling Detected | Sample rate conversion detected |
| MQA Encoded | MQA encoding detected in LSBs |
| Joint Stereo | Lossy joint stereo encoding |
| Pre-Echo | Transform codec artifacts |
| Phase Discontinuities | Codec frame boundary artifacts |
| Clipping | Samples at/above full scale |
| Inter-Sample Overs | True peak exceeds 0 dBFS |

---

## CI/CD Pipeline

AudioCheckr uses Jenkins for continuous integration with multiple test types:

### Test Types

| Test Type | Trigger | Files | Purpose |
|-----------|---------|-------|---------|
| `QUALIFICATION_GENRE` | Every push | ~50 files | Quick genre-specific validation |
| `REGRESSION_GENRE` | Weekly (Sat 10AM) / Manual | ~289 files | Comprehensive regression |
| `DIAGNOSTIC` | Manual | Full suite | Debug false positives |
| `DSP_TEST` | Manual | Dithering + Resampling | DSP artifact detection |
| `DSP_DIAGNOSTIC` | Manual | Dithering + Resampling | Detailed DSP diagnostics |
| `MQA_TEST` | Manual | MQA folder | MQA detection validation |

### Jenkins Pipeline Features

- **MinIO Integration**: Test files stored in MinIO bucket (`audiocheckr`)
- **Allure Reporting**: Beautiful test reports with environment info
- **SonarQube Analysis**: Code quality metrics (optional)
- **Automatic Cleanup**: Workspace cleaned after each build
- **Artifact Archiving**: Release binary and test results preserved

### Running Tests Locally

```bash
# Build release binary
cargo build --release

# Run qualification tests (requires GenreTestSuiteLite/)
cargo test --test qualification_genre_test --release -- --nocapture

# Run DSP tests (requires dithering_tests/ and resampling_tests/)
cargo test --test dithering_resampling_test --release -- --nocapture --test-threads=1

# Run MQA tests (requires MQA/ folder)
cargo test --test mqa_test --release -- --ignored --nocapture --test-threads=1

# Run all tests
cargo test --release -- --nocapture
```

### Manual Jenkins Trigger

1. Navigate to AudioCheckr job in Jenkins
2. Click "Build with Parameters"
3. Select test type from dropdown:
   - `MQA_TEST` for MQA detection tests
   - `DSP_TEST` for dithering/resampling tests
   - `DSP_DIAGNOSTIC` for detailed DSP analysis
4. Click "Build"

---

## Test File Generation

AudioCheckr includes utilities for generating test files with known characteristics:

```rust
use audiocheckr::testgen::{TestFileGenerator, DitherConfig, DitherMethod, ResampleConfig, ResamplerConfig};

let generator = TestFileGenerator::new("./test_output")?;

// Generate dithered file
let config = DitherConfig {
    method: DitherMethod::Shibata,
    scale: 1.0,
    output_bits: 16,
};
generator.generate_dithered("source.flac", &config)?;

// Generate resampled file
let config = ResampleConfig {
    target_rate: 96000,
    engine: ResamplerConfig::SoxrVHQ,
};
generator.generate_resampled("source.flac", &config)?;

// Generate complete test suite
generator.generate_all_dithered("source.flac")?;
generator.generate_all_resampled("source.flac", &[44100, 48000, 96000])?;
```

---

## Technical Notes

### DSP Concepts Used

- **Short-Time Fourier Transform (STFT)**: Time-frequency analysis
- **Mel Filterbank**: Perceptually-motivated frequency scale
- **Windowing**: Hann, Blackman-Harris for spectral leakage reduction
- **Sinc Interpolation**: Band-limited upsampling for true peak
- **Shannon Entropy**: LSB randomness measurement for MQA detection
- **ITU-R BS.1770**: True peak measurement standard

### Sample Rate Awareness

The detection pipeline is sample-rate-aware:

- **>48kHz files**: MP3/AAC detection automatically skipped (codecs don't support these rates)
- **High-res files**: Reduced spectral cutoff sensitivity to avoid false positives
- **Anti-aliasing detection**: Distinguishes resampler rolloff from lossy codec cutoff

### Performance

| Operation | Typical Time |
|-----------|--------------|
| Basic analysis | 2–5 seconds/file |
| With phase analysis | +1–2 seconds |
| Spectrogram generation | +1–3 seconds |
| Dithering detection | +0.5 seconds |
| Resampling detection | +1 second |
| Memory usage | ~100MB for 5-minute track |

### Limitations

- **High-quality transcodes**: 320kbps MP3 may not be detectable
- **Sophisticated upsampling**: Modern resamplers may fool the detector
- **Short files**: Less reliable on clips under 5 seconds
- **Synthesized audio**: Electronic music may show unusual but legitimate characteristics
- **Naturally band-limited**: Some recordings legitimately have limited HF content

---

## Migration from v0.2.x

### Breaking Changes

1. **Import paths changed**:
   ```rust
   // Old
   use audiocheckr::bit_depth::*;
   // New
   use audiocheckr::core::analysis::bit_depth::*;
   ```

2. **New library re-exports**:
   ```rust
   use audiocheckr::{AudioAnalyzer, ProfileConfig, ProfilePreset};
   use audiocheckr::{DitherDetector, ResampleDetector};
   ```

3. **DetectionConfig expanded**:
   ```rust
   DetectionConfig {
       // Existing fields...
       check_dithering: true,   // NEW
       check_resampling: true,  // NEW
   }
   ```

4. **New DefectTypes**:
   - `DitheringDetected { algorithm, scale, effective_bits, container_bits }`
   - `ResamplingDetected { original_rate, current_rate, engine, quality }`
   - `MqaEncoded { original_rate, mqa_type, lsb_entropy }`

---

## Building from Source

### Requirements

- Rust 1.70+ (edition 2021)
- ~300MB disk space for dependencies

### Build

```bash
git clone https://github.com/abalajiksh/audiocheckr
cd audiocheckr
cargo build --release
```

### Dependencies

Key crates:
- `symphonia` - Audio decoding
- `rustfft` - FFT processing
- `clap` - CLI argument parsing
- `image` - Spectrogram generation
- `serde` / `serde_json` - Serialization
- `colorful` - Terminal colors
- `walkdir` - Directory traversal
- `anyhow` - Error handling

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
- MQA Ltd: Technical papers on MQA encoding
- Grigoras, C.: Digital audio recording analysis - the ENF criterion
- IEEE: Applications of Electrical Network Frequency for audio forensics
