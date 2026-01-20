//! Watch command implementation.
//!
//! Continuously monitors production traffic and suggests policy refinements.

// Allow dead code for foundational types that will be used by future features
#![allow(dead_code)]

use anyhow::{Context, Result};
use nectar_corpus::Corpus;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Watch mode configuration.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Path to policy file.
    pub policy_path: String,
    /// OTLP gRPC port (if using OTLP receiver).
    pub otlp_port: Option<u16>,
    /// Honeycomb dataset (if using Honeycomb API).
    pub honeycomb_dataset: Option<String>,
    /// Honeycomb API key.
    pub honeycomb_api_key: Option<String>,
    /// Dry-run mode (suggest only, don't apply).
    pub dry_run: bool,
    /// Interval for policy drift checks (in seconds).
    pub check_interval_secs: u64,
    /// Maximum corpus size (for reservoir sampling).
    pub max_corpus_size: usize,
    /// Path to corpus directory for persistence.
    pub corpus_path: Option<String>,
    /// Webhook URL for alerts.
    pub webhook_url: Option<String>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            policy_path: "policy.toon".to_string(),
            otlp_port: None,
            honeycomb_dataset: None,
            honeycomb_api_key: None,
            dry_run: false,
            check_interval_secs: 60,
            max_corpus_size: 10_000,
            corpus_path: None,
            webhook_url: None,
        }
    }
}

/// Events that can occur during watch mode.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// New trace received.
    TraceReceived(nectar_corpus::Trace),
    /// Policy drift detected.
    DriftDetected(DriftReport),
    /// Budget violation detected.
    BudgetViolation(BudgetViolationReport),
    /// Refinement suggestion generated.
    RefinementSuggested(RefinementSuggestion),
    /// Error occurred.
    Error(String),
    /// Shutdown requested.
    Shutdown,
}

/// Report of detected policy drift.
#[derive(Debug, Clone)]
pub struct DriftReport {
    /// Description of the drift.
    pub description: String,
    /// Severity (low, medium, high).
    pub severity: String,
    /// Affected rules.
    pub affected_rules: Vec<String>,
    /// Timestamp of detection.
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

/// Report of budget violation.
#[derive(Debug, Clone)]
pub struct BudgetViolationReport {
    /// Current throughput.
    pub current_throughput: f64,
    /// Budget limit.
    pub budget_limit: f64,
    /// Percentage over budget.
    pub over_budget_percent: f64,
    /// Timestamp of detection.
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

/// Suggestion for policy refinement.
#[derive(Debug, Clone)]
pub struct RefinementSuggestion {
    /// Description of the suggestion.
    pub description: String,
    /// Suggested change (in TOON format).
    pub suggested_change: String,
    /// Reason for the suggestion.
    pub reason: String,
    /// Confidence level (0.0 to 1.0).
    pub confidence: f64,
}

/// Watch mode state.
#[derive(Debug)]
pub struct WatchState {
    /// Rolling corpus of trace exemplars.
    pub corpus: Corpus,
    /// Number of traces seen.
    pub traces_seen: u64,
    /// Number of traces kept.
    pub traces_kept: u64,
    /// Number of drift events.
    pub drift_events: u64,
    /// Number of budget violations.
    pub budget_violations: u64,
    /// Start time.
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl Default for WatchState {
    fn default() -> Self {
        Self {
            corpus: Corpus::new(),
            traces_seen: 0,
            traces_kept: 0,
            drift_events: 0,
            budget_violations: 0,
            started_at: chrono::Utc::now(),
        }
    }
}

/// The main watcher for continuous policy monitoring.
pub struct Watcher {
    config: WatchConfig,
    state: WatchState,
    running: Arc<AtomicBool>,
    event_tx: mpsc::Sender<WatchEvent>,
    event_rx: mpsc::Receiver<WatchEvent>,
}

impl Watcher {
    /// Creates a new watcher with the given configuration.
    pub fn new(config: WatchConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);
        Self {
            config,
            state: WatchState::default(),
            running: Arc::new(AtomicBool::new(false)),
            event_tx,
            event_rx,
        }
    }

    /// Returns the event sender for external components to send events.
    pub fn event_sender(&self) -> mpsc::Sender<WatchEvent> {
        self.event_tx.clone()
    }

