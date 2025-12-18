// src/testgen/mod.rs
//
// Test file generation utilities for AudioCheckr
// Generates test files with various dithering and resampling configurations
// for testing and validation of detection algorithms.
//
// This mirrors the functionality of the PowerShell scripts but provides
// a Rust-native implementation for CI/CD pipelines.

use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, Context, bail};

/// Dithering configuration for test file generation
#[derive(Debug, Clone)]
pub struct DitherConfig {
    /// Dithering method name (FFmpeg's aresample dither_method)
    pub method: DitherMethod,
    /// Dither scale (amplitude multiplier)
    pub scale: f32,
    /// Output bit depth (typically 16)
    pub output_bits: u8,
}

/// FFmpeg dithering methods available via aresample filter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherMethod {
    /// No dithering
    None,
    /// Rectangular PDF (RPDF)
    Rectangular,
    /// Triangular PDF (TPDF) - most common
    Triangular,
    /// High-pass triangular PDF
    TriangularHP,
    /// Lipshitz noise shaping
    Lipshitz,
    /// Shibata noise shaping
    Shibata,
    /// Low-frequency optimized Shibata
    LowShibata,
    /// High-frequency optimized Shibata  
    HighShibata,
    /// F-weighted psychoacoustic shaping
    FWeighted,
    /// Modified E-weighted noise shaping
    ModifiedEWeighted,
    /// Improved E-weighted noise shaping
    ImprovedEWeighted,
}

impl DitherMethod {
    /// Get FFmpeg filter parameter name
    pub fn ffmpeg_name(&self) -> &'static str {
        match self {
            DitherMethod::None => "none",
            DitherMethod::Rectangular => "rectangular",
            DitherMethod::Triangular => "triangular",
            DitherMethod::TriangularHP => "triangular_hp",
            DitherMethod::Lipshitz => "lipshitz",
            DitherMethod::Shibata => "shibata",
            DitherMethod::LowShibata => "low_shibata",
            DitherMethod::HighShibata => "high_shibata",
            DitherMethod::FWeighted => "f_weighted",
            DitherMethod::ModifiedEWeighted => "modified_e_weighted",
            DitherMethod::ImprovedEWeighted => "improved_e_weighted",
        }
    }
    
    /// Get all available dithering methods
    pub fn all() -> Vec<Self> {
        vec![
            Self::Rectangular,
            Self::Triangular,
            Self::TriangularHP,
            Self::Lipshitz,
            Self::Shibata,
            Self::LowShibata,
            Self::HighShibata,
            Self::FWeighted,
            Self::ModifiedEWeighted,
            Self::ImprovedEWeighted,
        ]
    }
}

impl std::fmt::Display for DitherMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ffmpeg_name())
    }
}

/// Resampling configuration for test file generation
#[derive(Debug, Clone)]
pub struct ResampleConfig {
    /// Target sample rate in Hz
    pub target_rate: u32,
    /// Resampler engine and settings
    pub engine: ResamplerConfig,
}

/// Resampler engine configuration
#[derive(Debug, Clone)]
pub enum ResamplerConfig {
    /// FFmpeg SWR default
    SwrDefault,
    /// SWR with cubic interpolation
    SwrCubic,
    /// SWR with Blackman-Nuttall window
    SwrBlackmanNuttall,
    /// SWR with Kaiser window
    SwrKaiser { beta: u8 },
    /// SWR with custom filter size
    SwrFilterSize { size: u16 },
    /// SoXR default quality
    SoxrDefault,
    /// SoXR high quality (precision=20)
    SoxrHQ,
    /// SoXR very high quality (precision=28)
    SoxrVHQ,
    /// SoXR VHQ with Chebyshev passband
    SoxrVHQCheby,
    /// SoXR with custom cutoff
    SoxrCutoff { cutoff: f32 },
}

