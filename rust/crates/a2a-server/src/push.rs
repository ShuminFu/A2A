//! Push notification delivery.

use a2a_types::{Task, TaskPushNotificationConfig};

/// The header carrying the configuration's validation token, so a receiver can check that a
/// notification belongs to a task it asked about (specification section 3.5.3).
pub const NOTIFICATION_TOKEN_HEADER: &str = "X-A2A-Notification-Token";

/// Posts `task` to every configuration registered for it.
///
/// Delivery is best-effort: a failing webhook is logged and does not affect the task, which
/// remains readable through `GetTask`.
pub async fn deliver(
    client: &reqwest::Client,
    configs: &[TaskPushNotificationConfig],
    task: &Task,
) {
    for config in configs {
        let mut request = client.post(&config.url).json(task);
        if let Some(token) = &config.token {
            request = request.header(NOTIFICATION_TOKEN_HEADER, token);
        }
        if let Some(auth) = &config.authentication {
            if let Some(credentials) = &auth.credentials {
                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    format!("{} {}", auth.scheme, credentials),
                );
            }
        }
        match request.send().await {
            Ok(response) if response.status().is_success() => {
                tracing::debug!(task = %task.id, url = %config.url, "push notification delivered");
            }
            Ok(response) => {
                tracing::warn!(
                    task = %task.id,
                    url = %config.url,
                    status = %response.status(),
                    "push notification rejected"
                );
            }
            Err(error) => {
                tracing::warn!(task = %task.id, url = %config.url, %error, "push notification failed");
            }
        }
    }
}
