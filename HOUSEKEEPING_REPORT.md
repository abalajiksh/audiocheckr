# Housekeeping Summary - AudioCheckr Cleanup Report

Generated: 2026-02-17

## Phase 1: Completed - Dead Code Removal ✅

### Deleted Files (Redundant Type Systems)

1. **src/detection/mod.rs** - Removed
2. **src/detection/result.rs** (8KB) - Removed
   - Duplicate AnalysisResult type
   - Profile-aware system never integrated
   - Not imported anywhere

3. **src/core/analyzer.rs** (4KB) - Removed
   - Broken wrapper referencing deleted code
   - Used non-existent `detect_quality_issues()` function
   - AudioDetector is used directly instead

4. **src/core/analysis/mfcc.rs** (1KB) - Removed
   - Only stub/TODO comments

**Total removed so far: ~13KB**

---

## Phase 2: Files Kept for Future Integration

### Useful Analysis Modules (Currently Unused by AudioDetector)

These files have real implementations but aren't called yet:

| File | Size | Status | Integration Plan |
|------|------|--------|------------------|
| **bit_depth.rs** | 18KB | KEEP | More sophisticated than inline version in detector.rs |
| **spectral.rs** | 23KB | KEEP | Utility functions, SpectralAnalyzer in dsp/ is used instead |
| **upsampling.rs** | 3KB | KEEP | Dedicated detector, currently inline in detector.rs |
| **true_peak.rs** | 4KB | KEEP | ITU-R BS.1770 compliant - needed for broadcast |
| **stereo.rs** | 3KB | KEEP | Stereo imaging analysis - useful feature |
| **transients.rs** | 6KB | KEEP | Transient detection for pre-echo artifacts |
| **phase.rs** | 1KB | KEEP (STUB) | Phase coherence analysis stub |
| **enf_detection.rs** | 31KB | KEEP | Complete ENF implementation for future |

**Total useful code kept: ~89KB**

---

## Current Active Architecture

### What AudioDetector Actually Uses

From `src/core/detector.rs`:

```
Detection Pipeline:
├── dithering_detection.rs ✅ (external module)
├── resampling_detection.rs ✅ (external module)  
├── detect_spectral_cutoff() → SpectralAnalyzer (dsp/mod.rs) ✅
├── detect_upsampling() (inline in detector.rs)
├── detect_bit_depth_inflation() (inline in detector.rs)
├── mqa_detection.rs ✅ (external module)
├── clipping_detection.rs ✅ (external module)
└── dynamic_range.rs ✅ (external module)
```

### Config System Status

`src/config/` directory defines:
- `DetectorType` enum
- `ProfileConfig` for genre-aware detection
- `ProfilePreset` for different use cases

**Status**: Referenced by deleted `detection/result.rs` but **ProfileConfig types are exported publicly** - may be intended for future use.

**Recommendation**: KEEP for now - could be useful for implementing genre-aware thresholds.

---

## Integration Recommendations

### High Priority (Would Improve Detection)

1. **true_peak.rs** → Replace basic true_peak calculation in `calculate_quality_metrics()`
   - Currently uses simple `20.0 * max_sample.log10()`
   - true_peak.rs has proper ITU-R BS.1770 oversampling
   
2. **stereo.rs** → Add stereo field analysis
   - Detect fake stereo (duplicated mono)
   - Detect narrow stereo image
   - Integration: Add to detection pipeline

3. **transients.rs** → Detect lossy encoding artifacts
   - Pre-echo detection (common in MP3/AAC)
   - Integration: Add to detection pipeline

### Medium Priority

4. **bit_depth.rs** → Replace inline bit depth detection
   - More sophisticated LSB entropy analysis
   - Better quantization noise detection
   - Integration: Replace `detect_bit_depth_inflation()` in detector.rs

5. **upsampling.rs** → Replace inline upsampling detection  
   - Dedicated implementation
   - Integration: Replace `detect_upsampling()` in detector.rs

### Low Priority (Future Features)

6. **enf_detection.rs** → ENF authenticity verification
   - Already has CLI flag (--enf) defined
   - Integration: Add to pipeline when config.enable_enf is true

7. **phase.rs** → Implement phase coherence analysis
   - Currently just stub
   - Useful for detecting phase issues

---

## Summary

✅ **Deleted**: 13KB of dead/broken code
✅ **Kept**: 89KB of useful features for future integration  
✅ **Active**: ~22KB core detection (detector.rs)

### Clean Codebase Structure

```
src/
├── cli/          ✅ CLI and output formatting
├── config/       ✅ Profile system (for future genre-aware detection)
├── core/
│   ├── analysis/
│   │   ├── clipping_detection.rs      ✅ USED
│   │   ├── detection_pipeline.rs      ❓ Check if used
│   │   ├── dithering_detection.rs     ✅ USED
│   │   ├── dynamic_range.rs           ✅ USED
│   │   ├── enf_detection.rs           💾 KEEP (future)
│   │   ├── mqa_detection.rs           ✅ USED
│   │   ├── resampling_detection.rs    ✅ USED
│   │   ├── bit_depth.rs               💾 KEEP (better impl)
│   │   ├── spectral.rs                💾 KEEP (utilities)
│   │   ├── stereo.rs                  💾 KEEP (integrate)
│   │   ├── transients.rs              💾 KEEP (integrate)
│   │   ├── true_peak.rs               💾 KEEP (integrate)
│   │   ├── upsampling.rs              💾 KEEP (better impl)
│   │   └── phase.rs                   💾 KEEP (stub)
│   ├── detector.rs        ✅ Main orchestrator
│   ├── decoder.rs         ✅ Audio decoding
│   └── dsp/               ✅ DSP primitives
├── testgen/      ✅ Test file generation
└── main.rs       ✅ CLI entry point
```

---

## Next Actions

**Option A**: Keep as-is, integrate features later when needed

**Option B**: Integrate high-priority features now:
1. Replace true_peak calculation with true_peak.rs
2. Add stereo.rs to detection pipeline
3. Add transients.rs to detection pipeline

**Option C**: Remove unused analysis files, re-add when needed

Your choice! The codebase is now clean of duplicate/broken code.
