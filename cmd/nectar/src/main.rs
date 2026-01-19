//! Nectar CLI - AI-native sampling policy engine.
//!
//! Commands:
//! - `nectar compile` - Compile policy.toon to rules.yaml
//! - `nectar prove` - Verify policy against corpus
//! - `nectar propose` - Generate policy from intent (uses Claude)
//! - `nectar explain` - Generate waggle report

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod commands;

#[derive(Parser)]
#[command(name = "nectar")]
#[command(about = "AI-native sampling policy engine for Honeycomb Refinery")]
#[command(version)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a TOON policy to Refinery rules
    Compile {
        /// Path to policy.toon file
        #[arg(short, long, default_value = "policy.toon")]
        policy: String,

        /// Output path for rules.yaml
        #[arg(short, long, default_value = "rules.yaml")]
        output: String,

        /// Output format (yaml or json)
        #[arg(short, long, default_value = "yaml")]
        format: String,

        /// Create a policy.lock file for verification
        #[arg(long)]
        lock: bool,
    },

    /// Verify a policy against a trace corpus
    Prove {
        /// Path to policy.toon file
        #[arg(short, long, default_value = "policy.toon")]
        policy: String,

        /// Path to corpus directory
        #[arg(short, long, default_value = "corpus")]
        corpus: String,

        /// Fail on warnings (not just errors)
        #[arg(long)]
        strict: bool,
    },

    /// Generate a policy from natural language intent
    Propose {
        /// Natural language description of desired policy
        intent: String,

        /// Path to corpus directory for context
        #[arg(short, long)]
        corpus: Option<String>,

        /// Path to existing policy to refine
        #[arg(short, long)]
        policy: Option<String>,

        /// Output path for generated policy
        #[arg(short, long, default_value = "policy.toon")]
        output: String,
    },

    /// Generate a waggle report explaining the policy
    Explain {
        /// Path to policy.toon file
        #[arg(short, long, default_value = "policy.toon")]
        policy: String,

        /// Output path for waggle.md
        #[arg(short, long, default_value = "waggle.md")]
        output: String,
    },

    /// Initialize a new Nectar project
    Init {
        /// Project name
        #[arg(default_value = ".")]
        path: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.command {
        Commands::Compile {
            policy,
            output,
            format,
            lock,
        } => commands::compile::run(&policy, &output, &format, lock),
        Commands::Prove {
            policy,
            corpus,
            strict,
        } => commands::prove::run(&policy, &corpus, strict),
        Commands::Propose {
            intent,
            corpus,
            policy,
            output,
        } => commands::propose::run(&intent, corpus.as_deref(), policy.as_deref(), &output).await,
        Commands::Explain { policy, output } => commands::explain::run(&policy, &output),
        Commands::Init { path } => commands::init::run(&path),
    }
}
