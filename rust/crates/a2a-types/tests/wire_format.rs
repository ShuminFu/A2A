//! The JSON on the wire has to match specification section 5.5 exactly: camelCase fields,
//! ProtoJSON enum names, and `oneof`s rendered as a single-key object.

use a2a_types::core::PartContent;
use a2a_types::*;
use serde_json::json;

#[test]
fn task_state_uses_protojson_enum_names() {
    assert_eq!(
        serde_json::to_value(TaskState::InputRequired).unwrap(),
        json!("TASK_STATE_INPUT_REQUIRED")
    );
    assert_eq!(
        serde_json::to_value(TaskState::Canceled).unwrap(),
        json!("TASK_STATE_CANCELED"),
        "v1.0 standardised on the American spelling"
    );
    assert_eq!(
        serde_json::from_value::<TaskState>(json!("TASK_STATE_AUTH_REQUIRED")).unwrap(),
        TaskState::AuthRequired
    );
}

#[test]
fn role_uses_protojson_enum_names() {
    assert_eq!(
        serde_json::to_value(Role::User).unwrap(),
        json!("ROLE_USER")
    );
    assert_eq!(
        serde_json::from_value::<Role>(json!("ROLE_AGENT")).unwrap(),
        Role::Agent
    );
}

#[test]
fn text_part_flattens_the_content_oneof() {
    let part = Part::text("hello");
    assert_eq!(
        serde_json::to_value(&part).unwrap(),
        json!({"text": "hello"})
    );

    let decoded: Part = serde_json::from_value(json!({"text": "hello"})).unwrap();
    assert_eq!(decoded, part);
}

#[test]
fn file_and_data_parts_carry_media_type_alongside_the_oneof() {
    let file = Part::file_url("https://example.com/a.png", "image/png");
    assert_eq!(
        serde_json::to_value(&file).unwrap(),
        json!({"url": "https://example.com/a.png", "mediaType": "image/png"})
    );

    let data = Part::data(json!({"answer": 42}));
    assert_eq!(
        serde_json::to_value(&data).unwrap(),
        json!({"data": {"answer": 42}, "mediaType": "application/json"})
    );

    let raw: Part = serde_json::from_value(json!({"raw": "aGk=", "filename": "hi.txt"})).unwrap();
    assert_eq!(raw.content, PartContent::Raw("aGk=".to_string()));
    assert_eq!(raw.filename.as_deref(), Some("hi.txt"));
}

#[test]
fn message_fields_are_camel_case() {
    let mut message = Message::user_text("msg-1", "hello");
    message.task_id = Some("task-1".to_string());
    message.context_id = Some("ctx-1".to_string());
    message.reference_task_ids = vec!["task-0".to_string()];

    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        json!({
            "messageId": "msg-1",
            "contextId": "ctx-1",
            "taskId": "task-1",
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}],
            "referenceTaskIds": ["task-0"],
        })
    );
}

#[test]
fn send_message_response_is_a_single_key_oneof() {
    let task = Task::submitted("task-1", "ctx-1");
    let value = serde_json::to_value(SendMessageResponse::Task(task.clone())).unwrap();
    assert!(
        value.get("task").is_some(),
        "expected a task payload, got {value}"
    );
    assert!(value.get("message").is_none());

    let round_tripped: SendMessageResponse = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, SendMessageResponse::Task(task));
}

#[test]
fn stream_response_names_match_the_proto_oneof() {
    let event = StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
        task_id: "task-1".to_string(),
        context_id: "ctx-1".to_string(),
        status: TaskStatus::now(TaskState::Working),
        metadata: None,
    });
    let value = serde_json::to_value(&event).unwrap();
    assert!(value.get("statusUpdate").is_some(), "got {value}");

    let artifact = StreamResponse::ArtifactUpdate(TaskArtifactUpdateEvent {
        task_id: "task-1".to_string(),
        context_id: "ctx-1".to_string(),
        artifact: Artifact::text("a-1", "result", "done"),
        append: false,
        last_chunk: true,
        metadata: None,
    });
    let value = serde_json::to_value(&artifact).unwrap();
    assert!(value.get("artifactUpdate").is_some(), "got {value}");
    assert_eq!(value["artifactUpdate"]["lastChunk"], json!(true));
}

#[test]
fn timestamps_serialize_as_iso_8601() {
    let status = TaskStatus::now(TaskState::Working);
    let value = serde_json::to_value(&status).unwrap();
    let timestamp = value["timestamp"].as_str().expect("timestamp is a string");
    chrono::DateTime::parse_from_rfc3339(timestamp).expect("timestamp parses as RFC 3339");
}

