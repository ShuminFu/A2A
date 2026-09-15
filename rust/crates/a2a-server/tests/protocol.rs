//! End-to-end tests: a real HTTP server driven by the real client over the JSON-RPC binding.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use a2a_client::{A2aClient, ClientError};
use a2a_server::{http, A2aService, AgentExecutor, EventSender, RequestContext};
use a2a_types::{
    A2aError, AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact,
    CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigsRequest,
    ListTasksRequest, Message, SendMessageConfiguration, SendMessageRequest, SendMessageResponse,
    StreamResponse, SubscribeToTaskRequest, Task, TaskPushNotificationConfig, TaskState,
};
use futures_util::StreamExt;

/// A test agent covering the paths the runtime has to get right: streaming progress, an
/// interrupted state, cancellation and failure.
struct TestAgent;

#[async_trait::async_trait]
impl AgentExecutor for TestAgent {
    async fn execute(&self, mut ctx: RequestContext, events: EventSender) -> Result<(), A2aError> {
        let text = ctx.text();

        if text.starts_with("ask") {
            events.input_required("which text should I echo?").await;
            return Ok(());
        }
        if text.starts_with("boom") {
            return Err(A2aError::Internal("the agent exploded".to_string()));
        }
        if text.starts_with("slow") {
            events.working("starting slow work").await;
            for _ in 0..100 {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                    _ = ctx.cancel.canceled() => return Ok(()),
                }
            }
            events.complete("slow work done").await;
            return Ok(());
        }

        events.working("echoing").await;
        events
            .artifact(Artifact::text("result", "echo", text.clone()))
            .await;
        events.complete(text).await;
        Ok(())
    }
}

fn test_card(public_url: &str) -> AgentCard {
    AgentCard {
        name: "Test Agent".to_string(),
        description: "An agent used by the integration tests.".to_string(),
        supported_interfaces: vec![AgentInterface::jsonrpc(format!("{public_url}/rpc"))],
        provider: None,
        version: "1.0.0".to_string(),
        documentation_url: None,
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(true),
            extensions: Vec::new(),
            extended_agent_card: Some(true),
        },
        security_schemes: Default::default(),
        security_requirements: Vec::new(),
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills: vec![AgentSkill {
            id: "echo".to_string(),
            name: "Echo".to_string(),
            description: "Echoes text.".to_string(),
            tags: vec!["text".to_string()],
            examples: Vec::new(),
            input_modes: Vec::new(),
            output_modes: Vec::new(),
            security_requirements: Vec::new(),
        }],
        signatures: Vec::new(),
        icon_url: None,
    }
}

/// Starts the test agent on an ephemeral port and returns its base URL.
async fn start_agent(extended_card: bool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());

    let mut service = A2aService::new(test_card(&base_url), TestAgent);
    if extended_card {
        let mut card = test_card(&base_url);
        card.description = "The authenticated view.".to_string();
        service = service.with_extended_agent_card(card);
    }

    tokio::spawn(async move {
        axum::serve(listener, http::router(service)).await.unwrap();
    });
    base_url
}

fn user_message(text: &str) -> Message {
    Message::user_text(uuid::Uuid::new_v4().to_string(), text)
}

fn expect_task(response: SendMessageResponse) -> Task {
    match response {
        SendMessageResponse::Task(task) => task,
        SendMessageResponse::Message(message) => panic!("expected a task, got {message:?}"),
    }
}

#[tokio::test]
async fn agent_card_is_served_from_the_well_known_path() {
    let base_url = start_agent(false).await;
    let client = A2aClient::discover(&base_url).await.unwrap();

    let card = client.agent_card().expect("card was discovered");
    assert_eq!(card.name, "Test Agent");
    assert_eq!(card.supported_interfaces[0].protocol_binding, "JSONRPC");
    assert_eq!(card.supported_interfaces[0].protocol_version, "1.0");
    assert_eq!(client.endpoint(), format!("{base_url}/rpc"));
}

#[tokio::test]
async fn send_message_blocks_until_the_task_completes() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let task = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("hello world")))
            .await
            .unwrap(),
    );

    assert_eq!(task.status.state, TaskState::Completed);
    assert_eq!(task.artifacts.len(), 1);
    assert_eq!(task.artifacts[0].parts[0].as_text(), Some("hello world"));
    // The client message and each agent status message are recorded in the history.
    assert!(task.history.len() >= 2, "history was {:?}", task.history);
    assert_eq!(task.history[0].text(), "hello world");
    assert!(task.context_id.is_some(), "the server assigns a context id");
}

