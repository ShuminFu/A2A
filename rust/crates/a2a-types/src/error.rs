//! The A2A error model and its JSON-RPC code mapping (specification sections 3.3.2 and 5.4).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An A2A protocol error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum A2aError {
    #[error("Invalid JSON payload")]
    JsonParse,
    #[error("Request payload validation error: {0}")]
    InvalidRequest(String),
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Task cannot be canceled: {0}")]
    TaskNotCancelable(String),
    #[error("Push notifications are not supported by this agent")]
    PushNotificationNotSupported,
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
    #[error("Content type not supported: {0}")]
    ContentTypeNotSupported(String),
    #[error("Invalid agent response: {0}")]
    InvalidAgentResponse(String),
    #[error("Extended agent card is not configured")]
    ExtendedAgentCardNotConfigured,
    #[error("Extension support required: {0}")]
    ExtensionSupportRequired(String),
    #[error("Protocol version not supported: {0}")]
    VersionNotSupported(String),
}

impl A2aError {
    /// The JSON-RPC error code for this error.
    pub fn code(&self) -> i32 {
        match self {
            A2aError::JsonParse => -32700,
            A2aError::InvalidRequest(_) => -32600,
            A2aError::MethodNotFound(_) => -32601,
            A2aError::InvalidParams(_) => -32602,
            A2aError::Internal(_) => -32603,
            A2aError::TaskNotFound(_) => -32001,
            A2aError::TaskNotCancelable(_) => -32002,
            A2aError::PushNotificationNotSupported => -32003,
            A2aError::UnsupportedOperation(_) => -32004,
            A2aError::ContentTypeNotSupported(_) => -32005,
            A2aError::InvalidAgentResponse(_) => -32006,
            A2aError::ExtendedAgentCardNotConfigured => -32007,
            A2aError::ExtensionSupportRequired(_) => -32008,
            A2aError::VersionNotSupported(_) => -32009,
        }
    }

    /// The canonical error name, used as the `reason` of the `google.rpc.ErrorInfo` detail.
    pub fn reason(&self) -> &'static str {
        match self {
            A2aError::JsonParse => "JSONParseError",
            A2aError::InvalidRequest(_) => "InvalidRequestError",
            A2aError::MethodNotFound(_) => "MethodNotFoundError",
            A2aError::InvalidParams(_) => "InvalidParamsError",
            A2aError::Internal(_) => "InternalError",
            A2aError::TaskNotFound(_) => "TaskNotFoundError",
            A2aError::TaskNotCancelable(_) => "TaskNotCancelableError",
            A2aError::PushNotificationNotSupported => "PushNotificationNotSupportedError",
            A2aError::UnsupportedOperation(_) => "UnsupportedOperationError",
            A2aError::ContentTypeNotSupported(_) => "ContentTypeNotSupportedError",
            A2aError::InvalidAgentResponse(_) => "InvalidAgentResponseError",
            A2aError::ExtendedAgentCardNotConfigured => "ExtendedAgentCardNotConfiguredError",
            A2aError::ExtensionSupportRequired(_) => "ExtensionSupportRequiredError",
            A2aError::VersionNotSupported(_) => "VersionNotSupportedError",
        }
    }

    /// The JSON-RPC error object for this error, with a `google.rpc.ErrorInfo` detail.
    pub fn to_jsonrpc_error(&self) -> crate::jsonrpc::JsonRpcError {
        crate::jsonrpc::JsonRpcError {
            code: self.code(),
            message: self.to_string(),
            data: Some(vec![serde_json::json!({
                "@type": "google.rpc.ErrorInfo",
                "reason": self.reason(),
                "domain": "a2a-protocol.org",
            })]),
        }
    }

    /// Rebuilds an error from a JSON-RPC code and message, for clients reading error responses.
    pub fn from_code(code: i32, message: String) -> Self {
        match code {
            -32700 => A2aError::JsonParse,
            -32600 => A2aError::InvalidRequest(message),
            -32601 => A2aError::MethodNotFound(message),
            -32602 => A2aError::InvalidParams(message),
            -32001 => A2aError::TaskNotFound(message),
            -32002 => A2aError::TaskNotCancelable(message),
            -32003 => A2aError::PushNotificationNotSupported,
            -32004 => A2aError::UnsupportedOperation(message),
            -32005 => A2aError::ContentTypeNotSupported(message),
            -32006 => A2aError::InvalidAgentResponse(message),
            -32007 => A2aError::ExtendedAgentCardNotConfigured,
            -32008 => A2aError::ExtensionSupportRequired(message),
            -32009 => A2aError::VersionNotSupported(message),
            _ => A2aError::Internal(message),
        }
    }
}

/// A `google.rpc.ErrorInfo` detail object, as carried in the `data` array of an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorInfo {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub reason: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}
