//! Compile command implementation.

use anyhow::{Context, Result};
use nectar_compiler::{Compiler, CompileOptions, Lockfile, OutputFormat};
use std::fs;
use std::path::Path;
use tracing::info;

/// Runs the compile command.
pub fn run(
    policy_path: &str,
    output_path: &str,
    format: &str,
    create_lockfile: bool,
) -> Result<()> {
    info!("Compiling policy: {}", policy_path);

    // Read policy file
    let policy_content = fs::read_to_string(policy_path)
        .with_context(|| format!("Failed to read policy file: {policy_path}"))?;

    // Parse policy
    let policy = toon_policy::parse(&policy_content)
        .with_context(|| "Failed to parse policy")?;

    info!("Parsed policy '{}' with {} rules", policy.name, policy.rules.len());

    // Determine output format
    let output_format = match format.to_lowercase().as_str() {
        "json" => OutputFormat::Json,
        "yaml" | "yml" => OutputFormat::Yaml,
        _ => {
            anyhow::bail!("Unknown output format: {format}. Use 'yaml' or 'json'.");
        }
    };

    // Compile
    let compiler = Compiler::with_options(CompileOptions {
        format: output_format,
        include_comments: true,
    });
    let output = compiler.compile(&policy)
        .with_context(|| "Failed to compile policy")?;

    // Write output
    fs::write(output_path, &output)
        .with_context(|| format!("Failed to write output file: {output_path}"))?;

    info!("Compiled policy written to: {}", output_path);

    // Create lockfile if requested
    if create_lockfile {
        let lockfile = Lockfile::new(&policy, &policy_content, &output).with_timestamp();
        let lock_path = format!("{}.lock", policy_path.trim_end_matches(".toon"));
        let lock_path = Path::new(&lock_path);

        lockfile.save(lock_path)
            .with_context(|| format!("Failed to write lockfile: {}", lock_path.display()))?;

        info!("Lockfile written to: {}", lock_path.display());
    }

    Ok(())
}

/// Verifies a policy against its lockfile.
#[allow(dead_code)] // Will be used when verify command is added
pub fn verify_lockfile(policy_path: &str, lock_path: &str) -> Result<bool> {
    info!("Verifying policy against lockfile");

    // Read policy file
    let policy_content = fs::read_to_string(policy_path)
        .with_context(|| format!("Failed to read policy file: {policy_path}"))?;

    // Parse policy
    let policy = toon_policy::parse(&policy_content)
        .with_context(|| "Failed to parse policy")?;

    // Load lockfile
    let lockfile = Lockfile::load(lock_path)
        .with_context(|| format!("Failed to read lockfile: {lock_path}"))?;

    // Compile to get current output
    let compiler = Compiler::new();
    let output = compiler.compile(&policy)
        .with_context(|| "Failed to compile policy")?;

    // Verify
    let source_matches = lockfile.verify_source(&policy_content);
    let compiled_matches = lockfile.verify_compiled(&output);

    if !source_matches {
        info!("Source hash mismatch - policy file has been modified");
    }

    if !compiled_matches {
        info!("Compiled hash mismatch - output differs from locked version");
    }

    Ok(source_matches && compiled_matches)
}
