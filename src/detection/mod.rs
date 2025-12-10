// src/detection/mod.rs
//
// Detection result types

mod result;

pub use result::{
    RawDetection, Finding, AnalysisResult, AnalysisVerdict, Severity,
};
