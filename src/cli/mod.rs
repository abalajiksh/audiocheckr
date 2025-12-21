// src/cli/mod.rs
//
// Command-line interface module

mod args;
mod output;
pub mod extended_detection;

pub use args::{Args, parse_args};
pub use output::{print_report, print_json};

/// Run the CLI
pub fn run() -> anyhow::Result<()> {
    // CLI entry point - delegates to main.rs for now
    // In future, this will be the primary entry point
    Ok(())
}

pub use extended_detection::{
    ExtendedDetectionArgs,
    ExtendedOutputFormat,
    EnfFrequencyArg,
    print_text_report,
    print_json_report,
    print_detailed_report,
};
