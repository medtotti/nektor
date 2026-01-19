//! TOON encoding utilities.

use crate::error::Result;

/// Encodes a value to TOON format.
pub trait ToonEncode {
    /// Encodes this value to TOON.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    fn encode_toon(&self) -> Result<String>;
}

/// Escapes a string value for TOON format.
///
/// Handles commas, newlines, and other special characters.
#[must_use]
pub fn escape_value(s: &str) -> String {
    if s.contains(',') || s.contains('\n') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_simple_value() {
        assert_eq!(escape_value("hello"), "hello");
    }

    #[test]
    fn escape_value_with_comma() {
        assert_eq!(escape_value("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn escape_value_with_quote() {
        assert_eq!(escape_value("say \"hi\""), "\"say \\\"hi\\\"\"");
    }
}
