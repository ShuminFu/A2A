//! Push notification configuration.
//!
//! v1.0 merged `PushNotificationConfig` into `TaskPushNotificationConfig` (#1500), so a single
//! flat object carries both the routing identity and the delivery details.

use serde::{Deserialize, Serialize};

/// Credentials the agent uses when calling a push notification URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationInfo {
    /// An HTTP authentication scheme from the IANA registry, e.g. `Bearer`.
    pub scheme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

/// A webhook the agent posts task updates to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPushNotificationConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Server-assigned id for this configuration. Empty when creating one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The task this configuration belongs to. Empty when sent inside a `SendMessage` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationInfo>,
}

impl TaskPushNotificationConfig {
    /// A configuration delivering to `url` with no authentication.
    pub fn new(url: impl Into<String>) -> Self {
        TaskPushNotificationConfig {
            tenant: None,
            id: None,
            task_id: None,
            url: url.into(),
            token: None,
            authentication: None,
        }
    }

    /// Sets the validation token the agent echoes back in the `X-A2A-Notification-Token` header.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }
}
