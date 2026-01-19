//! Prove command implementation.

use anyhow::{Context, Result};
use nectar_corpus::Corpus;
use nectar_prover::{Prover, ProverConfig};
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};

/// Runs the prove command.
pub fn run(policy_path: &str, corpus_path: &str, strict: bool) -> Result<()> {
    info!("Verifying policy: {}", policy_path);
    info!("Against corpus: {}", corpus_path);

    // Read policy file
    let policy_content = fs::read_to_string(policy_path)
        .with_context(|| format!("Failed to read policy file: {policy_path}"))?;

    // Parse policy
    let policy = toon_policy::parse(&policy_content).with_context(|| "Failed to parse policy")?;

    // Load corpus
    let corpus = load_corpus(corpus_path)?;
    info!("Loaded {} traces from corpus", corpus.len());

    // Create prover
    let prover = Prover::new(ProverConfig {
        require_error_handling: true,
        ..Default::default()
    });

    // Run verification
    let result = prover
        .verify(&policy, &corpus)
        .with_context(|| "Prover failed")?;

    // Report results
    info!(
        "Checks passed: {}/{}",
        result.checks_passed, result.checks_total
    );

    for violation in &result.violations {
        error!(
            "[{}] {}: {}",
            violation.severity, violation.check, violation.message
        );
    }

    for warning in &result.warnings {
        warn!(
            "[{}] {}: {}",
            warning.severity, warning.check, warning.message
        );
    }

    // Determine exit status
    if result.is_rejected() {
        anyhow::bail!(
            "Policy verification failed with {} violation(s)",
            result.violations.len()
        );
    }

    if strict && !result.warnings.is_empty() {
        anyhow::bail!(
            "Policy verification failed with {} warning(s) (strict mode)",
            result.warnings.len()
        );
    }

    info!("Policy verification passed!");
    Ok(())
}

fn load_corpus(path: &str) -> Result<Corpus> {
    let path = Path::new(path);

    if !path.exists() {
        warn!(
            "Corpus path not found: {}. Using empty corpus.",
            path.display()
        );
        return Ok(Corpus::new());
    }

    if path.is_dir() {
        // Load all JSON files from the directory
        Corpus::load_directory(path)
            .with_context(|| format!("Failed to load corpus from directory: {}", path.display()))
    } else if path.is_file() {
        // Load single JSON file
        Corpus::load_file(path)
            .with_context(|| format!("Failed to load corpus file: {}", path.display()))
    } else {
        warn!(
            "Corpus path is not a file or directory: {}. Using empty corpus.",
            path.display()
        );
        Ok(Corpus::new())
    }
}
