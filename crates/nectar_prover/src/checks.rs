//! Individual verification checks.

use crate::result::{Violation, Warning};
use nectar_corpus::Corpus;
use toon_policy::Policy;

/// Checks that the policy has a fallback rule.
pub fn check_fallback(policy: &Policy) -> Option<Violation> {
    if policy.has_fallback() {
        None
    } else {
        Some(Violation::critical(
            "fallback-rule",
            "Policy has no fallback rule (match: true). Unmatched traces will be dropped.",
        ))
    }
}

/// Checks that error traces are never dropped.
pub fn check_error_handling(policy: &Policy) -> Option<Violation> {
    for rule in &policy.rules {
        // Check if any rule could drop errors
        if matches!(rule.action, toon_policy::Action::Drop) {
            // This is a simplified check - real implementation would parse match_expr
            if rule.match_expr.contains("status") || rule.match_expr == "true" {
                return Some(Violation::critical(
                    "error-handling",
                    format!(
                        "Rule '{}' could drop error traces. Errors must always be kept.",
                        rule.name
                    ),
                ));
            }
        }
    }
    None
}

/// Checks for high-cardinality grouping keys.
#[allow(clippy::missing_const_for_fn)] // TODO: implement when adding actual logic
pub fn check_cardinality(_policy: &Policy, _corpus: &Corpus) -> Option<Warning> {
    // TODO: Implement cardinality check
    // Would analyze corpus to find high-cardinality fields
    None
}

/// Simulates policy against corpus and checks must-keep traces.
///
/// # Errors
///
/// Returns a `Violation` if the policy may drop error traces.
pub fn check_must_keep_coverage(
    policy: &Policy,
    corpus: &Corpus,
) -> std::result::Result<(), Violation> {
    let errors = corpus.errors();

    if errors.is_empty() {
        return Ok(());
    }

    // TODO: Implement actual policy evaluation
    // For now, just check that we have keep rules for errors
    let has_error_keep_rule = policy.rules.iter().any(|r| {
        matches!(r.action, toon_policy::Action::Keep)
            && (r.match_expr.contains("error") || r.match_expr.contains("status >= 500"))
    });

    if has_error_keep_rule {
        Ok(())
    } else {
        Err(Violation::critical(
            "must-keep-coverage",
            format!(
                "Policy may drop {} error traces. Add a rule to keep errors.",
                errors.len()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::{Action, Rule};

    #[test]
    fn check_fallback_passes_with_fallback() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.01), 0));
        assert!(check_fallback(&policy).is_none());
    }

    #[test]
    fn check_fallback_fails_without_fallback() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("errors", "error", Action::Keep, 100));
        assert!(check_fallback(&policy).is_some());
    }

    #[test]
    fn check_error_handling_rejects_drop_all() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("drop-all", "true", Action::Drop, 0));
        assert!(check_error_handling(&policy).is_some());
    }
}
