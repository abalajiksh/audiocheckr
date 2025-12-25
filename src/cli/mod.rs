// src/cli/mod.rs
//
// Command-line interface module for AudioCheckr

pub mod output;

pub use output::{format_report, format_defect, format_defect_type, defect_severity_color};
