use thiserror::Error;

/// Errors that can occur while executing or validating project commands.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("invalid command: {0}")]
    Invalid(&'static str),
    #[error("invariant violated: {0}")]
    InvariantViolation(String),
}

impl CommandError {
    pub fn invariant(message: impl Into<String>) -> Self {
        CommandError::InvariantViolation(message.into())
    }
}
