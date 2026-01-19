//! Explain command implementation.

use anyhow::{Context, Result};
use nectar_compiler::waggle::generate_waggle_report;
use std::fs;
use tracing::info;

/// Runs the explain command.
pub fn run(policy_path: &str, output_path: &str) -> Result<()> {
    info!("Generating waggle report for: {}", policy_path);

    // Read policy file
    let policy_content = fs::read_to_string(policy_path)
        .with_context(|| format!("Failed to read policy file: {policy_path}"))?;

    // Parse policy
    let policy = toon_policy::parse(&policy_content).with_context(|| "Failed to parse policy")?;

    // Generate waggle report
    let report = generate_waggle_report(&policy);

    // Write output
    fs::write(output_path, &report)
        .with_context(|| format!("Failed to write output file: {output_path}"))?;

    info!("Waggle report written to: {}", output_path);
    Ok(())
}
