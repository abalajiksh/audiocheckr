//! ENF and Clipping Detection Pipeline Module
//!
//! This module is now a thin compatibility wrapper around the main
//! `detection_pipeline` module, which owns the core detection context
//! and artifact discrimination logic.
//!
//! All public types and functions are re-exported from
//! `core::analysis::detection_pipeline` so existing imports continue
//! to work while the actual implementation lives in a single place.

pub use crate::core::analysis::detection_pipeline::*;
