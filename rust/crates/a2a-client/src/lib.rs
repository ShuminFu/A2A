//! A client for the A2A protocol v1.0 JSON-RPC binding.
//!
//! Start from an Agent Card: it names the endpoint, the protocol binding and the version, and
//! may pin a tenant that every request has to echo.
//!
//! ```no_run
//! use a2a_client::A2aClient;
//! use a2a_types::{Message, SendMessageRequest};
//!
//! # async fn call() -> Result<(), Box<dyn std::error::Error>> {
//! let client = A2aClient::discover("http://localhost:9999").await?;
//! let message = Message::user_text(uuid_like(), "hello");
//! let response = client.send_message(SendMessageRequest::new(message)).await?;
//! # Ok(())
//! # }
//! # fn uuid_like() -> String { "message-1".to_string() }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod sse;

use std::sync::atomic::{AtomicU64, Ordering};

use a2a_types::{
    card::protocol_binding, method, AgentCard, CancelTaskRequest,
    DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, JsonRpcRequest, JsonRpcResponse,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse, StreamResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig, AGENT_CARD_WELL_KNOWN_PATH,
    PROTOCOL_VERSION,
};
use futures_util::Stream;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub use error::{ClientError, Result};

/// The header naming the protocol version a client speaks.
pub const VERSION_HEADER: &str = "A2A-Version";

/// A client bound to one agent interface.
#[derive(Debug, Clone)]
pub struct A2aClient {
    http: reqwest::Client,
    endpoint: String,
    tenant: Option<String>,
    card: Option<AgentCard>,
    next_id: std::sync::Arc<AtomicU64>,
}

