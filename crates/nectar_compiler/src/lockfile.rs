//! Policy lockfile for deterministic compilation verification.
//!
//! The lockfile provides a way to verify that a policy compiles to the same
//! output across builds. It contains:
//! - A hash of the source policy
//! - A hash of the compiled output
//! - Metadata about the compilation

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use toon_policy::Policy;
use xxhash_rust::xxh64::xxh64;

/// Seed for xxhash to ensure deterministic hashing.
const HASH_SEED: u64 = 0x4E45_4354_4152; // "NECTAR" in hex

/// A policy lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Version of the lockfile format.
    pub version: u32,
    /// Hash of the source policy (TOON format).
    pub source_hash: String,
    /// Hash of the compiled output.
    pub compiled_hash: String,
    /// Policy name.
    pub policy_name: String,
    /// Number of rules in the policy.
    pub rule_count: usize,
    /// Timestamp when the lock was created (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl Lockfile {
    /// Creates a new lockfile from a policy and its compiled output.
    #[must_use]
    pub fn new(policy: &Policy, source_toon: &str, compiled_output: &str) -> Self {
        Self {
            version: 1,
            source_hash: hash_content(source_toon),
            compiled_hash: hash_content(compiled_output),
            policy_name: policy.name.clone(),
            rule_count: policy.rules.len(),
            created_at: None,
        }
    }

    /// Creates a lockfile with a timestamp.
    #[must_use]
    pub fn with_timestamp(mut self) -> Self {
        self.created_at = Some(chrono_lite_now());
        self
    }

    /// Verifies that the compiled output matches the lockfile.
    ///
    /// Returns `true` if the hashes match.
    #[must_use]
    pub fn verify(&self, source_toon: &str, compiled_output: &str) -> bool {
        let source_matches = self.source_hash == hash_content(source_toon);
        let compiled_matches = self.compiled_hash == hash_content(compiled_output);
        source_matches && compiled_matches
    }

    /// Verifies only the source hash matches.
    ///
    /// Returns `true` if the source hash matches.
    #[must_use]
    pub fn verify_source(&self, source_toon: &str) -> bool {
        self.source_hash == hash_content(source_toon)
    }

    /// Verifies only the compiled hash matches.
    ///
    /// Returns `true` if the compiled hash matches.
    #[must_use]
    pub fn verify_compiled(&self, compiled_output: &str) -> bool {
        self.compiled_hash == hash_content(compiled_output)
    }

    /// Loads a lockfile from a path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        let lockfile: Self = serde_json::from_str(&content)?;
        Ok(lockfile)
    }

    /// Saves the lockfile to a path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path.as_ref(), content)?;
        Ok(())
    }

    /// Serializes the lockfile to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parses a lockfile from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

/// Computes a deterministic hash of content.
fn hash_content(content: &str) -> String {
    let hash = xxh64(content.as_bytes(), HASH_SEED);
    format!("{hash:016x}")
}

/// Simple timestamp generator (no chrono dependency).
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();

    // Convert to date components
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    )
}

/// Converts days since epoch to year/month/day.
#[allow(clippy::cast_possible_wrap)]
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Simplified calculation - days since epoch won't exceed i64 range
    let mut remaining_days = days as i64;
    let mut year: u32 = 1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month: u32 = 1;
    for &days_in_month in &month_days {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let day = (remaining_days + 1) as u32;

    (year, month, day)
}

const fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

impl From<std::io::Error> for crate::error::Error {
    fn from(e: std::io::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::{Action, Rule};

    #[test]
    fn hash_is_deterministic() {
        let content = "test content";
        let hash1 = hash_content(content);
        let hash2 = hash_content(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn different_content_different_hash() {
        let hash1 = hash_content("content a");
        let hash2 = hash_content("content b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn lockfile_creation() {
        let mut policy = Policy::new("test-policy");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));

        let source = "nectar_policy{...}";
        let compiled = "RulesBasedSampler: ...";

        let lockfile = Lockfile::new(&policy, source, compiled);

        assert_eq!(lockfile.version, 1);
        assert_eq!(lockfile.policy_name, "test-policy");
        assert_eq!(lockfile.rule_count, 1);
        assert!(!lockfile.source_hash.is_empty());
        assert!(!lockfile.compiled_hash.is_empty());
    }

    #[test]
    fn lockfile_verification() {
        let policy = Policy::new("test");
        let source = "source content";
        let compiled = "compiled content";

        let lockfile = Lockfile::new(&policy, source, compiled);

        // Same content should verify
        assert!(lockfile.verify(source, compiled));
        assert!(lockfile.verify_source(source));
        assert!(lockfile.verify_compiled(compiled));

        // Different content should not verify
        assert!(!lockfile.verify("different", compiled));
        assert!(!lockfile.verify(source, "different"));
    }

    #[test]
    fn lockfile_roundtrip() {
        let policy = Policy::new("test");
        let lockfile = Lockfile::new(&policy, "source", "compiled");

        let json = lockfile.to_json().unwrap();
        let parsed = Lockfile::from_json(&json).unwrap();

        assert_eq!(lockfile.source_hash, parsed.source_hash);
        assert_eq!(lockfile.compiled_hash, parsed.compiled_hash);
        assert_eq!(lockfile.policy_name, parsed.policy_name);
    }

    #[test]
    fn timestamp_generation() {
        let policy = Policy::new("test");
        let lockfile = Lockfile::new(&policy, "source", "compiled").with_timestamp();

        assert!(lockfile.created_at.is_some());
        let ts = lockfile.created_at.unwrap();
        // Should be ISO 8601 format
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }
}
