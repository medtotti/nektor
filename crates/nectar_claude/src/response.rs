//! Claude API response handling.

use serde::{Deserialize, Serialize};

/// Claude API message response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    /// Response ID.
    pub id: String,
    /// Model used.
    pub model: String,
    /// Stop reason.
    pub stop_reason: Option<String>,
    /// Content blocks.
    pub content: Vec<ContentBlock>,
    /// Usage statistics.
    pub usage: Usage,
}

/// Content block in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of input tokens consumed.
    pub input_tokens: u64,
    /// Number of output tokens generated.
    pub output_tokens: u64,
}

impl MessageResponse {
    /// Extracts the text content from the response.
    #[must_use]
    pub fn text(&self) -> String {
        self.content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extracts TOON code blocks from the response.
    #[must_use]
    pub fn extract_toon(&self) -> Option<String> {
        let text = self.text();
        
        // Look for ```toon ... ``` blocks
        if let Some(start) = text.find("```toon") {
            let content_start = start + 7;
            if let Some(end) = text[content_start..].find("```") {
                let toon = text[content_start..content_start + end].trim();
                return Some(toon.to_string());
            }
        }
        
        // Fallback: look for ``` ... ``` blocks
        if let Some(start) = text.find("```") {
            let content_start = start + 3;
            // Skip language identifier if present
            let content_start = text[content_start..]
                .find('\n')
                .map_or(content_start, |n| content_start + n + 1);
            if let Some(end) = text[content_start..].find("```") {
                let toon = text[content_start..content_start + end].trim();
                return Some(toon.to_string());
            }
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_toon_from_response() {
        let response = MessageResponse {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            stop_reason: Some("end_turn".to_string()),
            content: vec![ContentBlock::Text {
                text: "Here's the policy:\n\n```toon\npolicy[1]{name}:\n  test\n```\n".to_string(),
            }],
            usage: Usage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };

        let toon = response.extract_toon().unwrap();
        assert!(toon.contains("policy[1]"));
    }
}
