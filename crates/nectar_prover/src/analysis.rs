//! Static analysis module for fast policy verification.
//!
//! Performs rule-level analysis without simulating traffic,
//! enabling rapid feedback during policy development.

use crate::result::{Severity, Violation};
use serde::{Deserialize, Serialize};
use toon_policy::{Action, Policy};

/// Analysis mode for policy verification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisMode {
    /// Fast rule analysis only (O(rules)).
    /// Checks coverage, conflicts, fallback without traffic simulation.
    Static,
    /// Full traffic simulation (O(rules × events)).
    /// Requires traffic pattern, provides accurate budget verification.
    Dynamic,
    /// Static for iterations, dynamic for final prove.
    /// Best for interactive policy development.
    #[default]
    Auto,
}

impl AnalysisMode {
    /// Returns true if this mode performs static analysis.
    #[must_use]
    pub const fn includes_static(&self) -> bool {
        matches!(self, Self::Static | Self::Auto)
    }

    /// Returns true if this mode performs dynamic simulation.
    #[must_use]
    pub const fn includes_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic | Self::Auto)
    }
}

/// Confidence level for analysis results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confidence {
    /// Low confidence - based on heuristics only.
    Low,
    /// Medium confidence - static analysis passed.
    Medium,
    /// High confidence - dynamic simulation passed.
    High,
}

impl Confidence {
    /// Returns a human-readable description.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Low => "heuristic analysis only",
            Self::Medium => "static analysis passed",
            Self::High => "dynamic simulation passed",
        }
    }
}

/// Result of static analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticAnalysisResult {
    /// Whether static checks passed.
    pub passed: bool,
    /// Violations found.
    pub violations: Vec<Violation>,
    /// Warnings (non-blocking issues).
    pub warnings: Vec<StaticWarning>,
    /// Coverage analysis.
    pub coverage: CoverageAnalysis,
    /// Conflict detection results.
    pub conflicts: Vec<RuleConflict>,
    /// Confidence level of this analysis.
    pub confidence: Confidence,
}

impl StaticAnalysisResult {
    /// Creates a passing result.
    #[must_use]
    pub const fn passed(coverage: CoverageAnalysis) -> Self {
        Self {
            passed: true,
            violations: Vec::new(),
            warnings: Vec::new(),
            coverage,
            conflicts: Vec::new(),
            confidence: Confidence::Medium,
        }
    }

    /// Creates a failing result.
    #[must_use]
    pub const fn failed(violations: Vec<Violation>, coverage: CoverageAnalysis) -> Self {
        Self {
            passed: false,
            violations,
            warnings: Vec::new(),
            coverage,
            conflicts: Vec::new(),
            confidence: Confidence::Low,
        }
    }

    /// Adds warnings to the result.
    #[must_use]
    pub fn with_warnings(mut self, warnings: Vec<StaticWarning>) -> Self {
        self.warnings = warnings;
        self
    }

    /// Adds conflicts to the result.
    #[must_use]
    pub fn with_conflicts(mut self, conflicts: Vec<RuleConflict>) -> Self {
        self.conflicts = conflicts;
        self
    }
}

/// Warning from static analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticWarning {
    /// Rule that triggered the warning.
    pub rule_name: String,
    /// Warning message.
    pub message: String,
    /// Suggested fix.
    pub suggestion: Option<String>,
}

impl StaticWarning {
    /// Creates a new warning.
    #[must_use]
    pub fn new(rule_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_name: rule_name.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Adds a suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Coverage analysis results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageAnalysis {
    /// Total rules in policy.
    pub total_rules: usize,
    /// Rules that match keep action.
    pub keep_rules: usize,
    /// Rules that match drop action.
    pub drop_rules: usize,
    /// Rules that match sample action.
    pub sample_rules: usize,
    /// Whether a fallback rule exists.
    pub has_fallback: bool,
    /// Whether error handling exists.
    pub has_error_handling: bool,
    /// Estimated coverage percentage (0-100).
    pub estimated_coverage: f64,
}

impl CoverageAnalysis {
    /// Analyzes a policy for coverage.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn analyze(policy: &Policy) -> Self {
        let mut analysis = Self {
            total_rules: policy.rules.len(),
            ..Default::default()
        };

        for rule in &policy.rules {
            match rule.action {
                Action::Keep => analysis.keep_rules += 1,
                Action::Drop => analysis.drop_rules += 1,
                Action::Sample(_) => analysis.sample_rules += 1,
            }

            // Check for fallback
            if rule.match_expr == "true" {
                analysis.has_fallback = true;
            }

            // Check for error handling
            if is_error_condition(&rule.match_expr) {
                analysis.has_error_handling = true;
            }
        }

        // Estimate coverage based on rule types
        analysis.estimated_coverage = calculate_coverage_estimate(&analysis);

        analysis
    }
}

/// A conflict between two rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConflict {
    /// First rule in conflict.
    pub rule_a: String,
    /// Second rule in conflict.
    pub rule_b: String,
    /// Type of conflict.
    pub conflict_type: ConflictType,
    /// Description of the conflict.
    pub description: String,
}