impl ResamplerConfig {
    /// Get FFmpeg filter parameters
    pub fn ffmpeg_filter(&self) -> String {
        match self {
            ResamplerConfig::SwrDefault => String::new(),
            ResamplerConfig::SwrCubic => "aresample=filter_type=cubic".to_string(),
            ResamplerConfig::SwrBlackmanNuttall => "aresample=filter_type=blackman_nuttall".to_string(),
            ResamplerConfig::SwrKaiser { beta } => format!("aresample=filter_type=kaiser:kaiser_beta={}", beta),
            ResamplerConfig::SwrFilterSize { size } => format!("aresample=filter_size={}", size),
            ResamplerConfig::SoxrDefault => "aresample=resampler=soxr".to_string(),
            ResamplerConfig::SoxrHQ => "aresample=resampler=soxr:precision=20".to_string(),
            ResamplerConfig::SoxrVHQ => "aresample=resampler=soxr:precision=28".to_string(),
            ResamplerConfig::SoxrVHQCheby => "aresample=resampler=soxr:precision=28:cheby=1".to_string(),
            ResamplerConfig::SoxrCutoff { cutoff } => format!("aresample=resampler=soxr:cutoff={}", cutoff),
        }
    }
    
    /// Get a short name for file naming
    pub fn short_name(&self) -> String {
        match self {
            ResamplerConfig::SwrDefault => "swr_default".to_string(),
            ResamplerConfig::SwrCubic => "swr_cubic".to_string(),
            ResamplerConfig::SwrBlackmanNuttall => "swr_blackman_nuttall".to_string(),
            ResamplerConfig::SwrKaiser { beta } => format!("swr_kaiser_beta{}", beta),
            ResamplerConfig::SwrFilterSize { size } => format!("swr_filter_size_{}", size),
            ResamplerConfig::SoxrDefault => "soxr_default".to_string(),
            ResamplerConfig::SoxrHQ => "soxr_hq".to_string(),
            ResamplerConfig::SoxrVHQ => "soxr_vhq".to_string(),
            ResamplerConfig::SoxrVHQCheby => "soxr_vhq_cheby".to_string(),
            ResamplerConfig::SoxrCutoff { cutoff } => format!("soxr_cutoff_{}", (cutoff * 100.0) as u8),
        }
    }
    
    /// Get all standard resampler configurations
    pub fn all_swr() -> Vec<Self> {
        vec![
            Self::SwrDefault,
            Self::SwrCubic,
            Self::SwrBlackmanNuttall,
            Self::SwrKaiser { beta: 9 },
            Self::SwrKaiser { beta: 12 },
            Self::SwrKaiser { beta: 16 },
            Self::SwrFilterSize { size: 16 },
            Self::SwrFilterSize { size: 64 },
        ]
    }
    
    /// Get all SoXR configurations
    pub fn all_soxr() -> Vec<Self> {
        vec![
            Self::SoxrDefault,
            Self::SoxrHQ,
            Self::SoxrVHQ,
            Self::SoxrVHQCheby,
            Self::SoxrCutoff { cutoff: 0.91 },
            Self::SoxrCutoff { cutoff: 0.95 },
        ]
    }
}

/// Test file generator using FFmpeg
pub struct TestFileGenerator {
    /// Path to FFmpeg binary
    ffmpeg_path: PathBuf,
    /// Output directory
    output_dir: PathBuf,
}

