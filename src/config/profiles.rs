// src/config/profiles.rs
//
// Genre-aware detection profiles for reducing false positives

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Types of detectors that can be configured
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetectorType {
    SpectralCutoff,
    BitDepth,
    Upsampling,
    PreEcho,
    StereoField,
    PhaseCoherence,
    CodecSignature,
    TruePeak,
}

impl DetectorType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::SpectralCutoff,
            Self::BitDepth,
            Self::Upsampling,
            Self::PreEcho,
            Self::StereoField,
            Self::PhaseCoherence,
            Self::CodecSignature,
            Self::TruePeak,
        ]
    }
}

/// Preset profiles for common use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfilePreset {
    /// Balanced defaults for general music
    Standard,
    /// For verified high-resolution sources (reduced cutoff sensitivity)
    HighRes,
    /// Electronic, EDM, synthwave (tolerates sharp cutoffs)
    Electronic,
    /// Ambient, drone, noise (full-spectrum tolerance)
    Noise,
    /// Orchestral, acoustic (strict dynamic range)
    Classical,
    /// Speech, voice content (limited detectors)
    Podcast,
    /// User-defined settings
    Custom,
}

/// Confidence modifier for a detector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceModifier {
    /// Multiplier for detector confidence (0.0-2.0, where 1.0 is neutral)
    pub multiplier: f32,
    /// If true, detector findings are shown but don't affect verdict
    pub suppress_from_verdict: bool,
    /// Custom threshold override (None uses detector default)
    pub threshold_override: Option<f32>,
}

impl Default for ConfidenceModifier {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            suppress_from_verdict: false,
            threshold_override: None,
        }
    }
}

impl ConfidenceModifier {
    pub fn with_multiplier(multiplier: f32) -> Self {
        Self {
            multiplier,
            ..Default::default()
        }
    }
    
    pub fn suppressed() -> Self {
        Self {
            suppress_from_verdict: true,
            ..Default::default()
        }
    }
    
    pub fn disabled() -> Self {
        Self {
            multiplier: 0.0,
            suppress_from_verdict: true,
            ..Default::default()
        }
    }
}

/// Complete profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Profile name
    pub name: String,
    /// Profile description
    pub description: String,
    /// Base preset this was derived from
    pub base_preset: ProfilePreset,
    /// Per-detector confidence modifiers
    pub detector_modifiers: HashMap<DetectorType, ConfidenceModifier>,
    /// Global sensitivity adjustment (0.0-2.0)
    pub global_sensitivity: f32,
    /// Minimum confidence to report a finding
    pub min_confidence: f32,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self::from_preset(ProfilePreset::Standard)
    }
}

impl ProfileConfig {
    /// Create profile from preset
    pub fn from_preset(preset: ProfilePreset) -> Self {
        match preset {
            ProfilePreset::Standard => Self::standard(),
            ProfilePreset::HighRes => Self::high_res(),
            ProfilePreset::Electronic => Self::electronic(),
            ProfilePreset::Noise => Self::noise(),
            ProfilePreset::Classical => Self::classical(),
            ProfilePreset::Podcast => Self::podcast(),
            ProfilePreset::Custom => Self::standard(),
        }
    }
    
    fn standard() -> Self {
        Self {
            name: "Standard".to_string(),
            description: "Balanced defaults for general music".to_string(),
            base_preset: ProfilePreset::Standard,
            detector_modifiers: HashMap::new(),
            global_sensitivity: 1.0,
            min_confidence: 0.5,
        }
    }
    
    fn high_res() -> Self {
        let mut modifiers = HashMap::new();
        
        // Reduce spectral cutoff sensitivity (high-res may have natural rolloff)
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::with_multiplier(0.7),
        );
        
        // Strict bit depth checking
        modifiers.insert(
            DetectorType::BitDepth,
            ConfidenceModifier::with_multiplier(1.2),
        );
        
