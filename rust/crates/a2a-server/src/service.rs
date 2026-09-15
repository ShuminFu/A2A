//! The transport-independent implementation of the eleven `A2AService` operations.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use a2a_types::{
    A2aError, AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, Result, SendMessageRequest, SendMessageResponse,
    StreamResponse, SubscribeToTaskRequest, Task, TaskArtifactUpdateEvent,
    TaskPushNotificationConfig, TaskState, TaskStatus, TaskStatusUpdateEvent,
};
use tokio::sync::{broadcast, mpsc, watch};

use crate::executor::{AgentEvent, AgentExecutor, CancelSignal, EventSender, RequestContext};
use crate::push;
use crate::store::{InMemoryTaskStore, PushConfigStore, TaskStore};

/// How many stream events are buffered per task before a slow subscriber starts losing them.
const BROADCAST_CAPACITY: usize = 64;
/// `ListTasks` page size when the client does not ask for one.
const DEFAULT_PAGE_SIZE: i32 = 50;
/// The largest page `ListTasks` will return, per the proto's documented bounds.
const MAX_PAGE_SIZE: i32 = 100;

struct Inner {
    card: AgentCard,
    extended_card: Option<AgentCard>,
    executor: Arc<dyn AgentExecutor>,
    store: Arc<dyn TaskStore>,
    push_configs: PushConfigStore,
    http: reqwest::Client,
    channels: Mutex<HashMap<String, broadcast::Sender<StreamResponse>>>,
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
}

/// An agent, ready to be served over any protocol binding.
///
/// The service owns task state, streaming fan-out, cancellation and push notification delivery;
/// the binding layer only translates wire formats.
#[derive(Clone)]
pub struct A2aService {
    inner: Arc<Inner>,
}

