//! Waggle report generation.
//!
//! The "waggle dance" is how bees communicate the location of resources.
//! In Nectar, the waggle report explains policy decisions to humans.

use toon_policy::Policy;

/// Generates a human-readable explanation of a policy.
///
/// The waggle report includes:
/// - Policy overview
/// - Rule-by-rule explanation
/// - Expected behavior summary
#[must_use]
pub fn generate_waggle_report(policy: &Policy) -> String {
    let mut report = String::new();

    // Header
    report.push_str(&format!("# Waggle Report: {}\n\n", policy.name));

    // Overview
    report.push_str("## Overview\n\n");
    report.push_str(&format!("- **Version**: {}\n", policy.version));
    if let Some(budget) = policy.budget_per_second {
        report.push_str(&format!("- **Budget**: {} traces/second\n", budget));
    }
    report.push_str(&format!("- **Rules**: {}\n", policy.rules.len()));
    report.push('\n');

    // Rules
    report.push_str("## Rules\n\n");
    report.push_str("Rules are evaluated in priority order (highest first).\n\n");

    for (i, rule) in policy.rules.iter().enumerate() {
        report.push_str(&format!("### {}. {} (priority: {})\n\n", i + 1, rule.name, rule.priority));
        
        if let Some(desc) = &rule.description {
            report.push_str(&format!("{}\n\n", desc));
        }

        report.push_str(&format!("- **Match**: `{}`\n", rule.match_expr));
        report.push_str(&format!("- **Action**: {}\n", format_action(&rule.action)));
        report.push('\n');
    }

    // Summary
    report.push_str("## Summary\n\n");
    
    let keep_rules: Vec<_> = policy.rules.iter()
        .filter(|r| matches!(r.action, toon_policy::Action::Keep))
        .collect();
    let sample_rules: Vec<_> = policy.rules.iter()
        .filter(|r| matches!(r.action, toon_policy::Action::Sample(_)))
        .collect();
    let drop_rules: Vec<_> = policy.rules.iter()
        .filter(|r| matches!(r.action, toon_policy::Action::Drop))
        .collect();

    if !keep_rules.is_empty() {
        report.push_str("**Always kept**: ");
        report.push_str(&keep_rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", "));
        report.push('\n');
    }
    if !sample_rules.is_empty() {
        report.push_str("**Sampled**: ");
        report.push_str(&sample_rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", "));
        report.push('\n');
    }
    if !drop_rules.is_empty() {
        report.push_str("**Dropped**: ");
        report.push_str(&drop_rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", "));
        report.push('\n');
    }

    report
}

fn format_action(action: &toon_policy::Action) -> String {
    match action {
        toon_policy::Action::Keep => "Keep all".to_string(),
        toon_policy::Action::Drop => "Drop all".to_string(),
        toon_policy::Action::Sample(rate) => format!("Sample at {:.1}%", rate * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::{Action, Rule};

    #[test]
    fn generate_waggle_report_basic() {
        let mut policy = Policy::new("test-policy");
        policy.add_rule(
            Rule::new("keep-errors", "status >= 500", Action::Keep, 100)
                .with_description("Retain all HTTP 5xx errors for debugging")
        );
        policy.add_rule(Rule::new("sample-rest", "true", Action::Sample(0.01), 0));

        let report = generate_waggle_report(&policy);

        assert!(report.contains("# Waggle Report: test-policy"));
        assert!(report.contains("keep-errors"));
        assert!(report.contains("Retain all HTTP 5xx errors"));
        assert!(report.contains("Sample at 1.0%"));
    }
}
