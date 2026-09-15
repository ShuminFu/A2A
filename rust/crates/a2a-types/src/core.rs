//! Core protocol objects: messages, parts, tasks and streaming events.
//!
//! Field names follow the JSON serialization rules in specification section 5.5:
//! camelCase fields, ProtoJSON enum names.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A key/value object carried alongside most protocol objects (`google.protobuf.Struct`).
pub type Metadata = Map<String, Value>;

/// Identifies the sender of a [`Message`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// The proto zero value.
    #[default]
    #[serde(rename = "ROLE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "ROLE_USER")]
    User,
    #[serde(rename = "ROLE_AGENT")]
    Agent,
}

/// The lifecycle states of a [`Task`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// The proto zero value.
    #[default]
    #[serde(rename = "TASK_STATE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "TASK_STATE_SUBMITTED")]
    Submitted,
    #[serde(rename = "TASK_STATE_WORKING")]
    Working,
    #[serde(rename = "TASK_STATE_COMPLETED")]
    Completed,
    #[serde(rename = "TASK_STATE_FAILED")]
    Failed,
    #[serde(rename = "TASK_STATE_CANCELED")]
    Canceled,
    #[serde(rename = "TASK_STATE_INPUT_REQUIRED")]
    InputRequired,
    #[serde(rename = "TASK_STATE_REJECTED")]
    Rejected,
    #[serde(rename = "TASK_STATE_AUTH_REQUIRED")]
    AuthRequired,
}

impl TaskState {
    /// `true` for states a task can never leave: completed, failed, canceled, rejected.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        )
    }

    /// `true` for states that pause a task pending client action, without ending it.
    pub fn is_interrupted(self) -> bool {
        matches!(self, TaskState::InputRequired | TaskState::AuthRequired)
    }

    /// `true` when a blocking `SendMessage` call may return: terminal or interrupted.
    pub fn is_final_for_blocking_call(self) -> bool {
        self.is_terminal() || self.is_interrupted()
    }
}

/// The content of a [`Part`]: the proto `oneof content`, flattened into the part object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartContent {
    /// Plain text content.
    Text(String),
    /// Raw file bytes, base64-encoded in JSON.
    Raw(String),
    /// A URL pointing at the file's content.
    Url(String),
    /// Arbitrary structured JSON.
    Data(Value),
}

/// A section of communication content: text, a file, or structured data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(flatten)]
    pub content: PartContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

impl Part {
    /// A `text/plain` text part.
    pub fn text(text: impl Into<String>) -> Self {
        Part {
            content: PartContent::Text(text.into()),
            metadata: None,
            filename: None,
            media_type: None,
        }
    }

    /// A structured data part.
    pub fn data(value: Value) -> Self {
        Part {
            content: PartContent::Data(value),
            metadata: None,
            filename: None,
            media_type: Some("application/json".to_string()),
        }
    }

    /// A file part referenced by URL.
    pub fn file_url(url: impl Into<String>, media_type: impl Into<String>) -> Self {
        Part {
            content: PartContent::Url(url.into()),
            metadata: None,
            filename: None,
            media_type: Some(media_type.into()),
        }
    }

    /// The text of this part, if it is a text part.
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            PartContent::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// One unit of communication between a client and an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_task_ids: Vec<String>,
}

impl Message {
    /// A user message carrying a single text part, with a generated message id.
    pub fn user_text(message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Message {
            message_id: message_id.into(),
            context_id: None,
            task_id: None,
            role: Role::User,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        }
    }

    /// An agent message carrying a single text part.
    pub fn agent_text(message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Message {
            message_id: message_id.into(),
            context_id: None,
            task_id: None,
            role: Role::Agent,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        }
    }

    /// All text parts joined with a space; empty when the message has no text.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(Part::as_text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The status of a [`Task`] at a point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl TaskStatus {
    /// A status in `state`, stamped with the current time.
    pub fn now(state: TaskState) -> Self {
        TaskStatus {
            state,
            message: None,
            timestamp: Some(Utc::now()),
        }
    }

    /// A status in `state` carrying an agent message, stamped with the current time.
    pub fn now_with_message(state: TaskState, message: Message) -> Self {
        TaskStatus {
            state,
            message: Some(message),
            timestamp: Some(Utc::now()),
        }
    }
}

/// A task output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

impl Artifact {
    /// An artifact with a single text part.
    pub fn text(
        artifact_id: impl Into<String>,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Artifact {
            artifact_id: artifact_id.into(),
            name: Some(name.into()),
            description: None,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: Vec::new(),
        }
    }
}

/// The core unit of action: a stateful job an agent is performing for a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl Task {
    /// A newly submitted task for `context_id`.
    pub fn submitted(id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Task {
            id: id.into(),
            context_id: Some(context_id.into()),
            status: TaskStatus::now(TaskState::Submitted),
            artifacts: Vec::new(),
            history: Vec::new(),
            metadata: None,
        }
    }

    /// Truncates `history` to the most recent `len` messages, per the `historyLength` semantics
    /// of specification section 3.2.4. `None` leaves the history untouched.
    pub fn with_history_length(mut self, len: Option<i32>) -> Self {
        if let Some(len) = len {
            let len = len.max(0) as usize;
            if self.history.len() > len {
                self.history.drain(..self.history.len() - len);
            }
        }
        self
    }
}

/// An event announcing that a task's status changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// An event announcing that a task produced or extended an artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub artifact: Artifact,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub append: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_chunk: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}
