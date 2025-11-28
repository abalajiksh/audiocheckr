# Audio Quality Checker v0.2.1

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

### v0.2.1 Improvements

**Fixed false positives on high sample rate files (88.2kHz+)**

The previous version used ratio-based detection (cutoff / Nyquist) which caused massive false positive rates on high-res files. For example, a genuine 96kHz file with content up to 20kHz has a cutoff ratio of only 42% - well below the old 85% threshold.

The new algorithm:
- Uses **absolute frequency thresholds** for high sample rate files (≥88.2kHz)
- Treats content up to 22kHz as normal for any sample rate
- Requires **positive codec identification** (brick-wall, steepness, shelf pattern) before flagging
- **No default fallback** - if a codec can't be positively identified, the file isn't flagged
- Maintains ratio-based detection for standard sample rates (44.1/48kHz)

### Analysis Methods

Each detection uses multiple algorithms with confidence scoring:

#### Spectral Analysis (`spectral.rs`)
- **Multi-frame FFT**: Analyzes 30 frames spread across the track (not just one window)
- **Derivative-based cutoff detection**: Finds where spectrum "falls off a cliff"
- **Rolloff steepness measurement**: dB/octave calculation
- **Brick-wall detection**: Sharp cutoffs characteristic of MP3
- **Shelf pattern detection**: Characteristic of AAC encoding
- **Encoder signature matching**: Compares against known codec signatures

#### Transcode Detection (`detector.rs`)

**For high sample rate files (88.2kHz+):**

| Cutoff Frequency | Evidence Required | Rationale |
|------------------|-------------------|-----------|
| > 22 kHz | None - pass | Normal high-res content |
| 20-22 kHz | Brick-wall AND >80 dB/oct | Could be MP3 320k or natural |
| 18-20 kHz | Brick-wall OR >60 dB/oct | Suspicious but needs evidence |
| 15-18 kHz | 2+ signals (brick-wall, steepness, shelf) | More suspicious |
| 10-15 kHz | Codec-specific signature | Low content, check for codec |
| < 10 kHz | Brick-wall required | Very suspicious if sharp cutoff |

**For standard sample rates (44.1/48kHz):**

| Cutoff Ratio | Evidence Required |
|--------------|-------------------|
| ≥ 80% | None - pass |
| 70-80% | Brick-wall OR >40 dB/oct |
| < 70% | Flag with ratio-based confidence |

**Codec Classification (must match one to flag):**
- **MP3**: Brick-wall + steepness >50 dB/oct, cutoff 15-20.5 kHz
- **AAC**: Shelf pattern detected
- **Opus**: Brick-wall at specific frequencies (8kHz, 12kHz, 20kHz modes)
- **Vorbis**: Soft rolloff (15-45 dB/oct), no brick-wall, cutoff 12-19 kHz, quality ≤6

#### Bit Depth Analysis (`bit_depth.rs`)
Four independent detection methods:
1. **LSB Precision Analysis**: Examines trailing zeros in 24-bit scaled samples
2. **Histogram Analysis**: Counts unique values at 16-bit vs 24-bit quantization
3. **Quantization Noise Analysis**: Measures noise floor in quiet sections
4. **Value Clustering Analysis**: Checks if values cluster on 256-multiples

Results are combined using weighted voting for robust detection.

#### Upsampling Detection (`upsampling.rs`)
Three detection methods:
1. **Spectral Method**: Compares frequency cutoff to original Nyquist frequencies
2. **Null Test**: Downsample → upsample and compare spectra
3. **Inter-sample Peak Analysis**: True high-res has inter-sample peaks; upsampled doesn't

#### Stereo Analysis (`stereo.rs`)
- Stereo width measurement (M/S energy ratio)
- Channel correlation calculation
- Frequency-dependent stereo width analysis
- Joint stereo detection via HF stereo reduction

#### Pre-Echo Analysis (`transients.rs`)
- Transient detection using envelope following
- Pre-transient energy measurement
- MDCT window size pattern detection (for MP3 ~25ms)
- Frame boundary artifact detection

#### Phase Analysis (`phase.rs`)
- Phase coherence between consecutive frames
- Phase discontinuity scoring
- Instantaneous frequency deviation analysis

#### True Peak Analysis (`true_peak.rs`)
- 4x oversampling via sinc interpolation
- Inter-sample over detection
- Clipping detection
- Dynamic range estimation (percentile method)
- Crest factor calculation

## Installation

```bash
cargo install --path .
```

Or build manually:
```bash
cargo build --release
```

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
```

### Examples

```bash
# Quick check for transcodes
audiocheckr -i album/ -q

# Full analysis with spectrogram
audiocheckr -i "track.flac" -s -u --stereo --transients -v

# Batch analysis with JSON output
audiocheckr -i music_library/ --json > results.json