impl TestFileGenerator {
    /// Create a new generator
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&output_dir)?;
        
        // Find FFmpeg
        let ffmpeg_path = which::which("ffmpeg")
            .context("FFmpeg not found in PATH")?;
        
        Ok(Self {
            ffmpeg_path,
            output_dir,
        })
    }
    
    /// Generate a dithered test file
    pub fn generate_dithered<P: AsRef<Path>>(
        &self,
        input: P,
        config: &DitherConfig,
    ) -> Result<PathBuf> {
        let input = input.as_ref();
        let stem = input.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        
        let output_name = format!(
            "{}_dither_{}_scale{}.flac",
            stem,
            config.method.ffmpeg_name(),
            format!("{:.2}", config.scale).replace('.', "_")
        );
        let output_path = self.output_dir.join(&output_name);
        
        // Build FFmpeg command
        let filter = format!(
            "aresample=out_sample_fmt=s{}:dither_method={}:dither_scale={}",
            config.output_bits,
            config.method.ffmpeg_name(),
            config.scale
        );
        
        let status = Command::new(&self.ffmpeg_path)
            .args([
                "-i", input.to_str().unwrap(),
                "-af", &filter,
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to execute FFmpeg")?;
        
        if !status.status.success() {
            bail!(
                "FFmpeg failed: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        
        Ok(output_path)
    }
    
    /// Generate a resampled test file
    pub fn generate_resampled<P: AsRef<Path>>(
        &self,
        input: P,
        config: &ResampleConfig,
    ) -> Result<PathBuf> {
        let input = input.as_ref();
        let stem = input.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        
        let output_name = format!(
            "{}_{}Hz_{}.flac",
            stem,
            config.target_rate,
            config.engine.short_name()
        );
        let output_path = self.output_dir.join(&output_name);
        
        // Build FFmpeg command
        let mut args: Vec<String> = vec![
            "-i".to_string(),
            input.to_str().unwrap().to_string(),
        ];
        
        let filter = config.engine.ffmpeg_filter();
        if !filter.is_empty() {
            args.push("-af".to_string());
            args.push(filter);
        }
        
        args.push("-ar".to_string());
        args.push(config.target_rate.to_string());
        args.push("-y".to_string());
        args.push(output_path.to_str().unwrap().to_string());
        
        let status = Command::new(&self.ffmpeg_path)
            .args(&args)
            .output()
            .context("Failed to execute FFmpeg")?;
        
        if !status.status.success() {
            bail!(
                "FFmpeg failed: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        
        Ok(output_path)
    }
    
    /// Generate all dithering test files with standard scales
    pub fn generate_all_dithered<P: AsRef<Path>>(
        &self,
        input: P,
    ) -> Result<Vec<PathBuf>> {
        let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
        let methods = DitherMethod::all();
        
        let mut outputs = Vec::new();
        
        for method in &methods {
            for &scale in &scales {
                let config = DitherConfig {
                    method: *method,
                    scale,
                    output_bits: 16,
                };
                
                match self.generate_dithered(&input, &config) {
                    Ok(path) => {
                        println!("Generated: {}", path.display());
                        outputs.push(path);
                    }
                    Err(e) => {
                        eprintln!("Failed to generate {} scale {}: {}", method, scale, e);
                    }
                }
            }
        }
        
        Ok(outputs)
    }
    
    /// Generate all resampling test files
    pub fn generate_all_resampled<P: AsRef<Path>>(
        &self,
        input: P,
        target_rates: &[u32],
    ) -> Result<Vec<PathBuf>> {
        let mut engines = ResamplerConfig::all_swr();
        engines.extend(ResamplerConfig::all_soxr());
        
        let mut outputs = Vec::new();
        
        for &rate in target_rates {
            for engine in &engines {
                let config = ResampleConfig {
                    target_rate: rate,
                    engine: engine.clone(),
                };
                
                match self.generate_resampled(&input, &config) {
                    Ok(path) => {
                        println!("Generated: {}", path.display());
                        outputs.push(path);
                    }
                    Err(e) => {
                        eprintln!("Failed to generate {}Hz {}: {}", rate, engine.short_name(), e);
                    }
                }
            }
        }
        
        Ok(outputs)
    }
}

/// Generate test file manifest describing all test cases
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestManifest {
    pub source_file: String,
    pub source_sample_rate: u32,
    pub source_bit_depth: u8,
    pub dithering_tests: Vec<DitherTestCase>,
    pub resampling_tests: Vec<ResampleTestCase>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DitherTestCase {
    pub filename: String,
    pub method: String,
    pub scale: f32,
    pub output_bits: u8,
    pub expected_detection: DitherExpectedResult,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DitherExpectedResult {
    pub is_bit_reduced: bool,
    pub effective_bits: u8,
    pub algorithm_detectable: bool,
    pub expected_algorithm: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResampleTestCase {
    pub filename: String,
    pub original_rate: u32,
    pub target_rate: u32,
    pub engine: String,
    pub expected_detection: ResampleExpectedResult,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResampleExpectedResult {
    pub is_resampled: bool,
    pub direction: String,
    pub quality_tier: String,
    pub engine_detectable: bool,
}

impl TestManifest {
    /// Generate manifest for a complete test suite
    pub fn generate(
        source: &str,
        source_rate: u32,
        source_bits: u8,
        target_rates: &[u32],
    ) -> Self {
        let mut dithering_tests = Vec::new();
        let mut resampling_tests = Vec::new();
        
        // Generate dithering test cases
        let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
        for method in DitherMethod::all() {
            for &scale in &scales {
                let filename = format!(
                    "{}_dither_{}_scale{}.flac",
                    source,
                    method.ffmpeg_name(),
                    format!("{:.2}", scale).replace('.', "_")
                );
                
                dithering_tests.push(DitherTestCase {
                    filename,
                    method: method.ffmpeg_name().to_string(),
                    scale,
                    output_bits: 16,
                    expected_detection: DitherExpectedResult {
                        is_bit_reduced: true,
                        effective_bits: 16,
                        algorithm_detectable: matches!(
                            method,
                            DitherMethod::Rectangular | DitherMethod::Triangular |
                            DitherMethod::Shibata | DitherMethod::HighShibata
                        ),
                        expected_algorithm: Some(method.ffmpeg_name().to_string()),
                    },
                });
            }
        }
        
        // Generate resampling test cases
        let mut engines = ResamplerConfig::all_swr();
        engines.extend(ResamplerConfig::all_soxr());
        
        for &rate in target_rates {
            for engine in &engines {
                let filename = format!(
                    "{}_{}Hz_{}.flac",
                    source,
                    rate,
                    engine.short_name()
                );
                
                let direction = if rate > source_rate {
                    "Upsample"
                } else if rate < source_rate {
                    "Downsample"
                } else {
                    "None"
                };
                
                let quality = match engine {
                    ResamplerConfig::SwrDefault | ResamplerConfig::SwrCubic => "Standard",
                    ResamplerConfig::SwrBlackmanNuttall |
                    ResamplerConfig::SwrKaiser { beta: 9 } |
                    ResamplerConfig::SwrFilterSize { size: 16 } => "High",
                    ResamplerConfig::SwrKaiser { beta: 12 } |
                    ResamplerConfig::SwrKaiser { beta: 16 } |
                    ResamplerConfig::SwrFilterSize { size: 64 } => "VeryHigh",
                    ResamplerConfig::SoxrDefault | ResamplerConfig::SoxrHQ => "High",
                    ResamplerConfig::SoxrVHQ | ResamplerConfig::SoxrVHQCheby |
                    ResamplerConfig::SoxrCutoff { .. } => "VeryHigh",
                    _ => "Standard",
                };
                
                resampling_tests.push(ResampleTestCase {
                    filename,
                    original_rate: source_rate,
                    target_rate: rate,
                    engine: engine.short_name(),
                    expected_detection: ResampleExpectedResult {
                        is_resampled: rate != source_rate,
                        direction: direction.to_string(),
                        quality_tier: quality.to_string(),
                        engine_detectable: matches!(
                            engine,
                            ResamplerConfig::SoxrVHQ | ResamplerConfig::SoxrVHQCheby |
                            ResamplerConfig::SwrBlackmanNuttall
                        ),
                    },
                });
            }
        }
        
        TestManifest {
            source_file: source.to_string(),
            source_sample_rate: source_rate,
            source_bit_depth: source_bits,
            dithering_tests,
            resampling_tests,
        }
    }
    
    /// Save manifest to JSON file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
    
    /// Load manifest from JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let manifest: Self = serde_json::from_str(&json)?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dither_method_names() {
        assert_eq!(DitherMethod::Rectangular.ffmpeg_name(), "rectangular");
        assert_eq!(DitherMethod::Shibata.ffmpeg_name(), "shibata");
        assert_eq!(DitherMethod::ImprovedEWeighted.ffmpeg_name(), "improved_e_weighted");
    }
    
    #[test]
    fn test_resampler_config() {
        let cfg = ResamplerConfig::SwrKaiser { beta: 12 };
        assert!(cfg.ffmpeg_filter().contains("kaiser_beta=12"));
        assert_eq!(cfg.short_name(), "swr_kaiser_beta12");
        
        let cfg = ResamplerConfig::SoxrVHQ;
        assert!(cfg.ffmpeg_filter().contains("precision=28"));
    }
    
    #[test]
    fn test_manifest_generation() {
        let manifest = TestManifest::generate(
            "test",
            176400,
            24,
            &[44100, 48000, 96000],
        );
        
        assert!(!manifest.dithering_tests.is_empty());
        assert!(!manifest.resampling_tests.is_empty());
        
        // Should have 10 methods × 6 scales = 60 dithering tests
        assert_eq!(manifest.dithering_tests.len(), 60);
    }
}
