//! CLI module for AudioCheckr

mod args;
mod output;

pub use args::{parse_args, print_profiles, CliArgs};
pub use output::{format_json, format_result, format_summary};
