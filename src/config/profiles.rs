//! Genre-aware detection profiles for AudioCheckr
//!
//! Provides configurable detection profiles that adjust sensitivity and
//! confidence thresholds based on audio genre/type characteristics.

use std::collections::HashMap;

/// Available detector types that can be configured per-profile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectorType {
    SpectralCutoff,
    PreEcho,
    BitDepth,
    Upsampling,
    CodecSignature,
    PhaseAnalysis,
    DynamicRange,
}

impl DetectorType {
    pub fn all() -> &'static [DetectorType] {
        &[
            DetectorType::SpectralCutoff,
            DetectorType::PreEcho,
            DetectorType::BitDepth,
            DetectorType::Upsampling,
            DetectorType::CodecSignature,
            DetectorType::PhaseAnalysis,
            DetectorType::DynamicRange,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            DetectorType::SpectralCutoff => "spectral_cutoff",
            DetectorType::PreEcho => "pre_echo",
            DetectorType::BitDepth => "bit_depth",
            DetectorType::Upsampling => "upsampling",
            DetectorType::CodecSignature => "codec_signature",
            DetectorType::PhaseAnalysis => "phase_analysis",
            DetectorType::DynamicRange => "dynamic_range",
        }
    }

    pub fn from_name(name: &str) -> Option<DetectorType> {
        match name {
            "spectral_cutoff" => Some(DetectorType::SpectralCutoff),
            "pre_echo" => Some(DetectorType::PreEcho),
            "bit_depth" => Some(DetectorType::BitDepth),
            "upsampling" => Some(DetectorType::Upsampling),
            "codec_signature" => Some(DetectorType::CodecSignature),
            "phase_analysis" => Some(DetectorType::PhaseAnalysis),
            "dynamic_range" => Some(DetectorType::DynamicRange),
            _ => None,
        }
    }
}

/// Named detection profiles for common use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfilePreset {
    /// Balanced settings for most music (default)
    #[default]
    Standard,
    /// For verified high-resolution audio sources
    HighResAudio,
    /// Electronic music with intentional sharp cutoffs
    Electronic,
    /// Noise, ambient, drone - expects full-spectrum energy
    Noise,
    /// Classical/orchestral with wide dynamic range
    Classical,
    /// Speech content - different spectral characteristics
    Podcast,
    /// Fully custom configuration
    Custom,
}

impl ProfilePreset {
    pub fn from_name(name: &str) -> Option<ProfilePreset> {
        match name.to_lowercase().as_str() {
            "standard" => Some(ProfilePreset::Standard),
            "highres" | "highresaudio" | "hi-res" => Some(ProfilePreset::HighResAudio),
            "electronic" | "edm" => Some(ProfilePreset::Electronic),
            "noise" | "ambient" => Some(ProfilePreset::Noise),
            "classical" | "orchestral" => Some(ProfilePreset::Classical),
            "podcast" | "speech" | "voice" => Some(ProfilePreset::Podcast),
            "custom" => Some(ProfilePreset::Custom),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ProfilePreset::Standard => "standard",
            ProfilePreset::HighResAudio => "highres",
            ProfilePreset::Electronic => "electronic",
            ProfilePreset::Noise => "noise",
            ProfilePreset::Classical => "classical",
            ProfilePreset::Podcast => "podcast",
            ProfilePreset::Custom => "custom",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ProfilePreset::Standard => "Balanced settings for most music",
            ProfilePreset::HighResAudio => "For verified high-resolution audio sources",
            ProfilePreset::Electronic => "Electronic music with intentional sharp cutoffs",
            ProfilePreset::Noise => "Noise/ambient/drone with full-spectrum energy",
            ProfilePreset::Classical => "Classical/orchestral with wide dynamic range",
            ProfilePreset::Podcast => "Speech/podcast content",
            ProfilePreset::Custom => "Fully custom configuration",
        }
    }
}

/// Confidence modifier for a specific detector
#[derive(Debug, Clone, Copy)]
pub struct ConfidenceModifier {
    /// Multiplier applied to raw confidence (0.0 - 2.0)
    pub multiplier: f32,
    /// Minimum confidence threshold (findings below this are suppressed)
    pub min_threshold: f32,
}