impl A2aService {
    /// Builds a service for `card`, executed by `executor`, storing tasks in memory.
    pub fn new(card: AgentCard, executor: impl AgentExecutor) -> Self {
        A2aService {
            inner: Arc::new(Inner {
                card,
                extended_card: None,
                executor: Arc::new(executor),
                store: Arc::new(InMemoryTaskStore::new()),
                push_configs: PushConfigStore::new(),
                http: reqwest::Client::new(),
                channels: Mutex::new(HashMap::new()),
                cancels: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Replaces the task store.
    pub fn with_task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        let inner = Arc::get_mut(&mut self.inner).expect("service was cloned before configuration");
        inner.store = store;
        self
    }

    /// Sets the card returned by `GetExtendedAgentCard` to authenticated callers.
    pub fn with_extended_agent_card(mut self, card: AgentCard) -> Self {
        let inner = Arc::get_mut(&mut self.inner).expect("service was cloned before configuration");
        inner.extended_card = Some(card);
        self
    }

    /// The agent's public card.
    pub fn agent_card(&self) -> &AgentCard {
        &self.inner.card
    }

    /// `SendMessage`: starts or continues a task and waits for it to settle.
    ///
    /// Returns when the task reaches a terminal or interrupted state, unless the request sets
    /// `returnImmediately`.
    pub async fn send_message(&self, request: SendMessageRequest) -> Result<SendMessageResponse> {
        let return_immediately = request
            .configuration
            .as_ref()
            .is_some_and(|configuration| configuration.return_immediately);
        let history_length = request
            .configuration
            .as_ref()
            .and_then(|configuration| configuration.history_length);

        let (task, mut events) = self.inner.clone().start(request).await?;
        if return_immediately {
            return Ok(SendMessageResponse::Task(
                task.with_history_length(history_length),
            ));
        }

        // The receiver was subscribed before the executor started, so no early event is missed.
        loop {
            match events.recv().await {
                Ok(event) if event.is_final() => break,
                Ok(_) => continue,
                // Lagged: the task outran this buffer. The store still holds the truth.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }

        let task = self
            .inner
            .store
            .get(&task.id)
            .await
            .ok_or_else(|| A2aError::TaskNotFound(task.id.clone()))?;
        Ok(SendMessageResponse::Task(
            task.with_history_length(history_length),
        ))
    }

    /// `SendStreamingMessage`: starts or continues a task and streams its updates.
    ///
    /// The first element of the returned pair is the initial task snapshot, which a binding
    /// sends as the first stream event.
    pub async fn send_streaming_message(
        &self,
        request: SendMessageRequest,
    ) -> Result<(Task, broadcast::Receiver<StreamResponse>)> {
        if !self.inner.card.capabilities.streaming.unwrap_or(false) {
            return Err(A2aError::UnsupportedOperation(
                "this agent does not support streaming".to_string(),
            ));
        }
        self.inner.clone().start(request).await
    }

    /// `GetTask`: the current state of one task.
    pub async fn get_task(&self, request: GetTaskRequest) -> Result<Task> {
        let task = self
            .inner
            .store
            .get(&request.id)
            .await
            .ok_or_else(|| A2aError::TaskNotFound(request.id.clone()))?;
        Ok(task.with_history_length(request.history_length))
    }

    /// `ListTasks`: filtered, paginated access to this agent's tasks.
    pub async fn list_tasks(&self, request: ListTasksRequest) -> Result<ListTasksResponse> {
        let mut tasks: Vec<Task> = self
            .inner
            .store
            .all()
            .await
            .into_iter()
            .filter(|task| match &request.context_id {
                Some(context_id) => task.context_id.as_deref() == Some(context_id.as_str()),
                None => true,
            })
            .filter(|task| match request.status {
                Some(state) => task.status.state == state,
                None => true,
            })
            .filter(|task| match request.status_timestamp_after {
                Some(after) => task.status.timestamp.is_some_and(|stamp| stamp >= after),
                None => true,
            })
            .collect();

        let total_size = tasks.len() as i32;
        let page_size = request
            .page_size
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let offset: usize = match &request.page_token {
            Some(token) if !token.is_empty() => token
                .parse()
                .map_err(|_| A2aError::InvalidParams(format!("malformed pageToken: {token}")))?,
            _ => 0,
        };

        let end = (offset + page_size as usize).min(tasks.len());
        let page: Vec<Task> = if offset >= tasks.len() {
            Vec::new()
        } else {
            tasks.drain(offset..end).collect()
        };
        let next_page_token = if end < total_size as usize {
            end.to_string()
        } else {
            String::new()
        };

        let include_artifacts = request.include_artifacts.unwrap_or(false);
        let tasks = page
            .into_iter()
            .map(|mut task| {
                if !include_artifacts {
                    task.artifacts.clear();
                }
                task.with_history_length(request.history_length)
            })
            .collect();

        Ok(ListTasksResponse {
            tasks,
            next_page_token,
            page_size,
            total_size,
        })
    }

    /// `CancelTask`: stops a task that has not finished.
    pub async fn cancel_task(&self, request: CancelTaskRequest) -> Result<Task> {
        let task = self
            .inner
            .store
            .get(&request.id)
            .await
            .ok_or_else(|| A2aError::TaskNotFound(request.id.clone()))?;
        if task.status.state.is_terminal() {
            return Err(A2aError::TaskNotCancelable(format!(
                "task {} is already in a terminal state",
                request.id
            )));
        }

        if let Some(cancel) = self
            .inner
            .cancels
            .lock()
            .expect("cancels poisoned")
            .get(&request.id)
        {
            let _ = cancel.send(true);
        }
        self.inner.executor.cancel(&request.id).await?;
        self.inner
            .apply(
                &request.id,
                AgentEvent::Status(TaskStatus::now(TaskState::Canceled)),
            )
            .await;

        self.inner
            .store
            .get(&request.id)
            .await
            .ok_or(A2aError::TaskNotFound(request.id))
    }

    /// `SubscribeToTask`: attaches to the update stream of a task that is still running.
    pub async fn subscribe_to_task(
        &self,
        request: SubscribeToTaskRequest,
    ) -> Result<(Task, broadcast::Receiver<StreamResponse>)> {
        let task = self
            .inner
            .store
            .get(&request.id)
            .await
            .ok_or_else(|| A2aError::TaskNotFound(request.id.clone()))?;
        if task.status.state.is_terminal() {
            return Err(A2aError::UnsupportedOperation(format!(
                "task {} is in a terminal state and cannot be subscribed to",
                request.id
            )));
        }
        let receiver = self.inner.channel(&request.id).subscribe();
        Ok((task, receiver))
    }

    /// `CreateTaskPushNotificationConfig`.
    pub async fn create_push_notification_config(
        &self,
        mut config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig> {
        self.inner.require_push_notifications()?;
        let task_id = config
            .task_id
            .clone()
            .ok_or_else(|| A2aError::InvalidParams("taskId is required".to_string()))?;
        if self.inner.store.get(&task_id).await.is_none() {
            return Err(A2aError::TaskNotFound(task_id));
        }
        if config.url.is_empty() {
            return Err(A2aError::InvalidParams("url is required".to_string()));
        }
        config.task_id = Some(task_id);
        Ok(self.inner.push_configs.create(config))
    }

    /// `GetTaskPushNotificationConfig`.
    pub async fn get_push_notification_config(
        &self,
        request: GetTaskPushNotificationConfigRequest,
    ) -> Result<TaskPushNotificationConfig> {
        self.inner.require_push_notifications()?;
        self.inner
            .push_configs
            .get(&request.task_id, &request.id)
            .ok_or_else(|| {
                A2aError::TaskNotFound(format!(
                    "no push notification config {} for task {}",
                    request.id, request.task_id
                ))
            })
    }

    /// `ListTaskPushNotificationConfigs`.
    pub async fn list_push_notification_configs(
        &self,
        request: ListTaskPushNotificationConfigsRequest,
    ) -> Result<ListTaskPushNotificationConfigsResponse> {
        self.inner.require_push_notifications()?;
        Ok(ListTaskPushNotificationConfigsResponse {
            configs: self.inner.push_configs.list(&request.task_id),
            next_page_token: String::new(),
        })
    }

    /// `DeleteTaskPushNotificationConfig`.
    pub async fn delete_push_notification_config(
        &self,
        request: DeleteTaskPushNotificationConfigRequest,
    ) -> Result<()> {
        self.inner.require_push_notifications()?;
        if self
            .inner
            .push_configs
            .delete(&request.task_id, &request.id)
        {
            Ok(())
        } else {
            Err(A2aError::TaskNotFound(format!(
                "no push notification config {} for task {}",
                request.id, request.task_id
            )))
        }
    }

    /// `GetExtendedAgentCard`.
    pub async fn get_extended_agent_card(
        &self,
        _request: GetExtendedAgentCardRequest,
    ) -> Result<AgentCard> {
        self.inner
            .extended_card
            .clone()
            .ok_or(A2aError::ExtendedAgentCardNotConfigured)
    }
}

impl Inner {
    fn require_push_notifications(&self) -> Result<()> {
        if self.card.capabilities.push_notifications.unwrap_or(false) {
            Ok(())
        } else {
            Err(A2aError::PushNotificationNotSupported)
        }
    }

    /// The broadcast channel for a task, created on first use.
    fn channel(&self, task_id: &str) -> broadcast::Sender<StreamResponse> {
        let mut channels = self.channels.lock().expect("channels poisoned");
        channels
            .entry(task_id.to_string())
            .or_insert_with(|| broadcast::channel(BROADCAST_CAPACITY).0)
            .clone()
    }

    fn cancel_signal(&self, task_id: &str) -> watch::Receiver<bool> {
        let mut cancels = self.cancels.lock().expect("cancels poisoned");
        cancels
            .entry(task_id.to_string())
            .or_insert_with(|| watch::channel(false).0)
            .subscribe()
    }

    /// Resolves the target task, records the incoming message and starts the executor.
    async fn start(
        self: Arc<Self>,
        request: SendMessageRequest,
    ) -> Result<(Task, broadcast::Receiver<StreamResponse>)> {
        let mut message = request.message;
        if message.parts.is_empty() {
            return Err(A2aError::InvalidParams(
                "message.parts must contain at least one part".to_string(),
            ));
        }
        if message.message_id.is_empty() {
            return Err(A2aError::InvalidParams(
                "message.messageId is required".to_string(),
            ));
        }

        let mut task = match &message.task_id {
            Some(task_id) => {
                let task = self
                    .store
                    .get(task_id)
                    .await
                    .ok_or_else(|| A2aError::TaskNotFound(task_id.clone()))?;
                if task.status.state.is_terminal() {
                    return Err(A2aError::UnsupportedOperation(format!(
                        "task {task_id} is in a terminal state and cannot accept more messages"
                    )));
                }
                // A client may pass both ids, but they have to agree (specification 3.4.1).
                if let (Some(context_id), Some(task_context)) =
                    (&message.context_id, &task.context_id)
                {
                    if context_id != task_context {
                        return Err(A2aError::InvalidParams(format!(
                            "contextId {context_id} does not match task {task_id}"
                        )));
                    }
                }
                task
            }
            None => {
                let context_id = message
                    .context_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                Task::submitted(uuid::Uuid::new_v4().to_string(), context_id)
            }
        };

        message.task_id = Some(task.id.clone());
        message.context_id = task.context_id.clone();
        task.history.push(message.clone());
        task.status = TaskStatus::now(TaskState::Submitted);
        self.store.put(task.clone()).await;

        if let Some(config) = request
            .configuration
            .as_ref()
            .and_then(|configuration| configuration.task_push_notification_config.clone())
        {
            self.require_push_notifications()?;
            let mut config = config;
            config.task_id = Some(task.id.clone());
            self.push_configs.create(config);
        }

        // Subscribe before the executor runs so the caller sees every event it emits.
        let channel = self.channel(&task.id);
        let subscription = channel.subscribe();
        let cancel = CancelSignal::new(self.cancel_signal(&task.id));

        let context = RequestContext {
            message,
            task: task.clone(),
            tenant: request.tenant.clone(),
            cancel,
        };

        let (event_tx, mut event_rx) = mpsc::channel(BROADCAST_CAPACITY);
        let executor = self.executor.clone();
        let runtime = self.clone();
        let task_id = task.id.clone();

        tokio::spawn(async move {
            let handle =
                tokio::spawn(
                    async move { executor.execute(context, EventSender::new(event_tx)).await },
                );

            // The executor's sender closes when it returns, ending this loop.
            while let Some(event) = event_rx.recv().await {
                runtime.apply(&task_id, event).await;
            }

            let outcome = match handle.await {
                Ok(result) => result,
                Err(join_error) => Err(A2aError::Internal(format!("agent panicked: {join_error}"))),
            };
            runtime.finalize(&task_id, outcome).await;
        });

        Ok((task, subscription))
    }

    /// Applies one agent event to the stored task and publishes it to subscribers.
    async fn apply(&self, task_id: &str, event: AgentEvent) {
        let Some(mut task) = self.store.get(task_id).await else {
            tracing::warn!(task = %task_id, "event for unknown task dropped");
            return;
        };
        // A terminal task never moves again, whatever a late event says.
        if task.status.state.is_terminal() {
            return;
        }
        let context_id = task.context_id.clone().unwrap_or_default();

        let stream_event = match event {
            AgentEvent::Status(status) => {
                if let Some(message) = &status.message {
                    let mut message = message.clone();
                    message.task_id = Some(task.id.clone());
                    message.context_id = task.context_id.clone();
                    task.history.push(message);
                }
                task.status = status.clone();
                StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                    task_id: task.id.clone(),
                    context_id,
                    status,
                    metadata: None,
                })
            }
            AgentEvent::Artifact {
                artifact,
                append,
                last_chunk,
            } => {
                merge_artifact(&mut task, &artifact, append);
                StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
                    task_id: task.id.clone(),
                    context_id,
                    artifact,
                    append,
                    last_chunk,
                    metadata: None,
                })
            }
        };

        let terminal = task.status.state.is_terminal();
        self.store.put(task.clone()).await;
        let _ = self.channel(task_id).send(stream_event);

        if terminal {
            let configs = self.push_configs.list(task_id);
            if !configs.is_empty() {
                push::deliver(&self.http, &configs, &task).await;
            }
            // Nothing more will ever be published for this task. Subscribers already hold the
            // buffered events, so dropping the sender releases them rather than stranding them.
            self.channels
                .lock()
                .expect("channels poisoned")
                .remove(task_id);
            self.cancels
                .lock()
                .expect("cancels poisoned")
                .remove(task_id);
        }
    }

    /// Settles a task once its executor has returned.
    async fn finalize(&self, task_id: &str, outcome: Result<()>) {
        let Some(task) = self.store.get(task_id).await else {
            return;
        };
        if task.status.state.is_final_for_blocking_call() {
            return;
        }
        let status = match outcome {
            Ok(()) => TaskStatus::now(TaskState::Completed),
            Err(error) => TaskStatus::now_with_message(
                TaskState::Failed,
                a2a_types::Message::agent_text(uuid::Uuid::new_v4().to_string(), error.to_string()),
            ),
        };
        self.apply(task_id, AgentEvent::Status(status)).await;
    }
}

/// Adds an artifact to a task, appending to an existing one when the agent asked for that.
fn merge_artifact(task: &mut Task, artifact: &a2a_types::Artifact, append: bool) {
    match task
        .artifacts
        .iter_mut()
        .find(|existing| existing.artifact_id == artifact.artifact_id)
    {
        Some(existing) if append => existing.parts.extend(artifact.parts.iter().cloned()),
        Some(existing) => *existing = artifact.clone(),
        None => task.artifacts.push(artifact.clone()),
    }
}
