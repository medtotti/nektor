//! Prompt building for Claude policy generation.

use crate::error::Result;
use nectar_corpus::Corpus;
use toon_policy::Policy;

/// Builds prompts for Claude policy generation.
#[derive(Default)]
pub struct PromptBuilder<'a> {
    intent: Option<&'a str>,
    corpus: Option<&'a Corpus>,
    current_policy: Option<&'a Policy>,
}

impl<'a> PromptBuilder<'a> {
    /// Creates a new prompt builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the user intent.
    #[must_use]
    pub const fn with_intent(mut self, intent: &'a str) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Sets the trace corpus.
    #[must_use]
    pub const fn with_corpus(mut self, corpus: &'a Corpus) -> Self {
        self.corpus = Some(corpus);
        self
    }

    /// Sets the current policy (for refinement).
    #[must_use]
    pub const fn with_current_policy(mut self, policy: Option<&'a Policy>) -> Self {
        self.current_policy = policy;
        self
    }

    /// Builds the prompt string.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<String> {
        let intent = self.intent.unwrap_or("Generate a sampling policy");

        let corpus_section = if let Some(corpus) = self.corpus {
            let toon = corpus.encode_toon()?;
            format!(
                "## Trace Corpus\n\nHere are representative traces from the system:\n\n```toon\n{toon}\n```\n\n"
            )
        } else {
            String::new()
        };

        let current_policy_section = self.current_policy.map_or_else(String::new, |policy| {
            let toon = toon_policy::serialize(policy);
            format!(
                "## Current Policy\n\nHere is the existing policy to refine:\n\n```toon\n{toon}```\n\n"
            )
        });

        Ok(format!(
            r"# Policy Generation Request

## Intent

{intent}

{corpus_section}{current_policy_section}## Instructions

Generate a Nectar sampling policy in TOON format that achieves the stated intent.

Requirements:
1. Use TOON format with explicit counts and field headers
2. Include a description for each rule
3. Ensure a fallback rule exists (match: true)
4. Order rules by priority (highest first)
5. Never drop error traces (status >= 500)

Output only the TOON code block, nothing else.

```toon
nectar_policy{{version,name,budget_per_second,rules}}:
  1
  <policy-name>
  <budget>
  rules[N]{{name,description,match,action,priority}}:
    <rules>
```
"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_minimal_prompt() {
        let prompt = PromptBuilder::new()
            .with_intent("Keep all errors")
            .build()
            .unwrap();

        assert!(prompt.contains("Keep all errors"));
        assert!(prompt.contains("nectar_policy"));
    }

    #[test]
    fn build_prompt_with_corpus() {
        let corpus = Corpus::new();
        let prompt = PromptBuilder::new()
            .with_intent("Sample at 1%")
            .with_corpus(&corpus)
            .build()
            .unwrap();

        assert!(prompt.contains("Trace Corpus"));
    }
}