#[tokio::test]
async fn return_immediately_hands_back_the_task_before_it_finishes() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let request = SendMessageRequest::new(user_message("slow job")).with_configuration(
        SendMessageConfiguration {
            return_immediately: true,
            ..Default::default()
        },
    );
    let task = expect_task(client.send_message(request).await.unwrap());
    assert!(
        !task.status.state.is_terminal(),
        "expected an unfinished task, got {:?}",
        task.status.state
    );

    client
        .cancel_task(CancelTaskRequest::new(task.id))
        .await
        .unwrap();
}

#[tokio::test]
async fn streaming_delivers_the_task_then_its_updates() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let mut events = Box::pin(
        client
            .send_streaming_message(SendMessageRequest::new(user_message("streamed")))
            .await
            .unwrap(),
    );

    let mut states = Vec::new();
    let mut artifacts = 0;
    let mut saw_initial_task = false;
    while let Some(event) = events.next().await {
        match event.unwrap() {
            StreamResponse::Task(_) => saw_initial_task = true,
            StreamResponse::StatusUpdate(update) => states.push(update.status.state),
            StreamResponse::ArtifactUpdate(_) => artifacts += 1,
            StreamResponse::Message(_) => {}
        }
    }

    assert!(saw_initial_task, "the stream opens with the task snapshot");
    assert_eq!(artifacts, 1);
    assert_eq!(
        states,
        vec![TaskState::Working, TaskState::Completed],
        "the stream ends on the terminal state"
    );
}

#[tokio::test]
async fn input_required_pauses_the_task_and_the_client_resumes_it() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let paused = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("ask me")))
            .await
            .unwrap(),
    );
    assert_eq!(paused.status.state, TaskState::InputRequired);

    // Continuing means reusing the task id: the server keeps the context and the history.
    let mut followup = user_message("the answer");
    followup.task_id = Some(paused.id.clone());

    let resumed = expect_task(
        client
            .send_message(SendMessageRequest::new(followup))
            .await
            .unwrap(),
    );
    assert_eq!(resumed.id, paused.id);
    assert_eq!(resumed.context_id, paused.context_id);
    assert_eq!(resumed.status.state, TaskState::Completed);
    assert!(
        resumed.history.len() > paused.history.len(),
        "the follow-up message extends the same task history"
    );
}

#[tokio::test]
async fn a_failing_agent_marks_the_task_failed_rather_than_erroring_the_call() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let task = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("boom")))
            .await
            .unwrap(),
    );
    assert_eq!(task.status.state, TaskState::Failed);
    assert!(task
        .status
        .message
        .as_ref()
        .is_some_and(|message| message.text().contains("exploded")));
}

#[tokio::test]
async fn get_task_reads_back_a_finished_task_and_honours_history_length() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let task = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("remembered")))
            .await
            .unwrap(),
    );

    let fetched = client
        .get_task(GetTaskRequest::new(task.id.clone()))
        .await
        .unwrap();
    assert_eq!(fetched.id, task.id);
    assert_eq!(fetched.status.state, TaskState::Completed);

    let trimmed = client
        .get_task(GetTaskRequest {
            tenant: None,
            id: task.id.clone(),
            history_length: Some(1),
        })
        .await
        .unwrap();
    assert_eq!(trimmed.history.len(), 1);

    let missing = client.get_task(GetTaskRequest::new("no-such-task")).await;
    assert!(
        matches!(
            missing,
            Err(ClientError::Protocol(A2aError::TaskNotFound(_)))
        ),
        "got {missing:?}"
    );
}