    /// Returns whether the watcher is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Signals the watcher to stop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Returns the current state.
    pub const fn state(&self) -> &WatchState {
        &self.state
    }

    /// Runs the watch loop.
    pub async fn run(&mut self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        info!("Starting watch mode...");
        info!("Policy: {}", self.config.policy_path);

        if let Some(port) = self.config.otlp_port {
            info!("OTLP receiver: port {}", port);
        }
        if let Some(ref dataset) = self.config.honeycomb_dataset {
            info!("Honeycomb dataset: {}", dataset);
        }
        if self.config.dry_run {
            info!("Dry-run mode: suggestions only, no changes applied");
        }

        // Load existing corpus if path provided
        if let Some(corpus_path) = self.config.corpus_path.clone() {
            self.load_existing_corpus(&corpus_path)?;
        }

        // Start input sources
        self.start_input_sources();

        // Main event loop
        let mut check_interval = interval(Duration::from_secs(self.config.check_interval_secs));

        while self.running.load(Ordering::SeqCst) {
            tokio::select! {
                // Handle incoming events
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event);
                }

                // Periodic drift check
                _ = check_interval.tick() => {
                    self.check_for_drift();
                }

                // Handle Ctrl+C
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    self.running.store(false, Ordering::SeqCst);
                }
            }
        }

        // Cleanup
        self.shutdown();