        Self {
            name: "HighRes".to_string(),
            description: "For verified high-resolution sources".to_string(),
            base_preset: ProfilePreset::HighRes,
            detector_modifiers: modifiers,
            global_sensitivity: 0.9,
            min_confidence: 0.6,
        }
    }
    
    fn electronic() -> Self {
        let mut modifiers = HashMap::new();
        
        // Electronic music often has intentional sharp cutoffs
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::with_multiplier(0.6),
        );
        
        // Synthesizers don't have pre-echo
        modifiers.insert(
            DetectorType::PreEcho,
            ConfidenceModifier::disabled(),
        );
        
        // Phase can be unusual in electronic music
        modifiers.insert(
            DetectorType::PhaseCoherence,
            ConfidenceModifier::with_multiplier(0.7),
        );
        
        Self {
            name: "Electronic".to_string(),
            description: "EDM, synthwave, and electronic music".to_string(),
            base_preset: ProfilePreset::Electronic,
            detector_modifiers: modifiers,
            global_sensitivity: 0.8,
            min_confidence: 0.6,
        }
    }
    
    fn noise() -> Self {
        let mut modifiers = HashMap::new();
        
        // Noise has full-spectrum energy by design
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::with_multiplier(0.3),
        );
        
        // Pre-echo is meaningless in noise
        modifiers.insert(
            DetectorType::PreEcho,
            ConfidenceModifier::with_multiplier(0.4),
        );
        
        // Upsampling detection unreliable for noise
        modifiers.insert(
            DetectorType::Upsampling,
            ConfidenceModifier::disabled(),
        );
        
        // Stereo field is often extreme in ambient
        modifiers.insert(
            DetectorType::StereoField,
            ConfidenceModifier::suppressed(),
        );
        
        Self {
            name: "Noise/Ambient".to_string(),
            description: "Ambient, drone, and noise music".to_string(),
            base_preset: ProfilePreset::Noise,
            detector_modifiers: modifiers,
            global_sensitivity: 0.5,
            min_confidence: 0.7,
        }
    }
    
    fn classical() -> Self {
        let mut modifiers = HashMap::new();
        
        // Classical recordings should have full dynamics
        modifiers.insert(
            DetectorType::BitDepth,
            ConfidenceModifier::with_multiplier(1.3),
        );
        
        // Pre-echo is very audible in classical
        modifiers.insert(
            DetectorType::PreEcho,
            ConfidenceModifier::with_multiplier(1.2),
        );
        
        // Phase should be clean in well-recorded classical
        modifiers.insert(
            DetectorType::PhaseCoherence,
            ConfidenceModifier::with_multiplier(1.1),
        );
        
        Self {
            name: "Classical".to_string(),
            description: "Orchestral and acoustic recordings".to_string(),
            base_preset: ProfilePreset::Classical,
            detector_modifiers: modifiers,
            global_sensitivity: 1.1,
            min_confidence: 0.5,
        }
    }
    
    fn podcast() -> Self {
        let mut modifiers = HashMap::new();
        
        // Most detectors are irrelevant for speech
        modifiers.insert(DetectorType::SpectralCutoff, ConfidenceModifier::disabled());
        modifiers.insert(DetectorType::Upsampling, ConfidenceModifier::disabled());
        modifiers.insert(DetectorType::PreEcho, ConfidenceModifier::disabled());
        modifiers.insert(DetectorType::StereoField, ConfidenceModifier::disabled());
        modifiers.insert(DetectorType::PhaseCoherence, ConfidenceModifier::disabled());
        
        // Keep bit depth and codec signature
        modifiers.insert(
            DetectorType::BitDepth,
            ConfidenceModifier::with_multiplier(1.0),
        );
        modifiers.insert(
            DetectorType::CodecSignature,
            ConfidenceModifier::with_multiplier(1.0),
        );
        
        Self {
            name: "Podcast".to_string(),
            description: "Speech and voice content".to_string(),
            base_preset: ProfilePreset::Podcast,
            detector_modifiers: modifiers,
            global_sensitivity: 0.5,
            min_confidence: 0.7,
        }
    }
    
    /// Get modifier for a specific detector
    pub fn get_modifier(&self, detector: DetectorType) -> ConfidenceModifier {
        self.detector_modifiers
            .get(&detector)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Check if a detector is enabled
    pub fn is_detector_enabled(&self, detector: DetectorType) -> bool {
        let modifier = self.get_modifier(detector);
        modifier.multiplier > 0.0
    }
    
    /// Apply profile adjustment to raw confidence
    pub fn adjust_confidence(&self, detector: DetectorType, raw_confidence: f32) -> f32 {
        let modifier = self.get_modifier(detector);
        (raw_confidence * modifier.multiplier * self.global_sensitivity).clamp(0.0, 1.0)
    }
}

/// Builder for custom profiles
pub struct ProfileBuilder {
    config: ProfileConfig,
}

impl ProfileBuilder {
    pub fn new() -> Self {
        Self {
            config: ProfileConfig::default(),
        }
    }
    
    pub fn from_preset(preset: ProfilePreset) -> Self {
        Self {
            config: ProfileConfig::from_preset(preset),
        }
    }
    
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }
    
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.config.description = desc.into();
        self
    }
    
    pub fn global_sensitivity(mut self, sensitivity: f32) -> Self {
        self.config.global_sensitivity = sensitivity.clamp(0.0, 2.0);
        self
    }
    
    pub fn min_confidence(mut self, threshold: f32) -> Self {
        self.config.min_confidence = threshold.clamp(0.0, 1.0);
        self
    }
    
    pub fn detector_multiplier(mut self, detector: DetectorType, multiplier: f32) -> Self {
        self.config.detector_modifiers.insert(
            detector,
            ConfidenceModifier::with_multiplier(multiplier),
        );
        self
    }
    
    pub fn disable_detector(mut self, detector: DetectorType) -> Self {
        self.config.detector_modifiers.insert(
            detector,
            ConfidenceModifier::disabled(),
        );
        self
    }
    
    pub fn suppress_detector(mut self, detector: DetectorType) -> Self {
        self.config.detector_modifiers.insert(
            detector,
            ConfidenceModifier::suppressed(),
        );
        self
    }
    
    pub fn build(mut self) -> ProfileConfig {
        self.config.base_preset = ProfilePreset::Custom;
        self.config
    }
}

impl Default for ProfileBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profile_adjustment() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Electronic);
        
        // Spectral cutoff should have reduced confidence
        let adjusted = profile.adjust_confidence(DetectorType::SpectralCutoff, 0.8);
        assert!(adjusted < 0.8);
        
        // Pre-echo should be disabled
        assert!(!profile.is_detector_enabled(DetectorType::PreEcho));
    }
    
    #[test]
    fn test_profile_builder() {
        let profile = ProfileBuilder::new()
            .name("Test Profile")
            .global_sensitivity(0.8)
            .disable_detector(DetectorType::Upsampling)
            .detector_multiplier(DetectorType::BitDepth, 1.5)
            .build();
        
        assert_eq!(profile.name, "Test Profile");
        assert!(!profile.is_detector_enabled(DetectorType::Upsampling));
        assert!(profile.is_detector_enabled(DetectorType::BitDepth));
    }
}
