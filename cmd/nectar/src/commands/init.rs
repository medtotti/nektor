//! Init command implementation.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::info;

/// Runs the init command.
pub fn run(path: &str) -> Result<()> {
    let project_path = Path::new(path);
    
    info!("Initializing Nectar project at: {}", project_path.display());

    // Create directories
    fs::create_dir_all(project_path.join("corpus"))
        .with_context(|| "Failed to create corpus directory")?;
    fs::create_dir_all(project_path.join("incidents"))
        .with_context(|| "Failed to create incidents directory")?;

    // Create default policy.toon
    let default_policy = r"nectar_policy{version,name,budget_per_second,rules}:
  1
  default-policy
  10000
  rules[2]{name,description,match,action,priority}:
    keep-errors,Retain all HTTP 5xx and application errors,status >= 500 || error == true,keep,100
    sample-baseline,Sample remaining traffic at 1%,true,sample(0.01),0
";

    let policy_path = project_path.join("policy.toon");
    if policy_path.exists() {
        info!("Skipped: {} (already exists)", policy_path.display());
    } else {
        fs::write(&policy_path, default_policy)
            .with_context(|| "Failed to create policy.toon")?;
        info!("Created: {}", policy_path.display());
    }

    // Create .gitignore additions
    let gitignore_content = r"# Nectar generated files
rules.yaml
policy.lock
waggle.md

# Local overrides
*.local.toon
";

    let gitignore_path = project_path.join(".gitignore");
    if gitignore_path.exists() {
        // Append to existing .gitignore
        let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
        if !existing.contains("# Nectar generated files") {
            let mut content = existing;
            content.push('\n');
            content.push_str(gitignore_content);
            fs::write(&gitignore_path, content)
                .with_context(|| "Failed to update .gitignore")?;
            info!("Updated: {}", gitignore_path.display());
        }
    } else {
        fs::write(&gitignore_path, gitignore_content)
            .with_context(|| "Failed to create .gitignore")?;
        info!("Created: {}", gitignore_path.display());
    }

    // Create README
    let readme_content = r#"# Nectar Sampling Policy

This directory contains Nectar sampling policies for Honeycomb Refinery.

## Files

- `policy.toon` - Source of truth for sampling rules (edit this)
- `rules.yaml` - Generated Refinery configuration (don't edit)
- `waggle.md` - Human-readable explanation of the policy

## Commands

```bash
# Compile policy to Refinery rules
nectar compile

# Verify policy against trace corpus
nectar prove

# Generate a new policy from intent
nectar propose "Keep all errors and slow traces, sample rest at 1%"

# Generate waggle report
nectar explain
```

## Workflow

1. Edit `policy.toon` or use `nectar propose` to generate
2. Run `nectar prove` to verify against historical data
3. Run `nectar compile` to generate `rules.yaml`
4. Deploy `rules.yaml` to Refinery
"#;

    let readme_path = project_path.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, readme_content)
            .with_context(|| "Failed to create README.md")?;
        info!("Created: {}", readme_path.display());
    }

    info!("Nectar project initialized successfully!");
    info!("");
    info!("Next steps:");
    info!("  1. Edit policy.toon to define your sampling rules");
    info!("  2. Add trace exemplars to corpus/");
    info!("  3. Run 'nectar prove' to verify");
    info!("  4. Run 'nectar compile' to generate rules.yaml");

    Ok(())
}