/// Types of rule conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictType {
    /// Rule is unreachable due to earlier rule.
    Shadowed,
    /// Rules have overlapping conditions with different actions.
    Overlapping,
    /// Rule contradicts another rule's intent.
    Contradictory,
}

impl RuleConflict {
    /// Creates a shadowed conflict.
    #[must_use]
    pub fn shadowed(rule_a: impl Into<String>, rule_b: impl Into<String>) -> Self {
        let rule_b = rule_b.into();
        Self {
            rule_a: rule_a.into(),
            rule_b: rule_b.clone(),
            conflict_type: ConflictType::Shadowed,
            description: format!("Rule '{rule_b}' is shadowed by an earlier rule"),
        }
    }

    /// Creates an overlapping conflict.
    #[must_use]
    pub fn overlapping(
        rule_a: impl Into<String>,
        rule_b: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            rule_a: rule_a.into(),
            rule_b: rule_b.into(),
            conflict_type: ConflictType::Overlapping,
            description: description.into(),
        }
    }
}

/// Static analyzer for policies.
#[derive(Debug, Clone, Default)]
pub struct StaticAnalyzer {
    /// Whether to check for shadowed rules.
    pub check_shadowing: bool,
    /// Whether to check for overlapping conditions.
    pub check_overlaps: bool,
}

impl StaticAnalyzer {
    /// Creates a new analyzer with all checks enabled.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            check_shadowing: true,
            check_overlaps: true,
        }
    }

    /// Performs static analysis on a policy.
    #[must_use]
    pub fn analyze(&self, policy: &Policy) -> StaticAnalysisResult {
        let coverage = CoverageAnalysis::analyze(policy);
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let mut conflicts = Vec::new();

        // Check for fallback rule
        if !coverage.has_fallback {
            violations.push(Violation::new(
                Severity::Critical,
                "missing-fallback",
                "Policy must have a fallback rule matching 'true'",
            ));
        }

        // Check for error handling
        if !coverage.has_error_handling {
            warnings.push(StaticWarning::new(
                "policy",
                "No explicit error handling detected",
            ).with_suggestion("Add a rule to keep error traces (e.g., status >= 500)"));
        }

        // Check for empty policy
        if policy.rules.is_empty() {
            violations.push(Violation::new(
                Severity::Critical,
                "empty-policy",
                "Policy has no rules",
            ));
        }

        // Check for rule conflicts
        if self.check_shadowing {
            conflicts.extend(self.detect_shadowing(policy));
        }

        if self.check_overlaps {
            conflicts.extend(self.detect_overlaps(policy));
        }

        // Add warnings for conflicts
        for conflict in &conflicts {
            warnings.push(StaticWarning::new(
                &conflict.rule_b,
                &conflict.description,
            ));
        }

        if violations.is_empty() {
            StaticAnalysisResult::passed(coverage)
                .with_warnings(warnings)
                .with_conflicts(conflicts)
        } else {
            StaticAnalysisResult::failed(violations, coverage)
                .with_warnings(warnings)
                .with_conflicts(conflicts)
        }
    }

    /// Detects shadowed rules.
    fn detect_shadowing(&self, policy: &Policy) -> Vec<RuleConflict> {
        let mut conflicts = Vec::new();

        for (i, rule) in policy.rules.iter().enumerate() {
            // Check if any earlier rule shadows this one
            for earlier in policy.rules.iter().take(i) {
                if self.rule_shadows(earlier, rule) {
                    conflicts.push(RuleConflict::shadowed(&earlier.name, &rule.name));
                }
            }
        }

        conflicts
    }

    /// Checks if `rule_a` shadows `rule_b`.
    #[allow(clippy::unused_self)]
    fn rule_shadows(&self, rule_a: &toon_policy::Rule, rule_b: &toon_policy::Rule) -> bool {
        // A "true" rule shadows everything after it with same or lower priority
        if rule_a.match_expr == "true" && rule_a.priority >= rule_b.priority {
            return true;
        }

        // Same condition shadows
        if rule_a.match_expr == rule_b.match_expr && rule_a.priority >= rule_b.priority {
            return true;
        }

        false
    }

    /// Detects overlapping rules with different actions.
    fn detect_overlaps(&self, policy: &Policy) -> Vec<RuleConflict> {
        let mut conflicts = Vec::new();

        for (i, rule_a) in policy.rules.iter().enumerate() {
            for rule_b in policy.rules.iter().skip(i + 1) {
                if let Some(conflict) = self.check_overlap(rule_a, rule_b) {
                    conflicts.push(conflict);
                }
            }
        }

        conflicts
    }

    /// Checks if two rules have a problematic overlap.
    fn check_overlap(
        &self,
        rule_a: &toon_policy::Rule,
        rule_b: &toon_policy::Rule,
    ) -> Option<RuleConflict> {
        // Skip if same action
        if rule_a.action == rule_b.action {
            return None;
        }

        // Check for contradictory conditions
        // e.g., "status >= 500" (Keep) vs "status >= 400" (Drop)
        if self.conditions_overlap(&rule_a.match_expr, &rule_b.match_expr) {
            // Only flag if actions contradict (Keep vs Drop)
            if is_contradictory_action(&rule_a.action, &rule_b.action) {
                return Some(RuleConflict::overlapping(
                    &rule_a.name,
                    &rule_b.name,
                    format!(
                        "Rules '{}' and '{}' have overlapping conditions with contradictory actions",
                        rule_a.name, rule_b.name
                    ),
                ));
            }
        }

        None
    }

    /// Checks if two conditions might overlap.
    /// This is a heuristic check - full overlap detection would require
    /// symbolic execution.
    #[allow(clippy::unused_self)]
    fn conditions_overlap(&self, expr_a: &str, expr_b: &str) -> bool {
        // "true" overlaps with everything
        if expr_a == "true" || expr_b == "true" {
            return true;
        }

        // Same field comparisons might overlap
        let fields_a = extract_fields(expr_a);
        let fields_b = extract_fields(expr_b);

        // If they operate on the same fields, they might overlap
        fields_a.iter().any(|f| fields_b.contains(f))
    }
}