#[test]
fn agent_card_round_trips_through_the_documented_shape() {
    let json = json!({
        "name": "Text Tools",
        "description": "Transforms text.",
        "supportedInterfaces": [{
            "url": "https://agent.example.com/rpc",
            "protocolBinding": "JSONRPC",
            "protocolVersion": "1.0",
            "tenant": "team-a"
        }],
        "version": "1.0.0",
        "capabilities": {"streaming": true, "pushNotifications": true},
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": [{
            "id": "echo",
            "name": "Echo",
            "description": "Returns the text it was given.",
            "tags": ["text"]
        }],
        "securitySchemes": {
            "oidc": {
                "openIdConnectSecurityScheme": {
                    "openIdConnectUrl": "https://id.example.com/.well-known/openid-configuration"
                }
            },
            "mtls": {"mtlsSecurityScheme": {}}
        }
    });

    let card: AgentCard = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        card.supported_interfaces[0].tenant.as_deref(),
        Some("team-a")
    );
    assert_eq!(
        card.preferred_supported_interface().unwrap().url,
        "https://agent.example.com/rpc"
    );
    assert_eq!(serde_json::to_value(&card).unwrap(), json);
}

#[test]
fn agent_card_well_known_path_is_the_v0_3_name() {
    assert_eq!(AGENT_CARD_WELL_KNOWN_PATH, "/.well-known/agent-card.json");
}

#[test]
fn terminal_and_interrupted_states_are_classified_per_the_proto() {
    for state in [
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Canceled,
        TaskState::Rejected,
    ] {
        assert!(state.is_terminal(), "{state:?} is terminal");
        assert!(!state.is_interrupted());
    }
    for state in [TaskState::InputRequired, TaskState::AuthRequired] {
        assert!(state.is_interrupted(), "{state:?} is interrupted");
        assert!(!state.is_terminal());
        assert!(state.is_final_for_blocking_call());
    }
    for state in [TaskState::Submitted, TaskState::Working] {
        assert!(!state.is_final_for_blocking_call());
    }
}

#[test]
fn history_length_keeps_the_most_recent_messages() {
    let mut task = Task::submitted("task-1", "ctx-1");
    task.history = (0..5)
        .map(|index| Message::user_text(format!("msg-{index}"), format!("message {index}")))
        .collect();

    let trimmed = task.clone().with_history_length(Some(2));
    assert_eq!(trimmed.history.len(), 2);
    assert_eq!(trimmed.history[0].message_id, "msg-3");

    assert_eq!(task.clone().with_history_length(Some(0)).history.len(), 0);
    assert_eq!(task.clone().with_history_length(None).history.len(), 5);
    assert_eq!(task.with_history_length(Some(99)).history.len(), 5);
}

#[test]
fn error_codes_match_the_specification_table() {
    assert_eq!(A2aError::TaskNotFound("t".into()).code(), -32001);
    assert_eq!(A2aError::TaskNotCancelable("t".into()).code(), -32002);
    assert_eq!(A2aError::PushNotificationNotSupported.code(), -32003);
    assert_eq!(A2aError::UnsupportedOperation("x".into()).code(), -32004);
    assert_eq!(A2aError::ContentTypeNotSupported("x".into()).code(), -32005);
    assert_eq!(A2aError::InvalidAgentResponse("x".into()).code(), -32006);
    assert_eq!(A2aError::ExtendedAgentCardNotConfigured.code(), -32007);
    assert_eq!(
        A2aError::ExtensionSupportRequired("x".into()).code(),
        -32008
    );
    assert_eq!(A2aError::VersionNotSupported("0.3".into()).code(), -32009);
    assert_eq!(A2aError::MethodNotFound("x".into()).code(), -32601);
}

#[test]
fn errors_carry_a_typed_detail_object() {
    let error = A2aError::TaskNotFound("task-1".to_string()).to_jsonrpc_error();
    let details = error.data.expect("error carries details");
    assert_eq!(details[0]["@type"], json!("google.rpc.ErrorInfo"));
    assert_eq!(details[0]["reason"], json!("TaskNotFoundError"));
}

#[test]
fn jsonrpc_method_names_are_the_v1_pascal_case_names() {
    assert_eq!(method::SEND_MESSAGE, "SendMessage");
    assert_eq!(method::SUBSCRIBE_TO_TASK, "SubscribeToTask");
    assert_eq!(
        method::CREATE_TASK_PUSH_NOTIFICATION_CONFIG,
        "CreateTaskPushNotificationConfig"
    );
}