# Check if upsampled from CD quality
audiocheckr -i hi-res-album/ -u -v
```

## Output Interpretation

### Quality Score
- **90-100%**: Likely true lossless
- **70-90%**: Possibly lossless but has some issues
- **50-70%**: Suspicious - likely transcoded
- **0-50%**: Almost certainly transcoded

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

## Algorithm Details

### Frequency Cutoff Detection

The detector uses a derivative-based approach:

1. Compute 30 FFT frames spread across the track
2. Convert to dB scale and smooth
3. Starting from high frequencies, find where signal rises above noise floor
4. Check for "cliff" patterns (sharp drops over one octave)
5. Use median of all frames to reject outliers

This is more robust than single-frame analysis which can miss quiet passages or be fooled by transients.

### High Sample Rate Handling (v0.2.1)

**The Problem:** At 96kHz sample rate, Nyquist is 48kHz. Music content naturally stops around 20kHz (human hearing limit), giving a cutoff ratio of only 42%. The old 85% threshold flagged everything.

**The Solution:** For files ≥88.2kHz, use absolute frequency thresholds:
- Content to 22kHz is **normal** regardless of sample rate
- Require **codec-specific evidence** (brick-wall, steepness pattern, shelf) to flag
- Never flag based on cutoff alone
- No default "must be Vorbis" fallback

### Bit Depth Detection

The four-method approach handles edge cases:

```
LSB Analysis: Checks if lower 8 bits of 24-bit samples are meaningful
             16-bit upscaled → many samples with 8+ trailing zeros
             True 24-bit → varied LSB patterns

Histogram:   True 24-bit has ~256x more unique values than 16-bit
             Ratio close to 1 = definitely 16-bit padded

Noise Floor: True 24-bit has noise floor around -144 dBFS
             16-bit has noise floor around -96 dBFS

Clustering:  16-bit padded → values cluster on multiples of 256
             True 24-bit → uniform distribution of LSBs
```

### Pre-Echo Detection

MP3/AAC use MDCT which spreads energy across a ~25ms window. Before transients, this creates audible "pre-echo":

1. Find transients via envelope analysis
2. Measure energy in 25ms window before each transient
3. Compare to earlier reference windows
4. High ratio = pre-echo present

### Encoder Signatures

Known characteristics:
- **MP3**: Brick-wall cutoff, ~15-20kHz depending on bitrate, steep rolloff (>50 dB/oct)
- **AAC**: Softer rolloff, often shows "shelf" pattern before cutoff
- **Vorbis**: Variable cutoff, smoother rolloff (15-45 dB/oct), no brick-wall
- **Opus**: Very sharp cutoff at specific frequencies (8kHz, 12kHz, 20kHz modes)

## Library Usage

```rust
use audiocheckr::{AudioAnalyzer, AnalyzerBuilder, is_likely_lossless};
use std::path::Path;

// Quick check
let is_lossless = is_likely_lossless(Path::new("audio.flac"))?;

// Full analysis
let analyzer = AudioAnalyzer::new(Path::new("audio.flac"))?;
let report = analyzer.analyze()?;

println!("Quality score: {:.0}%", report.quality_score * 100.0);
println!("Likely lossless: {}", report.is_likely_lossless);

for defect in &report.defects {
    println!("Defect: {:?} (confidence: {:.0}%)", 
        defect.defect_type, defect.confidence * 100.0);
}

// Builder pattern for custom configuration
let analyzer = AnalyzerBuilder::new()
    .expected_bit_depth(24)
    .check_upsampling(true)
    .check_stereo(true)
    .check_transients(true)
    .min_confidence(0.6)
    .build(Path::new("audio.flac"))?;
```

## Technical Notes

### DSP Concepts Used

1. **Short-Time Fourier Transform (STFT)**: Time-frequency analysis
2. **Mel Filterbank**: Perceptually-motivated frequency scale
3. **Windowing**: Hann, Blackman-Harris windows for spectral leakage reduction
4. **Pre-emphasis**: High-frequency boost for better visualization
5. **Sinc Interpolation**: Band-limited upsampling
6. **Hilbert Transform Approximation**: Envelope extraction
7. **Phase Vocoder Principles**: Instantaneous frequency analysis
8. **ITU-R BS.1770**: True peak measurement standard

### Limitations

- **High-quality transcodes**: Very high bitrate lossy (320kbps MP3, 256kbps AAC) may not be detectable
- **Sophisticated upsampling**: Modern resamplers with steep anti-aliasing may fool the detector
- **Short files**: Less reliable on clips under 5 seconds
- **Synthesized audio**: Electronic music may show unusual but legitimate spectral characteristics
- **Naturally band-limited content**: Some recordings (old jazz, classical, ambient) legitimately have limited high-frequency content

### Performance

- Typical analysis: 2-5 seconds per file
- Phase analysis adds ~1-2 seconds
- Spectrogram generation adds ~1-3 seconds
- Memory usage: ~100MB for typical 5-minute track

## Changelog

### v0.2.1
- **Fixed**: False positives on high sample rate files (88.2kHz, 96kHz, 176.4kHz, 192kHz)
- **Changed**: Use absolute frequency thresholds instead of ratio-based for high-res files
- **Changed**: Require positive codec identification before flagging transcodes
- **Removed**: Default "Vorbis" fallback for unidentified cutoffs
- **Improved**: More conservative thresholds for borderline cases

### v0.2.0
- Initial release with comprehensive audio analysis

## Building from Source

Requirements:
- Rust 1.70+ (edition 2021)
- ~300MB disk space for dependencies

```bash
git clone https://github.com/yourusername/audiocheckr
cd audiocheckr
cargo build --release
```

## License

GNU Affero General Public License v3.0 (AGPL-3.0)

## References

- ITU-R BS.1770: Algorithms for loudness and true-peak measurement
- AES17: Standard method for measuring peak level
- Hydrogenaudio Wiki: Lossy codec detection techniques
- Julius O. Smith: Digital Audio Signal Processing
- Udo Zölzer: DAFX - Digital Audio Effects
