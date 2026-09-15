//! Request and response objects for the eleven `A2AService` operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::card::AgentCard;
use crate::core::{
    Message, Metadata, Task, TaskArtifactUpdateEvent, TaskState, TaskStatusUpdateEvent,
};
use crate::push::TaskPushNotificationConfig;

/// Per-request tuning for `SendMessage` and `SendStreamingMessage`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageConfiguration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_output_modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_push_notification_config: Option<TaskPushNotificationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// When `true` the call returns as soon as the task exists, without waiting for a
    /// terminal or interrupted state.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub return_immediately: bool,
}

/// Parameters for `SendMessage` and `SendStreamingMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub message: Message,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<SendMessageConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl SendMessageRequest {
    /// A request carrying `message` with no configuration.
    pub fn new(message: Message) -> Self {
        SendMessageRequest {
            tenant: None,
            message,
            configuration: None,
            metadata: None,
        }
    }

    /// Replaces the request configuration.
    pub fn with_configuration(mut self, configuration: SendMessageConfiguration) -> Self {
        self.configuration = Some(configuration);
        self
    }
}

/// The result of `SendMessage`: either a task, or a direct message for trivial exchanges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SendMessageResponse {
    Task(Task),
    Message(Message),
}

/// One event in a `SendStreamingMessage` or `SubscribeToTask` stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamResponse {
    Task(Task),
    Message(Message),
    StatusUpdate(TaskStatusUpdateEvent),
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

impl StreamResponse {
    /// The task state this event reports, when it reports one.
    pub fn task_state(&self) -> Option<TaskState> {
        match self {
            StreamResponse::Task(task) => Some(task.status.state),
            StreamResponse::StatusUpdate(event) => Some(event.status.state),
            _ => None,
        }
    }

    /// `true` when this event ends the stream: a terminal or interrupted state.
    pub fn is_final(&self) -> bool {
        self.task_state()
            .is_some_and(TaskState::is_final_for_blocking_call)
    }
}

/// Parameters for `GetTask`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
}

impl GetTaskRequest {
    /// Fetches task `id` with its full history.
    pub fn new(id: impl Into<String>) -> Self {
        GetTaskRequest {
            tenant: None,
            id: id.into(),
            history_length: None,
        }
    }
}

/// Parameters for `ListTasks`, new in v1.0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_timestamp_after: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_artifacts: Option<bool>,
}

/// The result of `ListTasks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksResponse {
    pub tasks: Vec<Task>,
    pub next_page_token: String,
    pub page_size: i32,
    pub total_size: i32,
}

/// Parameters for `CancelTask`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTaskRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl CancelTaskRequest {
    /// Cancels task `id`.
    pub fn new(id: impl Into<String>) -> Self {
        CancelTaskRequest {
            tenant: None,
            id: id.into(),
            metadata: None,
        }
    }
}

/// Parameters for `SubscribeToTask`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeToTaskRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub id: String,
}

impl SubscribeToTaskRequest {
    /// Subscribes to task `id`.
    pub fn new(id: impl Into<String>) -> Self {
        SubscribeToTaskRequest {
            tenant: None,
            id: id.into(),
        }
    }
}

/// Parameters for `GetTaskPushNotificationConfig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskPushNotificationConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub task_id: String,
    pub id: String,
}

/// Parameters for `DeleteTaskPushNotificationConfig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTaskPushNotificationConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub task_id: String,
    pub id: String,
}

/// Parameters for `ListTaskPushNotificationConfigs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskPushNotificationConfigsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// The result of `ListTaskPushNotificationConfigs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskPushNotificationConfigsResponse {
    #[serde(default)]
    pub configs: Vec<TaskPushNotificationConfig>,
    #[serde(default)]
    pub next_page_token: String,
}

/// Parameters for `GetExtendedAgentCard`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetExtendedAgentCardRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

/// The result of `GetExtendedAgentCard`.
pub type GetExtendedAgentCardResponse = AgentCard;

/// The JSON-RPC method names, which in v1.0 are the PascalCase RPC names from the proto
/// rather than the `tasks/send` style of v0.x (specification section 9.1).
pub mod method {
    pub const SEND_MESSAGE: &str = "SendMessage";
    pub const SEND_STREAMING_MESSAGE: &str = "SendStreamingMessage";
    pub const GET_TASK: &str = "GetTask";
    pub const LIST_TASKS: &str = "ListTasks";
    pub const CANCEL_TASK: &str = "CancelTask";
    pub const SUBSCRIBE_TO_TASK: &str = "SubscribeToTask";
    pub const CREATE_TASK_PUSH_NOTIFICATION_CONFIG: &str = "CreateTaskPushNotificationConfig";
    pub const GET_TASK_PUSH_NOTIFICATION_CONFIG: &str = "GetTaskPushNotificationConfig";
    pub const LIST_TASK_PUSH_NOTIFICATION_CONFIGS: &str = "ListTaskPushNotificationConfigs";
    pub const DELETE_TASK_PUSH_NOTIFICATION_CONFIG: &str = "DeleteTaskPushNotificationConfig";
    pub const GET_EXTENDED_AGENT_CARD: &str = "GetExtendedAgentCard";
}