impl Default for ConfidenceModifier {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            min_threshold: 0.0,
        }
    }
}

impl ConfidenceModifier {
    pub fn new(multiplier: f32, min_threshold: f32) -> Self {
        Self {
            multiplier: multiplier.clamp(0.0, 2.0),
            min_threshold: min_threshold.clamp(0.0, 1.0),
        }
    }

    /// Apply modifier to a raw confidence score
    pub fn apply(&self, raw_confidence: f32) -> Option<f32> {
        let modified = raw_confidence * self.multiplier;
        if modified >= self.min_threshold {
            Some(modified.clamp(0.0, 1.0))
        } else {
            None // Suppressed
        }
    }
}

/// Complete profile configuration
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Base preset this config derives from
    pub preset: ProfilePreset,
    /// Display name for reporting
    pub name: String,
    /// Detectors that are enabled
    pub enabled_detectors: Vec<DetectorType>,
    /// Confidence modifiers per detector
    pub confidence_modifiers: HashMap<DetectorType, ConfidenceModifier>,
    /// Global sensitivity multiplier (applied to all detectors)
    pub global_sensitivity: f32,
    /// Spectral cutoff tolerance in Hz (for high-res legitimacy)
    pub spectral_cutoff_tolerance_hz: u32,
    /// Minimum frequency for spectral analysis (Hz)
    pub spectral_min_freq_hz: u32,
    /// Pre-echo sensitivity (0.0 = ignore, 1.0 = very sensitive)
    pub pre_echo_sensitivity: f32,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self::from_preset(ProfilePreset::Standard)
    }
}

impl ProfileConfig {
    /// Create a profile from a preset
    pub fn from_preset(preset: ProfilePreset) -> Self {
        match preset {
            ProfilePreset::Standard => Self::standard(),
            ProfilePreset::HighResAudio => Self::highres(),
            ProfilePreset::Electronic => Self::electronic(),
            ProfilePreset::Noise => Self::noise(),
            ProfilePreset::Classical => Self::classical(),
            ProfilePreset::Podcast => Self::podcast(),
            ProfilePreset::Custom => Self::standard(), // Start from standard
        }
    }

    fn standard() -> Self {
        Self {
            preset: ProfilePreset::Standard,
            name: "Standard".into(),
            enabled_detectors: DetectorType::all().to_vec(),
            confidence_modifiers: HashMap::new(),
            global_sensitivity: 1.0,
            spectral_cutoff_tolerance_hz: 500,
            spectral_min_freq_hz: 16000,
            pre_echo_sensitivity: 0.7,
        }
    }

    fn highres() -> Self {
        let mut modifiers = HashMap::new();
        // High-res audio is less likely to be transcoded
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::new(0.7, 0.4),
        );
        modifiers.insert(
            DetectorType::Upsampling,
            ConfidenceModifier::new(0.8, 0.3),
        );

