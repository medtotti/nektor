//! Claude API client.

use crate::error::{Error, Result};
use crate::prompt::PromptBuilder;
use crate::response::MessageResponse;
use nectar_corpus::Corpus;
use serde::Serialize;
use toon_policy::Policy;
use tracing::{debug, info, warn};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 4096;

/// Claude API client for policy generation.
pub struct Client {
    api_key: String,
    http: reqwest::Client,
    model: String,
}

/// Configuration for the Claude client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Model to use (default: claude-sonnet-4-20250514).
    pub model: String,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            timeout_seconds: 120,
        }
    }
}

/// Request body for Claude API.
#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    system: Option<String>,
}

/// A message in the conversation.
#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

impl Client {
    /// Creates a new Claude client.
    ///
    /// # Errors
    ///
    /// Returns an error if the API key is empty.
    pub fn new(config: ClientConfig) -> Result<Self> {
        if config.api_key.is_empty() {
            return Err(Error::InvalidApiKey);
        }

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()?;

        Ok(Self {
            api_key: config.api_key,
            http,
            model: config.model,
        })
    }

    /// Generates a policy based on the given intent and corpus.
    ///
    /// # Arguments
    ///
    /// * `intent` - Natural language description of what the policy should do
    /// * `corpus` - Trace exemplars to inform the policy
    /// * `current_policy` - Optional existing policy to refine
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - API request fails
    /// - Response is not valid TOON
    /// - Parsed policy is invalid
    pub async fn generate_policy(
        &self,
        intent: &str,
        corpus: &Corpus,
        current_policy: Option<&Policy>,
    ) -> Result<Policy> {
        info!("Generating policy for intent: {}", intent);

        let prompt = PromptBuilder::new()
            .with_intent(intent)
            .with_corpus(corpus)
            .with_current_policy(current_policy)
            .build()?;

        debug!("Built prompt with {} chars", prompt.len());

        let response = self.call_api(&prompt).await?;
        Self::parse_policy_response(&response)
    }

    async fn call_api(&self, prompt: &str) -> Result<MessageResponse> {
        let system_prompt = r"You are a sampling policy expert for Honeycomb Refinery.
Your task is to generate TOON-formatted sampling policies based on user requirements.

IMPORTANT:
- Always output valid TOON format
- Include explicit array counts [N] that match actual items
- Include descriptions for all rules
- Never drop error traces (status >= 500)
- Always include a fallback rule (match: true)
- Order rules by priority (highest first)";

        let request = MessageRequest {
            model: self.model.clone(),
            max_tokens: MAX_TOKENS,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            system: Some(system_prompt.to_string()),
        };

        debug!("Sending request to Claude API");

        let response = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        debug!("Received response with status: {}", status);

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            return Err(Error::RateLimited {
                retry_after_seconds: retry_after,
            });
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::InvalidApiKey);
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::ApiError(format!(
                "API request failed with status {status}: {error_text}"
            )));
        }

        let msg_response: MessageResponse = response
            .json()
            .await
            .map_err(|e| Error::ParseError(format!("Failed to parse API response: {e}")))?;

        info!(
            "Received response: {} input tokens, {} output tokens",
            msg_response.usage.input_tokens, msg_response.usage.output_tokens
        );

        Ok(msg_response)
    }

    fn parse_policy_response(response: &MessageResponse) -> Result<Policy> {
        let toon = response
            .extract_toon()
            .ok_or_else(|| Error::ParseError("No TOON code block found in response".to_string()))?;

        debug!("Extracted TOON:\n{}", toon);

        let policy = toon_policy::parse(&toon).map_err(|e| {
            warn!("Failed to parse TOON: {}", e);
            Error::ToonValidationError(format!("Invalid TOON from Claude: {e}"))
        })?;

        // Validate basic invariants
        if !policy.has_fallback() {
            warn!("Generated policy lacks fallback rule");
            return Err(Error::ToonValidationError(
                "Generated policy lacks a fallback rule (match: true)".to_string(),
            ));
        }

        info!(
            "Successfully parsed policy with {} rules",
            policy.rules.len()
        );
        Ok(policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_rejects_empty_api_key() {
        let config = ClientConfig::default();
        assert!(Client::new(config).is_err());
    }

    #[test]
    fn client_accepts_valid_config() {
        let config = ClientConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        assert!(Client::new(config).is_ok());
    }

    #[test]
    fn parse_policy_from_response() {
        let response = MessageResponse {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            stop_reason: Some("end_turn".to_string()),
            content: vec![crate::response::ContentBlock::Text {
                text: r"Here's the policy:

```toon
nectar_policy{version,name,budget_per_second,rules}:
  1
  test-policy
  5000
  rules[2]{name,description,match,action,priority}:
    keep-errors,Keep all errors,http.status >= 500,keep,100
    sample-rest,Sample remaining traffic,true,sample(0.01),0
```
"
                .to_string(),
            }],
            usage: crate::response::Usage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };

        let policy = Client::parse_policy_response(&response).unwrap();
        assert_eq!(policy.name, "test-policy");
        assert_eq!(policy.rules.len(), 2);
        assert!(policy.has_fallback());
    }

    #[test]
    fn rejects_policy_without_fallback() {
        let response = MessageResponse {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            stop_reason: Some("end_turn".to_string()),
            content: vec![crate::response::ContentBlock::Text {
                text: r"```toon
nectar_policy{version,name,budget_per_second,rules}:
  1
  test-policy
  5000
  rules[1]{name,description,match,action,priority}:
    keep-errors,Keep all errors,http.status >= 500,keep,100
```
"
                .to_string(),
            }],
            usage: crate::response::Usage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };

        let result = Client::parse_policy_response(&response);
        assert!(matches!(result, Err(Error::ToonValidationError(_))));
    }
}