#[tokio::test]
async fn list_tasks_filters_by_context_and_paginates() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let first = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("one")))
            .await
            .unwrap(),
    );
    let context_id = first.context_id.clone().unwrap();

    // Two more messages in the same context, each its own task.
    for text in ["two", "three"] {
        let mut message = user_message(text);
        message.context_id = Some(context_id.clone());
        client
            .send_message(SendMessageRequest::new(message))
            .await
            .unwrap();
    }
    // And one in a context of its own.
    client
        .send_message(SendMessageRequest::new(user_message("elsewhere")))
        .await
        .unwrap();

    let all = client
        .list_tasks(ListTasksRequest::default())
        .await
        .unwrap();
    assert_eq!(all.total_size, 4);

    let in_context = client
        .list_tasks(ListTasksRequest {
            context_id: Some(context_id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(in_context.total_size, 3);
    assert!(in_context.next_page_token.is_empty());

    let page = client
        .list_tasks(ListTasksRequest {
            context_id: Some(context_id.clone()),
            page_size: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.tasks.len(), 2);
    assert_eq!(page.next_page_token, "2");
    assert!(
        page.tasks[0].artifacts.is_empty(),
        "artifacts are omitted unless the client asks for them"
    );

    let rest = client
        .list_tasks(ListTasksRequest {
            context_id: Some(context_id),
            page_size: Some(2),
            page_token: Some(page.next_page_token),
            include_artifacts: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rest.tasks.len(), 1);
    assert!(rest.next_page_token.is_empty());
    assert!(!rest.tasks[0].artifacts.is_empty());

    let completed = client
        .list_tasks(ListTasksRequest {
            status: Some(TaskState::Completed),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(completed.total_size, 4);
}

#[tokio::test]
async fn cancel_stops_a_running_task_and_is_refused_once_terminal() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let request = SendMessageRequest::new(user_message("slow job")).with_configuration(
        SendMessageConfiguration {
            return_immediately: true,
            ..Default::default()
        },
    );
    let started = expect_task(client.send_message(request).await.unwrap());

    let canceled = client
        .cancel_task(CancelTaskRequest::new(started.id.clone()))
        .await
        .unwrap();
    assert_eq!(canceled.status.state, TaskState::Canceled);

    // The runtime keeps a terminal task terminal, even though the agent is still unwinding.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = client
        .get_task(GetTaskRequest::new(started.id.clone()))
        .await
        .unwrap();
    assert_eq!(after.status.state, TaskState::Canceled);

    let again = client.cancel_task(CancelTaskRequest::new(started.id)).await;
    assert!(
        matches!(
            again,
            Err(ClientError::Protocol(A2aError::TaskNotCancelable(_)))
        ),
        "got {again:?}"
    );
}

#[tokio::test]
async fn subscribing_to_a_terminal_task_is_an_unsupported_operation() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let task = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("finished")))
            .await
            .unwrap(),
    );

    let result = client
        .subscribe_to_task(SubscribeToTaskRequest::new(task.id))
        .await
        .map(|_| ());
    assert!(
        matches!(
            result,
            Err(ClientError::Protocol(A2aError::UnsupportedOperation(_)))
        ),
        "got {result:?}"
    );
}

#[tokio::test]
async fn subscribe_follows_a_task_that_is_still_running() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let request = SendMessageRequest::new(user_message("slow job")).with_configuration(
        SendMessageConfiguration {
            return_immediately: true,
            ..Default::default()
        },
    );
    let started = expect_task(client.send_message(request).await.unwrap());

    let mut events = Box::pin(
        client
            .subscribe_to_task(SubscribeToTaskRequest::new(started.id.clone()))
            .await
            .unwrap(),
    );
    let first = events
        .next()
        .await
        .expect("the stream opens immediately")
        .unwrap();
    assert!(matches!(first, StreamResponse::Task(_)));

    client
        .cancel_task(CancelTaskRequest::new(started.id))
        .await
        .unwrap();

    let terminal = events
        .next()
        .await
        .expect("cancellation reaches the subscriber")
        .unwrap();
    assert_eq!(terminal.task_state(), Some(TaskState::Canceled));
}

