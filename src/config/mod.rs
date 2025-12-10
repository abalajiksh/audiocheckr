// src/config/mod.rs
//
// Configuration and detection profiles

mod profiles;

pub use profiles::{
    DetectorType, ProfilePreset, ProfileConfig, ProfileBuilder,
    ConfidenceModifier,
};