        Ok(())
    }

    /// Loads existing corpus from disk.
    fn load_existing_corpus(&mut self, path: &str) -> Result<()> {
        let path = Path::new(path);
        if path.exists() {
            if path.is_dir() {
                self.state.corpus = Corpus::load_directory(path)
                    .with_context(|| format!("Failed to load corpus from: {}", path.display()))?;
            } else {
                self.state.corpus = Corpus::load_file(path)
                    .with_context(|| format!("Failed to load corpus file: {}", path.display()))?;
            }
            info!("Loaded {} existing traces", self.state.corpus.len());
        }
        Ok(())
    }

    /// Starts input sources (OTLP receiver, Honeycomb polling, etc.).
    fn start_input_sources(&self) {
        // OTLP receiver (placeholder - implemented in #8)
        if let Some(port) = self.config.otlp_port {
            info!("OTLP receiver would start on port {} (not yet implemented)", port);
            // TODO: Start OTLP gRPC receiver (#8)
        }

        // Honeycomb polling (placeholder - implemented in #12)
        if self.config.honeycomb_dataset.is_some() {
            info!("Honeycomb polling would start (not yet implemented)");
            // TODO: Start Honeycomb API polling (#12)
        }

        // If no input source configured, warn
        if self.config.otlp_port.is_none() && self.config.honeycomb_dataset.is_none() {
            warn!("No input source configured. Use --otlp-port or --honeycomb-dataset");
        }
    }

    /// Handles a watch event.
    fn handle_event(&mut self, event: WatchEvent) {
        match event {
            WatchEvent::TraceReceived(trace) => {
                self.handle_trace(trace);
            }
            WatchEvent::DriftDetected(report) => {
                self.handle_drift(&report);
            }
            WatchEvent::BudgetViolation(report) => {
                self.handle_budget_violation(&report);
            }
            WatchEvent::RefinementSuggested(suggestion) => {
                self.handle_suggestion(&suggestion);
            }
            WatchEvent::Error(msg) => {
                error!("Watch error: {}", msg);
            }
            WatchEvent::Shutdown => {
                info!("Shutdown event received");
                self.running.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Handles a new trace.
    fn handle_trace(&mut self, trace: nectar_corpus::Trace) {
        self.state.traces_seen += 1;

        // Add to corpus with reservoir sampling (placeholder - implemented in #9)
        if self.state.corpus.len() < self.config.max_corpus_size {
            self.state.corpus.add(trace);
            self.state.traces_kept += 1;
        } else {
            // TODO: Implement reservoir sampling (#9)
            // For now, just skip if at capacity
            debug!("Corpus at capacity, skipping trace");
        }

        // Log progress periodically
        if self.state.traces_seen % 1000 == 0 {
            info!(
                "Traces: {} seen, {} kept, corpus size: {}",
                self.state.traces_seen,
                self.state.traces_kept,
                self.state.corpus.len()
            );
        }
    }

    /// Handles drift detection.
    fn handle_drift(&mut self, report: &DriftReport) {
        self.state.drift_events += 1;
        warn!(
            "Policy drift detected [{}]: {}",
            report.severity, report.description
        );

        // Send alert (placeholder - implemented in #13)
        if self.config.webhook_url.is_some() {
            // TODO: Send webhook alert (#13)
            debug!("Would send drift alert to webhook");
        }
    }

    /// Handles budget violation.
    fn handle_budget_violation(&mut self, report: &BudgetViolationReport) {
        self.state.budget_violations += 1;
        error!(
            "Budget violation: {:.1} events/sec (limit: {:.1}, {:.1}% over)",
            report.current_throughput, report.budget_limit, report.over_budget_percent
        );

        // Send alert (placeholder - implemented in #13)
        if self.config.webhook_url.is_some() {
            // TODO: Send webhook alert (#13)
            debug!("Would send budget violation alert to webhook");
        }
    }

    /// Handles a refinement suggestion.
    fn handle_suggestion(&self, suggestion: &RefinementSuggestion) {
        info!("Refinement suggestion (confidence: {:.0}%):", suggestion.confidence * 100.0);
        info!("  Reason: {}", suggestion.reason);
        info!("  Suggestion: {}", suggestion.description);

        if self.config.dry_run {
            info!("  [Dry-run mode - not applying]");
        } else {
            // TODO: Apply suggestion (#11)
            info!("  [Would apply change - not yet implemented]");
        }
    }

    /// Checks for policy drift.
    #[allow(clippy::unused_self)] // Will use self when #10 is implemented
    fn check_for_drift(&self) {
        // Placeholder - implemented in #10
        debug!("Checking for policy drift...");

        // TODO: Implement drift detection (#10)
        // - Compare current traffic patterns against policy rules
        // - Detect rules that no longer match
        // - Detect new patterns not covered by policy
    }

    /// Performs cleanup on shutdown.
    fn shutdown(&self) {
        info!("Shutting down watch mode...");

        let uptime = chrono::Utc::now() - self.state.started_at;
        info!("Watch mode statistics:");
        info!("  Uptime: {}s", uptime.num_seconds());
        info!("  Traces seen: {}", self.state.traces_seen);
        info!("  Traces kept: {}", self.state.traces_kept);
        info!("  Drift events: {}", self.state.drift_events);
        info!("  Budget violations: {}", self.state.budget_violations);
        info!("  Final corpus size: {}", self.state.corpus.len());

        // Save corpus if path provided
        if let Some(ref corpus_path) = self.config.corpus_path {
            info!("Saving corpus to {}...", corpus_path);
            // TODO: Implement corpus persistence
        }

        info!("Watch mode stopped");
    }
}

/// Runs the watch command.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    policy_path: &str,
    otlp_port: Option<u16>,
    honeycomb_dataset: Option<String>,
    honeycomb_api_key: Option<String>,
    corpus_path: Option<String>,
    dry_run: bool,
    check_interval: u64,
    max_corpus_size: usize,
    webhook_url: Option<String>,
) -> Result<()> {
    // Validate policy exists
    if !Path::new(policy_path).exists() {
        anyhow::bail!("Policy file not found: {policy_path}");
    }

    // Validate input source
    if otlp_port.is_none() && honeycomb_dataset.is_none() {
        warn!("No input source specified. Use --otlp-port or --honeycomb-dataset");
        warn!("Watch mode will start but won't receive any traces.");
    }

    // Validate Honeycomb config
    if honeycomb_dataset.is_some() && honeycomb_api_key.is_none() {
        // Try to read from environment
        if std::env::var("HONEYCOMB_API_KEY").is_err() {
            anyhow::bail!("Honeycomb dataset specified but no API key provided. Use --honeycomb-api-key or set HONEYCOMB_API_KEY");
        }
    }

    let config = WatchConfig {
        policy_path: policy_path.to_string(),
        otlp_port,
        honeycomb_dataset,
        honeycomb_api_key: honeycomb_api_key.or_else(|| std::env::var("HONEYCOMB_API_KEY").ok()),
        dry_run,
        check_interval_secs: check_interval,
        max_corpus_size,
        corpus_path,
        webhook_url,
    };

    let mut watcher = Watcher::new(config);
    watcher.run().await
}