/// Checks if expression is an error condition.
fn is_error_condition(expr: &str) -> bool {
    let lower = expr.to_lowercase();
    lower.contains("error")
        || lower.contains("status >= 500")
        || lower.contains("status >= 400")
        || lower.contains("is_error")
        || lower.contains("exception")
}

/// Calculates coverage estimate based on rule analysis.
#[allow(clippy::cast_precision_loss)]
fn calculate_coverage_estimate(analysis: &CoverageAnalysis) -> f64 {
    let mut score = 0.0;

    // Base score from having rules
    if analysis.total_rules > 0 {
        score += 20.0;
    }

    // Fallback is critical
    if analysis.has_fallback {
        score += 30.0;
    }

    // Error handling is important
    if analysis.has_error_handling {
        score += 20.0;
    }

    // Mix of rule types is good
    let rule_type_diversity = [
        analysis.keep_rules > 0,
        analysis.drop_rules > 0,
        analysis.sample_rules > 0,
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    score += (rule_type_diversity as f64) * 10.0;

    score.min(100.0)
}

/// Checks if two actions are contradictory.
const fn is_contradictory_action(a: &Action, b: &Action) -> bool {
    matches!(
        (a, b),
        (Action::Keep, Action::Drop) | (Action::Drop, Action::Keep)
    )
}

/// Extracts field names from an expression.
fn extract_fields(expr: &str) -> Vec<&str> {
    // Simple heuristic: extract words that look like field names
    let field_patterns = ["status", "duration", "service", "endpoint", "error", "name"];

    field_patterns
        .iter()
        .filter(|&&f| expr.contains(f))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::Rule;

    fn sample_policy() -> Policy {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));
        policy.add_rule(Rule::new("sample-normal", "true", Action::Sample(0.1), 0));
        policy
    }

    #[test]
    fn analysis_mode_includes() {
        assert!(AnalysisMode::Static.includes_static());
        assert!(!AnalysisMode::Static.includes_dynamic());

        assert!(!AnalysisMode::Dynamic.includes_static());
        assert!(AnalysisMode::Dynamic.includes_dynamic());

        assert!(AnalysisMode::Auto.includes_static());
        assert!(AnalysisMode::Auto.includes_dynamic());
    }

    #[test]
    fn coverage_analysis() {
        let policy = sample_policy();
        let coverage = CoverageAnalysis::analyze(&policy);

        assert_eq!(coverage.total_rules, 2);
        assert_eq!(coverage.keep_rules, 1);
        assert_eq!(coverage.sample_rules, 1);
        assert!(coverage.has_fallback);
        assert!(coverage.has_error_handling);
    }

    #[test]
    fn static_analysis_passes() {
        let analyzer = StaticAnalyzer::new();
        let policy = sample_policy();

        let result = analyzer.analyze(&policy);

        assert!(result.passed);
        assert!(result.violations.is_empty());
        assert_eq!(result.confidence, Confidence::Medium);
    }

    #[test]
    fn static_analysis_fails_without_fallback() {
        let analyzer = StaticAnalyzer::new();
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));

        let result = analyzer.analyze(&policy);

        assert!(!result.passed);
        assert!(result.violations.iter().any(|v| v.check == "missing-fallback"));
    }

    #[test]
    fn detects_shadowed_rules() {
        let analyzer = StaticAnalyzer::new();
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("catch-all", "true", Action::Keep, 100));
        policy.add_rule(Rule::new("errors", "status >= 500", Action::Keep, 50));

        let result = analyzer.analyze(&policy);

        assert!(!result.conflicts.is_empty());
        assert!(result.conflicts.iter().any(|c| c.conflict_type == ConflictType::Shadowed));
    }

    #[test]
    fn confidence_ordering() {
        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
    }

    #[test]
    fn empty_policy_fails() {
        let analyzer = StaticAnalyzer::new();
        let policy = Policy::new("empty");

        let result = analyzer.analyze(&policy);

        assert!(!result.passed);
        assert!(result.violations.iter().any(|v| v.check == "empty-policy"));
    }
}