#[tokio::test]
async fn push_notification_configs_support_full_crud() {
    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    let task = expect_task(
        client
            .send_message(SendMessageRequest::new(user_message("watched")))
            .await
            .unwrap(),
    );

    let mut config = TaskPushNotificationConfig::new("https://webhook.example.com/a2a")
        .with_token("shared-secret");
    config.task_id = Some(task.id.clone());
    let created = client
        .create_push_notification_config(config)
        .await
        .unwrap();
    let config_id = created.id.clone().expect("the server assigns an id");

    let fetched = client
        .get_push_notification_config(GetTaskPushNotificationConfigRequest {
            tenant: None,
            task_id: task.id.clone(),
            id: config_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(fetched.url, "https://webhook.example.com/a2a");
    assert_eq!(fetched.token.as_deref(), Some("shared-secret"));

    let listed = client
        .list_push_notification_configs(ListTaskPushNotificationConfigsRequest {
            tenant: None,
            task_id: task.id.clone(),
            page_size: None,
            page_token: None,
        })
        .await
        .unwrap();
    assert_eq!(listed.configs.len(), 1);

    client
        .delete_push_notification_config(DeleteTaskPushNotificationConfigRequest {
            tenant: None,
            task_id: task.id.clone(),
            id: config_id.clone(),
        })
        .await
        .unwrap();

    let after = client
        .list_push_notification_configs(ListTaskPushNotificationConfigsRequest {
            tenant: None,
            task_id: task.id,
            page_size: None,
            page_token: None,
        })
        .await
        .unwrap();
    assert!(after.configs.is_empty());
}

#[tokio::test]
async fn a_terminal_task_is_posted_to_the_registered_webhook() {
    // A webhook receiver that records the token header and task of every delivery.
    type Deliveries = Arc<Mutex<Vec<(Option<String>, Task)>>>;
    let received: Deliveries = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let webhook_url = format!("http://{}/hook", listener.local_addr().unwrap());
    let app = axum::Router::new().route(
        "/hook",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(task): axum::Json<Task>| {
                let sink = sink.clone();
                async move {
                    let token = headers
                        .get(a2a_server::push::NOTIFICATION_TOKEN_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    sink.lock().unwrap().push((token, task));
                    axum::http::StatusCode::OK
                }
            },
        ),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();

    // The push config can ride along with the message that creates the task.
    let request = SendMessageRequest::new(user_message("notify me")).with_configuration(
        SendMessageConfiguration {
            task_push_notification_config: Some(
                TaskPushNotificationConfig::new(&webhook_url).with_token("shared-secret"),
            ),
            ..Default::default()
        },
    );
    let task = expect_task(client.send_message(request).await.unwrap());
    assert_eq!(task.status.state, TaskState::Completed);

    // Delivery happens as the task settles; give the webhook a moment to be called.
    for _ in 0..50 {
        if !received.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let deliveries = received.lock().unwrap();
    assert_eq!(
        deliveries.len(),
        1,
        "exactly one notification for one terminal state"
    );
    let (token, delivered) = &deliveries[0];
    assert_eq!(token.as_deref(), Some("shared-secret"));
    assert_eq!(delivered.id, task.id);
    assert_eq!(delivered.status.state, TaskState::Completed);
}

#[tokio::test]
async fn the_extended_agent_card_is_only_served_when_configured() {
    let with_card = A2aClient::discover(&start_agent(true).await).await.unwrap();
    let extended = with_card.get_extended_agent_card().await.unwrap();
    assert_eq!(extended.description, "The authenticated view.");

    let without_card = A2aClient::discover(&start_agent(false).await)
        .await
        .unwrap();
    let result = without_card.get_extended_agent_card().await;
    assert!(
        matches!(
            result,
            Err(ClientError::Protocol(
                A2aError::ExtendedAgentCardNotConfigured
            ))
        ),
        "got {result:?}"
    );
}

#[tokio::test]
async fn unknown_methods_and_bad_envelopes_are_rejected_per_jsonrpc() {
    let base_url = start_agent(false).await;
    let endpoint = format!("{base_url}/rpc");
    let http = reqwest::Client::new();

    let response: serde_json::Value = http
        .post(&endpoint)
        .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tasks/send"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        response["error"]["code"], -32601,
        "the v0.x method name is gone in v1.0: {response}"
    );

    let response: serde_json::Value = http
        .post(&endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["error"]["code"], -32700);

    let response: serde_json::Value = http
        .post(&endpoint)
        .json(&serde_json::json!({"jsonrpc": "1.0", "id": 1, "method": "GetTask"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["error"]["code"], -32600);

    let response: serde_json::Value = http
        .post(&endpoint)
        .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "GetTask", "params": {}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        response["error"]["code"], -32602,
        "id is required: {response}"
    );
}

#[tokio::test]
async fn a_client_pinning_an_older_protocol_version_is_told_it_is_unsupported() {
    let base_url = start_agent(false).await;
    let response: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/rpc"))
        .header(a2a_server::http::VERSION_HEADER, "0.3")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "GetTask",
            "params": {"id": "whatever"}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(response["error"]["code"], -32009, "got {response}");
    assert_eq!(
        response["error"]["data"][0]["reason"],
        "VersionNotSupportedError"
    );
}
