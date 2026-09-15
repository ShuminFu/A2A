//! The interface an agent implements, and the events it emits while running.

use a2a_types::{A2aError, Artifact, Message, Task, TaskState, TaskStatus};
use tokio::sync::{mpsc, watch};

/// An update produced by a running agent.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// The task moved to a new state, optionally with an explanatory message.
    Status(TaskStatus),
    /// The task produced an artifact, or a further chunk of one.
    Artifact {
        /// The artifact, whole or partial.
        artifact: Artifact,
        /// Append to a previously sent artifact with the same id rather than replacing it.
        append: bool,
        /// This is the final chunk of the artifact.
        last_chunk: bool,
    },
}

/// The handle an agent uses to report progress.
///
/// Dropping the handle without reaching a terminal or interrupted state leaves the task
/// `WORKING`; the runtime marks such a task `FAILED` when the executor returns.
#[derive(Debug, Clone)]
pub struct EventSender {
    tx: mpsc::Sender<AgentEvent>,
}

impl EventSender {
    pub(crate) fn new(tx: mpsc::Sender<AgentEvent>) -> Self {
        EventSender { tx }
    }

    /// Emits a raw event.
    pub async fn send(&self, event: AgentEvent) {
        // A closed receiver means the runtime stopped caring about this task (it was canceled
        // or the process is shutting down); dropping the event is the correct response.
        let _ = self.tx.send(event).await;
    }

    /// Moves the task to `state` with no message.
    pub async fn status(&self, state: TaskState) {
        self.send(AgentEvent::Status(TaskStatus::now(state))).await;
    }

    /// Moves the task to `state` with an agent message.
    pub async fn status_with_text(&self, state: TaskState, text: impl Into<String>) {
        let message = Message::agent_text(new_message_id(), text);
        self.send(AgentEvent::Status(TaskStatus::now_with_message(
            state, message,
        )))
        .await;
    }

    /// Reports that work has started.
    pub async fn working(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::Working, text).await;
    }

    /// Emits a complete artifact.
    pub async fn artifact(&self, artifact: Artifact) {
        self.send(AgentEvent::Artifact {
            artifact,
            append: false,
            last_chunk: true,
        })
        .await;
    }

    /// Emits a chunk of a streamed artifact.
    pub async fn artifact_chunk(&self, artifact: Artifact, append: bool, last_chunk: bool) {
        self.send(AgentEvent::Artifact {
            artifact,
            append,
            last_chunk,
        })
        .await;
    }

    /// Completes the task successfully.
    pub async fn complete(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::Completed, text).await;
    }

    /// Fails the task.
    pub async fn fail(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::Failed, text).await;
    }

    /// Pauses the task until the client sends another message for the same task id.
    pub async fn input_required(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::InputRequired, text).await;
    }

    /// Pauses the task until the client authenticates.
    pub async fn auth_required(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::AuthRequired, text).await;
    }

    /// Rejects the task outright.
    pub async fn reject(&self, text: impl Into<String>) {
        self.status_with_text(TaskState::Rejected, text).await;
    }
}

/// A cancellation signal handed to a running agent.
#[derive(Debug, Clone)]
pub struct CancelSignal {
    /// `None` for a signal that can never fire, which is what tests construct.
    rx: Option<watch::Receiver<bool>>,
}

impl CancelSignal {
    pub(crate) fn new(rx: watch::Receiver<bool>) -> Self {
        CancelSignal { rx: Some(rx) }
    }

    /// A signal that never fires, for building a [`RequestContext`] in tests.
    pub fn never_canceled() -> Self {
        CancelSignal { rx: None }
    }

    /// `true` once `CancelTask` has been called for this task.
    pub fn is_canceled(&self) -> bool {
        self.rx.as_ref().is_some_and(|rx| *rx.borrow())
    }

    /// Resolves when the task is canceled. Long-running agents should select on this.
    ///
    /// A task whose runtime has gone away is never canceled, so this waits forever rather than
    /// resolving: an agent selecting on it keeps working instead of stopping spuriously.
    pub async fn canceled(&mut self) {
        let Some(rx) = self.rx.as_mut() else {
            return std::future::pending().await;
        };
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                return std::future::pending().await;
            }
        }
    }
}

/// Everything an agent needs to handle one incoming message.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// The message that triggered this execution.
    pub message: Message,
    /// The task as it stands, including the history of this context.
    pub task: Task,
    /// The routing identifier from the request, when the agent is served multi-tenant.
    pub tenant: Option<String>,
    /// Fires when the client cancels the task.
    pub cancel: CancelSignal,
}

impl RequestContext {
    /// The text of the incoming message.
    pub fn text(&self) -> String {
        self.message.text()
    }

    /// `true` when this message continues a task that was waiting for input.
    pub fn is_continuation(&self) -> bool {
        self.task
            .history
            .iter()
            .filter(|message| message.role == a2a_types::Role::User)
            .count()
            > 1
    }
}

/// The agent itself: the one trait an implementation has to write.
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    /// Handles one message. Report progress through `events`; the runtime persists each event,
    /// fans it out to stream subscribers and delivers push notifications.
    ///
    /// Returning `Ok(())` without having reached a terminal or interrupted state marks the task
    /// `COMPLETED`. Returning `Err` marks it `FAILED`.
    async fn execute(&self, ctx: RequestContext, events: EventSender) -> Result<(), A2aError>;

    /// Called when a client cancels a task. The default does nothing: the runtime has already
    /// set the cancellation signal and marked the task `CANCELED`.
    async fn cancel(&self, _task_id: &str) -> Result<(), A2aError> {
        Ok(())
    }
}

pub(crate) fn new_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
