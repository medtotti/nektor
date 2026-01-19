//! Propose command implementation.

use anyhow::{Context, Result};
use nectar_claude::{Client, ClientConfig};
use nectar_corpus::Corpus;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Runs the propose command.
pub async fn run(
    intent: &str,
    corpus_path: Option<&str>,
    policy_path: Option<&str>,
    output_path: &str,
) -> Result<()> {
    info!("Generating policy for intent: {}", intent);

    // Load API key from environment
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .with_context(|| "ANTHROPIC_API_KEY environment variable not set")?;

    // Create Claude client
    let client = Client::new(ClientConfig {
        api_key,
        ..Default::default()
    })
    .with_context(|| "Failed to create Claude client")?;

    // Load corpus if provided
    let corpus = if let Some(path) = corpus_path {
        load_corpus(path)?
    } else {
        Corpus::new()
    };

    if !corpus.is_empty() {
        info!("Loaded {} traces from corpus", corpus.len());
    }

    // Load existing policy if provided
    let current_policy = if let Some(path) = policy_path {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read policy file: {path}"))?;
        Some(toon_policy::parse(&content).with_context(|| "Failed to parse existing policy")?)
    } else {
        None
    };

    // Generate policy
    let policy = client
        .generate_policy(intent, &corpus, current_policy.as_ref())
        .await
        .with_context(|| "Failed to generate policy")?;

    info!(
        "Generated policy '{}' with {} rules",
        policy.name,
        policy.rules.len()
    );

    // Serialize policy to TOON format
    let output = toon_policy::serialize(&policy);

    // Write output
    fs::write(output_path, &output)
        .with_context(|| format!("Failed to write output file: {output_path}"))?;

    info!("Generated policy written to: {}", output_path);
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
        Corpus::load_directory(path)
            .with_context(|| format!("Failed to load corpus from directory: {}", path.display()))
    } else if path.is_file() {
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
