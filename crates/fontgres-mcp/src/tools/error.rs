//! The one error type MCP tool bodies return.

use std::fmt;

use rmcp::model::{ContentBlock, IntoContents};

/// A tool-call failure, holding the message the model sees.
///
/// It renders as one text block. rmcp's `IntoCallToolResult` for
/// `Result<T, E>` flags the result `is_error: true`, exactly as it does for an
/// `Err(String)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolError(String);

/// A message a tool or helper words itself, such as an input error, used
/// verbatim.
impl From<String> for ToolError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl IntoContents for ToolError {
    fn into_contents(self) -> Vec<ContentBlock> {
        vec![ContentBlock::text(self.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::ToolError;

    #[test]
    fn input_message_is_used_verbatim() {
        assert_eq!(
            ToolError::from("name must not be empty".to_string()).to_string(),
            "name must not be empty"
        );
    }
}
