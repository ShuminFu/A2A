//! Task and push-notification-config storage.

use std::collections::HashMap;
use std::sync::Mutex;

use a2a_types::{Task, TaskPushNotificationConfig};

/// Where tasks live between requests.
///
/// The protocol requires tasks to outlive the connection that created them: `GetTask`,
/// `ListTasks` and `SubscribeToTask` all read a task the client is no longer streaming.
#[async_trait::async_trait]
pub trait TaskStore: Send + Sync + 'static {
    /// Fetches a task by id.
    async fn get(&self, id: &str) -> Option<Task>;
    /// Inserts or replaces a task.
    async fn put(&self, task: Task);
    /// Every task, ordered oldest first. Filtering and pagination happen above this.
    async fn all(&self) -> Vec<Task>;
}

/// A task store backed by a `HashMap`, suitable for tests and single-process agents.
#[derive(Debug, Default)]
pub struct InMemoryTaskStore {
    tasks: Mutex<HashMap<String, Task>>,
    order: Mutex<Vec<String>>,
}

impl InMemoryTaskStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn get(&self, id: &str) -> Option<Task> {
        self.tasks
            .lock()
            .expect("task store poisoned")
            .get(id)
            .cloned()
    }

    async fn put(&self, task: Task) {
        let mut tasks = self.tasks.lock().expect("task store poisoned");
        if tasks.insert(task.id.clone(), task.clone()).is_none() {
            self.order
                .lock()
                .expect("task order poisoned")
                .push(task.id);
        }
    }

    async fn all(&self) -> Vec<Task> {
        let tasks = self.tasks.lock().expect("task store poisoned");
        self.order
            .lock()
            .expect("task order poisoned")
            .iter()
            .filter_map(|id| tasks.get(id).cloned())
            .collect()
    }
}

/// The push notification configurations registered for each task.
#[derive(Debug, Default)]
pub struct PushConfigStore {
    configs: Mutex<HashMap<String, Vec<TaskPushNotificationConfig>>>,
}

impl PushConfigStore {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `config` for its task, assigning an id when the client did not supply one.
    pub fn create(&self, mut config: TaskPushNotificationConfig) -> TaskPushNotificationConfig {
        let task_id = config.task_id.clone().unwrap_or_default();
        if config.id.is_none() {
            config.id = Some(uuid::Uuid::new_v4().to_string());
        }
        let mut configs = self.configs.lock().expect("push config store poisoned");
        let entry = configs.entry(task_id).or_default();
        // Re-creating a configuration with a known id replaces it, keeping the operation
        // idempotent for clients that retry.
        if let Some(existing) = entry.iter_mut().find(|c| c.id == config.id) {
            *existing = config.clone();
        } else {
            entry.push(config.clone());
        }
        config
    }

    /// Fetches one configuration.
    pub fn get(&self, task_id: &str, id: &str) -> Option<TaskPushNotificationConfig> {
        self.configs
            .lock()
            .expect("push config store poisoned")
            .get(task_id)?
            .iter()
            .find(|config| config.id.as_deref() == Some(id))
            .cloned()
    }

    /// Every configuration for a task.
    pub fn list(&self, task_id: &str) -> Vec<TaskPushNotificationConfig> {
        self.configs
            .lock()
            .expect("push config store poisoned")
            .get(task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Removes one configuration, reporting whether it existed.
    pub fn delete(&self, task_id: &str, id: &str) -> bool {
        let mut configs = self.configs.lock().expect("push config store poisoned");
        let Some(entry) = configs.get_mut(task_id) else {
            return false;
        };
        let before = entry.len();
        entry.retain(|config| config.id.as_deref() != Some(id));
        before != entry.len()
    }
}