        Self {
            preset: ProfilePreset::HighResAudio,
            name: "High-Resolution Audio".into(),
            enabled_detectors: DetectorType::all().to_vec(),
            confidence_modifiers: modifiers,
            global_sensitivity: 0.8,
            spectral_cutoff_tolerance_hz: 1000,
            spectral_min_freq_hz: 20000,
            pre_echo_sensitivity: 0.5,
        }
    }

    fn electronic() -> Self {
        let mut modifiers = HashMap::new();
        // Electronic music often has intentional sharp cutoffs
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::new(0.5, 0.6),
        );
        // Phase anomalies common in synthesized audio
        modifiers.insert(
            DetectorType::PhaseAnalysis,
            ConfidenceModifier::new(0.6, 0.5),
        );

        Self {
            preset: ProfilePreset::Electronic,
            name: "Electronic/EDM".into(),
            enabled_detectors: DetectorType::all().to_vec(),
            confidence_modifiers: modifiers,
            global_sensitivity: 0.9,
            spectral_cutoff_tolerance_hz: 2000,
            spectral_min_freq_hz: 18000,
            pre_echo_sensitivity: 0.6,
        }
    }

    fn noise() -> Self {
        let mut modifiers = HashMap::new();
        // Noise/ambient should have full spectrum - reduce false positives
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::new(0.3, 0.7),
        );
        // Dynamic range compression is common/intentional
        modifiers.insert(
            DetectorType::DynamicRange,
            ConfidenceModifier::new(0.4, 0.6),
        );
        // Bit depth analysis less reliable for noise
        modifiers.insert(
            DetectorType::BitDepth,
            ConfidenceModifier::new(0.5, 0.5),
        );

        Self {
            preset: ProfilePreset::Noise,
            name: "Noise/Ambient".into(),
            enabled_detectors: DetectorType::all().to_vec(),
            confidence_modifiers: modifiers,
            global_sensitivity: 0.6,
            spectral_cutoff_tolerance_hz: 3000,
            spectral_min_freq_hz: 15000,
            pre_echo_sensitivity: 0.3,
        }
    }

    fn classical() -> Self {
        let mut modifiers = HashMap::new();
        // Classical recordings should have excellent dynamic range
        modifiers.insert(
            DetectorType::DynamicRange,
            ConfidenceModifier::new(1.2, 0.2),
        );

        Self {
            preset: ProfilePreset::Classical,
            name: "Classical/Orchestral".into(),
            enabled_detectors: DetectorType::all().to_vec(),
            confidence_modifiers: modifiers,
            global_sensitivity: 1.0,
            spectral_cutoff_tolerance_hz: 500,
            spectral_min_freq_hz: 18000,
            pre_echo_sensitivity: 0.8,
        }
    }

    fn podcast() -> Self {
        let mut modifiers = HashMap::new();
        // Speech has limited high-frequency content
        modifiers.insert(
            DetectorType::SpectralCutoff,
            ConfidenceModifier::new(0.4, 0.7),
        );
        // Dynamic range compression is standard for podcasts
        modifiers.insert(
            DetectorType::DynamicRange,
            ConfidenceModifier::new(0.3, 0.8),
        );

        // Disable some detectors irrelevant to speech
        let enabled = vec![
            DetectorType::BitDepth,
            DetectorType::CodecSignature,
            DetectorType::PreEcho,
        ];

        Self {
            preset: ProfilePreset::Podcast,
            name: "Podcast/Speech".into(),
            enabled_detectors: enabled,
            confidence_modifiers: modifiers,
            global_sensitivity: 0.7,
            spectral_cutoff_tolerance_hz: 4000,
            spectral_min_freq_hz: 12000,
            pre_echo_sensitivity: 0.5,
        }
    }

    /// Check if a detector is enabled
    pub fn is_detector_enabled(&self, detector: DetectorType) -> bool {
        self.enabled_detectors.contains(&detector)
    }

    /// Get the confidence modifier for a detector
    pub fn get_modifier(&self, detector: DetectorType) -> ConfidenceModifier {
        self.confidence_modifiers
            .get(&detector)
            .copied()
            .unwrap_or_default()
    }

    /// Apply profile adjustments to a raw detection result
    pub fn adjust_confidence(&self, detector: DetectorType, raw_confidence: f32) -> Option<f32> {
        if !self.is_detector_enabled(detector) {
            return None;
        }

        let modifier = self.get_modifier(detector);
        let modified = modifier.apply(raw_confidence)?;

        // Apply global sensitivity
        Some((modified * self.global_sensitivity).clamp(0.0, 1.0))
    }

    /// Enable a specific detector
    pub fn enable_detector(&mut self, detector: DetectorType) {
        if !self.enabled_detectors.contains(&detector) {
            self.enabled_detectors.push(detector);
        }
    }

    /// Disable a specific detector
    pub fn disable_detector(&mut self, detector: DetectorType) {
        self.enabled_detectors.retain(|&d| d != detector);
    }

    /// Set confidence modifier for a detector
    pub fn set_modifier(&mut self, detector: DetectorType, modifier: ConfidenceModifier) {
        self.confidence_modifiers.insert(detector, modifier);
    }

    /// Set global sensitivity
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.global_sensitivity = sensitivity.clamp(0.1, 2.0);
    }
}

