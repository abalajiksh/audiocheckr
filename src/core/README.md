# AudioCheckr Detection Improvements

## Problem Analysis

Based on the diagnostic and regression test results:

1. **Bit Depth False Positives**: ALL 24-bit control files were incorrectly detected as 16-bit
2. **No Spectral Detection**: All files showed 100% cutoff ratio and 0.0 dB/oct steepness - the spectral analyzer was a placeholder

## Root Causes

### Bit Depth Detection Issues

The original algorithm was too aggressive in detecting 16-bit upscaled files:

| Issue | Original Threshold | Problem |
|-------|-------------------|---------|
| LSB trailing zeros | 85% with 8+ zeros → 16-bit | Real 24-bit audio often has some samples with trailing zeros due to dithering |
| Histogram clustering | ratio < 1.5 → 16-bit | Too sensitive to normal audio characteristics |
| Voting | Simple weighted vote | Single noisy detector could trigger false positive |
| Mismatch threshold | confidence > 0.7 | Not strict enough |

### Spectral Analysis Issues

The spectral analyzer was a stub that always returned:
- `frequency_cutoff: nyquist` (100% of Nyquist)
- `rolloff_steepness: 0.0`
- No actual FFT analysis performed

## Fixes Applied

### 1. Bit Depth Detection (`bit_depth.rs`)

**New Conservative Thresholds:**

```
LSB Analysis:
- ratio_exactly_8 > 0.95 && ratio_low_zeros < 0.02 → 16-bit (95% confidence)
- ratio_8plus > 0.90 && ratio_low_zeros < 0.05 → 16-bit (85% confidence)
- ratio_low_zeros > 0.30 → 24-bit (90% confidence)
- ratio_low_zeros > 0.15 → 24-bit (75% confidence)
- Default → 24-bit (60% confidence) [to avoid false positives]
```

**New Histogram Analysis:**
- Uses unique value ratio between 24-bit and 16-bit quantization
- Checks for samples on 256-boundaries (signature of upscaling)
- ratio > 100 → definitely 24-bit
- ratio < 1.2 && boundary_ratio > 0.85 → definitely 16-bit

**New Conservative Voting:**
- Requires **3+ high-confidence (≥80%) 16-bit votes** to flag as 16-bit
- Or 2+ high-confidence votes with 1.5x vote weight advantage
- Otherwise defaults to claimed bit depth

**New Mismatch Threshold:**
- Increased from 0.7 to **0.85** confidence required
- Must be claimed 24-bit, detected 16-bit

### 2. Spectral Analysis (`spectral.rs`)

**Actual FFT Analysis:**
- Computes average spectrum across up to 100 frames
- Uses proper Hann windowing
- Applies smoothing for robust cutoff detection

**Cutoff Detection Algorithm:**
- Finds peak level in mid-frequencies (1-10kHz)
- Searches for where spectrum drops 30dB below peak
- Detects brick-wall cutoffs by steep rolloff (>60 dB/octave)

**Codec Signature Matching:**
- MP3: 64/128/192/256/320 kbps cutoffs
- AAC: 128/256 kbps cutoffs
- Opus: 64/128 kbps cutoffs
- Vorbis: Q3/Q7 cutoffs

**Rolloff Steepness Calculation:**
- Measures dB difference over one octave at cutoff point
- Values >60 dB/octave indicate lossy codec

## Expected Results After Fix

| Category | Before | Expected After |
|----------|--------|----------------|
| Control_Original | 100% (but wrong bit depth) | 100% (correct bit depth) |
| BitDepth_16to24 | 57% | ~90%+ |
| MP3_128_Boundary | 0% | ~80%+ |
| MP3_320_HighQuality | 0% | ~70%+ |
| AAC_256_High | 0% | ~60%+ |
| Opus_128_Mid | 0% | ~50%+ |

## Key Design Principles

1. **Avoid False Positives**: Better to miss a fake lossless than incorrectly flag genuine audio
2. **Require Strong Evidence**: Multiple high-confidence detectors must agree
3. **Default to Trust**: When uncertain, trust the file's claimed properties
4. **Real Analysis**: Actually perform FFT-based spectral analysis, don't use placeholders

## Integration Notes

To integrate these changes:

1. Replace `src/core/analysis/bit_depth.rs` with the new version
2. Replace `src/core/analysis/spectral.rs` with the new version
3. The detector.rs may need updates to use the new spectral analysis results
4. Run the test suite to verify improvements
