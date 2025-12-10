// src/core/analyzer.rs
//
// High-level audio analysis API with builder pattern.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::decoder::{decode_audio, AudioData};
use super::detector::{detect_quality_issues, DetectionConfig, QualityReport};

/// File information extracted before full decoding
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub extension: String,
}

/// Builder for AudioAnalyzer configuration
pub struct AnalyzerBuilder {
    config: DetectionConfig,
}

impl AnalyzerBuilder {
    pub fn new() -> Self {
        Self {
            config: DetectionConfig::default(),
        }
    }
    
    pub fn expected_bit_depth(mut self, depth: u32) -> Self {
        self.config.expected_bit_depth = depth;
        self
    }
    
    pub fn check_upsampling(mut self, check: bool) -> Self {
        self.config.check_upsampling = check;
        self
    }
    
    pub fn check_stereo(mut self, check: bool) -> Self {
        self.config.check_stereo = check;
        self
    }
    
    pub fn check_transients(mut self, check: bool) -> Self {
        self.config.check_transients = check;
        self
    }
    
    pub fn check_phase(mut self, check: bool) -> Self {
        self.config.check_phase = check;
        self
    }
    
    pub fn check_mfcc(mut self, check: bool) -> Self {
        self.config.check_mfcc = check;
        self
    }
    
    pub fn min_confidence(mut self, threshold: f32) -> Self {
        self.config.min_confidence = threshold;
        self
    }
    
    pub fn build<P: AsRef<Path>>(self, path: P) -> Result<AudioAnalyzer> {
        let audio = decode_audio(path.as_ref())?;
        Ok(AudioAnalyzer {
            path: path.as_ref().to_path_buf(),
            audio,
            config: self.config,
        })
    }
}

impl Default for AnalyzerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Main audio analyzer with fluent API
pub struct AudioAnalyzer {
    path: PathBuf,
    audio: AudioData,
    config: DetectionConfig,
}

impl AudioAnalyzer {
    /// Create analyzer with default configuration
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        AnalyzerBuilder::new().build(path)
    }
    
    /// Create analyzer with custom configuration
    pub fn with_config<P: AsRef<Path>>(path: P, config: DetectionConfig) -> Result<Self> {
        let audio = decode_audio(path.as_ref())?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            audio,
            config,
        })
    }
    
    /// Create a builder for custom configuration
    pub fn builder() -> AnalyzerBuilder {
        AnalyzerBuilder::new()
    }
    
    /// Run full analysis
    pub fn analyze(&self) -> Result<QualityReport> {
        Ok(detect_quality_issues(&self.audio, &self.config))
    }
    
    /// Get raw audio data
    pub fn audio_data(&self) -> &AudioData {
        &self.audio
    }
    
    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Generate spectrogram image
    pub fn generate_spectrogram(
        &self,
        output_path: &Path,
        linear_scale: bool,
        full_length: bool,
    ) -> Result<()> {
        use super::visualization::spectrogram::{
            generate_mel_spectrogram, generate_linear_spectrogram, SpectrogramConfig,
        };
        
        let mono = super::decoder::extract_mono(&self.audio);
        
        let config = SpectrogramConfig {
            width: if full_length { 2000 } else { 1200 },
            height: 400,
            fft_size: 4096,
            hop_size: 1024,
            min_db: -90.0,
            max_db: 0.0,
            max_seconds: if full_length { None } else { Some(15.0) },
        };
        
        if linear_scale {
            generate_linear_spectrogram(&mono, self.audio.sample_rate, &config, output_path)
        } else {
            generate_mel_spectrogram(&mono, self.audio.sample_rate, &config, output_path)
        }
    }
}