/// Builder for creating custom profiles
#[derive(Debug, Clone)]
pub struct ProfileBuilder {
    config: ProfileConfig,
}

impl ProfileBuilder {
    /// Start from a preset
    pub fn from_preset(preset: ProfilePreset) -> Self {
        Self {
            config: ProfileConfig::from_preset(preset),
        }
    }

    /// Start with standard settings
    pub fn new() -> Self {
        Self::from_preset(ProfilePreset::Standard)
    }

    /// Set the profile name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    /// Set global sensitivity
    pub fn sensitivity(mut self, sensitivity: f32) -> Self {
        self.config.set_sensitivity(sensitivity);
        self
    }

    /// Disable a detector
    pub fn disable(mut self, detector: DetectorType) -> Self {
        self.config.disable_detector(detector);
        self
    }

    /// Enable a detector
    pub fn enable(mut self, detector: DetectorType) -> Self {
        self.config.enable_detector(detector);
        self
    }

    /// Set confidence modifier
    pub fn modifier(mut self, detector: DetectorType, multiplier: f32, min_threshold: f32) -> Self {
        self.config.set_modifier(detector, ConfidenceModifier::new(multiplier, min_threshold));
        self
    }

    /// Set spectral cutoff tolerance
    pub fn spectral_tolerance(mut self, hz: u32) -> Self {
        self.config.spectral_cutoff_tolerance_hz = hz;
        self
    }

    /// Set pre-echo sensitivity
    pub fn pre_echo_sensitivity(mut self, sensitivity: f32) -> Self {
        self.config.pre_echo_sensitivity = sensitivity.clamp(0.0, 1.0);
        self
    }

    /// Build the final config
    pub fn build(mut self) -> ProfileConfig {
        self.config.preset = ProfilePreset::Custom;
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
    fn test_confidence_modifier_apply() {
        let modifier = ConfidenceModifier::new(0.5, 0.3);

        // 0.8 * 0.5 = 0.4, above threshold
        assert_eq!(modifier.apply(0.8), Some(0.4));

        // 0.5 * 0.5 = 0.25, below threshold
        assert_eq!(modifier.apply(0.5), None);

        // Test clamping
        assert_eq!(modifier.apply(2.5), Some(1.0));
    }

    #[test]
    fn test_profile_adjust_confidence() {
        let profile = ProfileConfig::from_preset(ProfilePreset::Noise);

        // Spectral cutoff has 0.3x multiplier, 0.7 threshold
        // 0.9 * 0.3 = 0.27, below 0.7 threshold
        assert_eq!(
            profile.adjust_confidence(DetectorType::SpectralCutoff, 0.9),
            None
        );

        // Standard detector without modifier
        let standard = ProfileConfig::from_preset(ProfilePreset::Standard);
        assert!(standard
            .adjust_confidence(DetectorType::SpectralCutoff, 0.8)
            .is_some());
    }

    #[test]
    fn test_profile_builder() {
        let profile = ProfileBuilder::new()
            .name("Test Profile")
            .sensitivity(0.5)
            .disable(DetectorType::PreEcho)
            .modifier(DetectorType::SpectralCutoff, 0.7, 0.4)
            .build();

        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.global_sensitivity, 0.5);
        assert!(!profile.is_detector_enabled(DetectorType::PreEcho));
        assert!(profile.is_detector_enabled(DetectorType::SpectralCutoff));
    }

    #[test]
    fn test_preset_from_name() {
        assert_eq!(
            ProfilePreset::from_name("electronic"),
            Some(ProfilePreset::Electronic)
        );
        assert_eq!(
            ProfilePreset::from_name("EDM"),
            Some(ProfilePreset::Electronic)
        );
        assert_eq!(
            ProfilePreset::from_name("noise"),
            Some(ProfilePreset::Noise)
        );
        assert_eq!(ProfilePreset::from_name("invalid"), None);
    }
}