impl A2aClient {
    /// Fetches `{base_url}/.well-known/agent-card.json` and binds to the agent's preferred
    /// JSON-RPC interface.
    pub async fn discover(base_url: &str) -> Result<Self> {
        let url = format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            AGENT_CARD_WELL_KNOWN_PATH
        );
        Self::from_card_url(&url).await
    }

    /// Fetches an Agent Card from an explicit URL and binds to its JSON-RPC interface.
    pub async fn from_card_url(card_url: &str) -> Result<Self> {
        let http = reqwest::Client::new();
        let card: AgentCard = http.get(card_url).send().await?.json().await?;
        Self::from_card(card)
    }

    /// Binds to the JSON-RPC interface of a card already in hand.
    pub fn from_card(card: AgentCard) -> Result<Self> {
        let interface = card
            .preferred_supported_interface()
            .ok_or_else(|| {
                ClientError::NoCompatibleInterface(protocol_binding::JSONRPC.to_string())
            })?
            .clone();
        Ok(A2aClient {
            http: reqwest::Client::new(),
            endpoint: interface.url,
            tenant: interface.tenant,
            card: Some(card),
            next_id: std::sync::Arc::new(AtomicU64::new(1)),
        })
    }

    /// Binds directly to a JSON-RPC endpoint, skipping discovery.
    pub fn from_endpoint(endpoint: impl Into<String>) -> Self {
        A2aClient {
            http: reqwest::Client::new(),
            endpoint: endpoint.into(),
            tenant: None,
            card: None,
            next_id: std::sync::Arc::new(AtomicU64::new(1)),
        }
    }

    /// The card this client discovered, when it discovered one.
    pub fn agent_card(&self) -> Option<&AgentCard> {
        self.card.as_ref()
    }

    /// The endpoint this client calls.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// `SendMessage`: send a message and wait for the task to settle.
    pub async fn send_message(
        &self,
        mut request: SendMessageRequest,
    ) -> Result<SendMessageResponse> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(method::SEND_MESSAGE, to_params(&request)?).await
    }

    /// `SendStreamingMessage`: send a message and read task updates as they happen.
    pub async fn send_streaming_message(
        &self,
        mut request: SendMessageRequest,
    ) -> Result<impl Stream<Item = Result<StreamResponse>>> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.stream(method::SEND_STREAMING_MESSAGE, to_params(&request)?)
            .await
    }

    /// `GetTask`.
    pub async fn get_task(&self, mut request: GetTaskRequest) -> Result<Task> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(method::GET_TASK, to_params(&request)?).await
    }

    /// `ListTasks`.
    pub async fn list_tasks(&self, mut request: ListTasksRequest) -> Result<ListTasksResponse> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(method::LIST_TASKS, to_params(&request)?).await
    }

    /// `CancelTask`.
    pub async fn cancel_task(&self, mut request: CancelTaskRequest) -> Result<Task> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(method::CANCEL_TASK, to_params(&request)?).await
    }

    /// `SubscribeToTask`: attach to a running task's stream.
    pub async fn subscribe_to_task(
        &self,
        mut request: SubscribeToTaskRequest,
    ) -> Result<impl Stream<Item = Result<StreamResponse>>> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.stream(method::SUBSCRIBE_TO_TASK, to_params(&request)?)
            .await
    }

    /// `CreateTaskPushNotificationConfig`.
    pub async fn create_push_notification_config(
        &self,
        mut config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig> {
        config.tenant = config.tenant.or_else(|| self.tenant.clone());
        self.call(
            method::CREATE_TASK_PUSH_NOTIFICATION_CONFIG,
            to_params(&config)?,
        )
        .await
    }

    /// `GetTaskPushNotificationConfig`.
    pub async fn get_push_notification_config(
        &self,
        mut request: GetTaskPushNotificationConfigRequest,
    ) -> Result<TaskPushNotificationConfig> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(
            method::GET_TASK_PUSH_NOTIFICATION_CONFIG,
            to_params(&request)?,
        )
        .await
    }

    /// `ListTaskPushNotificationConfigs`.
    pub async fn list_push_notification_configs(
        &self,
        mut request: ListTaskPushNotificationConfigsRequest,
    ) -> Result<ListTaskPushNotificationConfigsResponse> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        self.call(
            method::LIST_TASK_PUSH_NOTIFICATION_CONFIGS,
            to_params(&request)?,
        )
        .await
    }

    /// `DeleteTaskPushNotificationConfig`.
    pub async fn delete_push_notification_config(
        &self,
        mut request: DeleteTaskPushNotificationConfigRequest,
    ) -> Result<()> {
        request.tenant = request.tenant.or_else(|| self.tenant.clone());
        let _: Value = self
            .call(
                method::DELETE_TASK_PUSH_NOTIFICATION_CONFIG,
                to_params(&request)?,
            )
            .await?;
        Ok(())
    }

    /// `GetExtendedAgentCard`: the fuller card an authenticated client may see.
    pub async fn get_extended_agent_card(&self) -> Result<AgentCard> {
        let request = GetExtendedAgentCardRequest {
            tenant: self.tenant.clone(),
        };
        self.call(method::GET_EXTENDED_AGENT_CARD, to_params(&request)?)
            .await
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest::new(json!(id), method, Some(params));
        let response: JsonRpcResponse = self
            .http
            .post(&self.endpoint)
            .header(VERSION_HEADER, PROTOCOL_VERSION)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(error) = response.error {
            return Err(ClientError::Protocol(a2a_types::A2aError::from_code(
                error.code,
                error.message,
            )));
        }
        Ok(serde_json::from_value(
            response.result.unwrap_or(Value::Null),
        )?)
    }

    async fn stream(
        &self,
        method: &str,
        params: Value,
    ) -> Result<impl Stream<Item = Result<StreamResponse>>> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest::new(json!(id), method, Some(params));
        let response = self
            .http
            .post(&self.endpoint)
            .header(VERSION_HEADER, PROTOCOL_VERSION)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&request)
            .send()
            .await?;

        // A method that fails before streaming starts answers with a plain JSON-RPC error.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !content_type.starts_with("text/event-stream") {
            let body: JsonRpcResponse = response.json().await?;
            if let Some(error) = body.error {
                return Err(ClientError::Protocol(a2a_types::A2aError::from_code(
                    error.code,
                    error.message,
                )));
            }
            return Err(ClientError::Decode(format!(
                "expected an event stream, got {content_type}"
            )));
        }

        Ok(sse::read_events(response))
    }
}

fn to_params<T: serde::Serialize>(value: &T) -> Result<Value> {
    Ok(serde_json::to_value(value)?)
}
