//! Property-based generators for synthetic test data.
//!
//! Uses proptest strategies to generate:
//! - Trace exemplars with realistic distributions
//! - Policy rules with valid syntax
//! - Match expressions with various operators
//! - Corpus configurations

use proptest::prelude::*;
use toon_policy::{Action, Policy, Rule};

/// Strategy for generating valid service names.
///
/// # Panics
///
/// Panics if the internal regex is invalid (should never happen).
pub fn service_name() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-z][a-z0-9-]{2,20}").expect("valid regex")
}

/// Strategy for generating HTTP status codes with realistic distribution.
pub fn http_status() -> impl Strategy<Value = u16> {
    prop_oneof![
        8 => 200..=204u16,      // Success (80%)
        1 => 400..=404u16,      // Client errors (10%)
        1 => 500..=504u16,      // Server errors (10%)
    ]
}

/// Strategy for generating durations in milliseconds.
pub fn duration_ms() -> impl Strategy<Value = u64> {
    prop_oneof![
        7 => 1..100u64,         // Fast (70%)
        2 => 100..1000u64,      // Normal (20%)
        1 => 1000..30000u64,    // Slow (10%)
    ]
}

/// Strategy for generating HTTP routes.
///
/// # Panics
///
/// Panics if the internal regex is invalid (should never happen).
pub fn http_route() -> impl Strategy<Value = String> {
    prop::string::string_regex("/api/v[12]/[a-z]+(/[a-z]+)?").expect("valid regex")
}

/// Strategy for generating trace attributes.
#[derive(Debug, Clone)]
pub struct TraceAttrs {
    /// Service name.
    pub service_name: String,
    /// HTTP status code.
    pub http_status: u16,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// HTTP route.
    pub http_route: String,
    /// Whether the trace represents an error.
    pub has_error: bool,
}

impl Arbitrary for TraceAttrs {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (service_name(), http_status(), duration_ms(), http_route())
            .prop_map(|(service_name, http_status, duration_ms, http_route)| {
                let has_error = http_status >= 500;
                Self {
                    service_name,
                    http_status,
                    duration_ms,
                    http_route,
                    has_error,
                }
            })
            .boxed()
    }
}

/// Strategy for generating valid match expressions.
pub fn match_expr() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("true".to_string()),
        http_status().prop_map(|s| format!("http.status >= {s}")),
        duration_ms().prop_map(|d| format!("duration > {d}ms")),
        service_name().prop_map(|s| format!("service.name == \"{s}\"")),
        (http_status(), any::<bool>()).prop_map(|(s, err)| {
            if err {
                format!("http.status >= {s} || error == true")
            } else {
                format!("http.status >= {s}")
            }
        }),
    ]
}

/// Strategy for generating valid actions.
pub fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        3 => Just(Action::Keep),
        1 => Just(Action::Drop),
        6 => (0.001f64..1.0).prop_map(Action::Sample),
    ]
}

/// Strategy for generating valid priorities (u8).
pub fn priority() -> impl Strategy<Value = u8> {
    0u8..=100
}

/// Strategy for generating valid rules.
pub fn rule() -> impl Strategy<Value = Rule> {
    ("[a-z][a-z0-9-]{2,15}", match_expr(), action(), priority()).prop_map(
        |(name, match_expr, action, priority)| Rule::new(name, match_expr, action, priority),
    )
}

/// Strategy for generating valid policies.
pub fn policy() -> impl Strategy<Value = Policy> {
    (service_name(), prop::collection::vec(rule(), 1..10)).prop_map(|(name, mut rules)| {
        // Ensure fallback rule exists
        let has_fallback = rules.iter().any(|r| r.match_expr == "true");
        if !has_fallback {
            rules.push(Rule::new("fallback", "true", Action::Sample(0.01), 0));
        }
        let mut policy = Policy::new(name);
        for rule in rules {
            policy.add_rule(rule);
        }
        policy
    })
}

/// Strategy for generating policies that may be invalid (for negative testing).
pub fn possibly_invalid_policy() -> impl Strategy<Value = Policy> {
    prop_oneof![
        // Valid policy
        8 => policy(),
        // Policy without fallback
        1 => (service_name(), prop::collection::vec(rule(), 1..5))
            .prop_map(|(name, rules)| {
                let mut policy = Policy::new(name);
                for rule in rules {
                    if rule.match_expr != "true" {
                        policy.add_rule(rule);
                    }
                }
                // Add at least one non-fallback rule
                if policy.rules.is_empty() {
                    policy.add_rule(Rule::new("errors", "http.status >= 500", Action::Keep, 100));
                }
                policy
            }),
        // Empty policy
        1 => service_name().prop_map(Policy::new),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn generated_policies_are_valid(policy in policy()) {
            // All generated policies should have at least one rule
            prop_assert!(!policy.rules.is_empty());
            // All generated policies should have a fallback
            prop_assert!(policy.has_fallback());
        }

        #[test]
        fn generated_trace_attrs_are_consistent(attrs in any::<TraceAttrs>()) {
            // Error flag should match status code
            prop_assert_eq!(attrs.has_error, attrs.http_status >= 500);
        }

        #[test]
        fn generated_match_exprs_are_parseable(expr in match_expr()) {
            // All generated match expressions should parse
            let result = nectar_compiler::match_expr::MatchExpr::parse(&expr);
            prop_assert!(result.is_ok(), "Failed to parse: {}", expr);
        }
    }
}
