//! Client-side errors.

use a2a_types::A2aError;

/// Everything that can go wrong calling an A2A agent.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The agent answered with a protocol error.
    #[error(transparent)]
    Protocol(#[from] A2aError),
    /// The request never completed: DNS, TLS, connection or timeout.
    #[error("transport error: {0}")]
    Transport(String),
    /// The response was not the shape this client expected.
    #[error("could not decode response: {0}")]
    Decode(String),
    /// The Agent Card offers no interface this client can speak.
    #[error("agent card has no interface with the {0} protocol binding")]
    NoCompatibleInterface(String),
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        ClientError::Transport(error.to_string())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        ClientError::Decode(error.to_string())
    }
}

/// A client result.
pub type Result<T> = std::result::Result<T, ClientError>;
