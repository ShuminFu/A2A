//! The JSON-RPC 2.0 protocol binding over HTTP, with Server-Sent Events for streaming.

use std::convert::Infallible;

use a2a_types::{
    method, A2aError, JsonRpcRequest, JsonRpcResponse, StreamResponse, Task, JSONRPC_PATH,
};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{stream, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::service::A2aService;

/// The header a client uses to pin the protocol version it speaks (specification section 9.2).
pub const VERSION_HEADER: &str = "A2A-Version";
/// The header carrying the extension URIs a client activates.
pub const EXTENSIONS_HEADER: &str = "A2A-Extensions";

/// An HTTP router serving `service`: the Agent Card at its well-known path, and the JSON-RPC
/// binding at both `/` and `/rpc`.
pub fn router(service: A2aService) -> Router {
    Router::new()
        .route(a2a_types::AGENT_CARD_WELL_KNOWN_PATH, get(agent_card))
        .route("/", post(jsonrpc))
        .route(JSONRPC_PATH, post(jsonrpc))
        .with_state(service)
}

async fn agent_card(State(service): State<A2aService>) -> Json<a2a_types::AgentCard> {
    Json(service.agent_card().clone())
}

async fn jsonrpc(State(service): State<A2aService>, headers: HeaderMap, body: Bytes) -> Response {
    let request: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return error_response(None, A2aError::JsonParse),
    };
    if request.jsonrpc != a2a_types::jsonrpc::JSONRPC_VERSION {
        return error_response(
            request.id,
            A2aError::InvalidRequest("jsonrpc must be \"2.0\"".to_string()),
        );
    }
    if let Err(error) = check_version(&headers) {
        return error_response(request.id, error);
    }

    let id = request.id.clone();
    match dispatch(&service, request).await {
        Ok(outcome) => outcome.into_response(id),
        Err(error) => error_response(id, error),
    }
}

/// What a dispatched method produced: a plain result, or a stream to hand back as SSE.
enum Outcome {
    Value(Value),
    Stream(Box<(Task, broadcast::Receiver<StreamResponse>)>),
}

impl Outcome {
    fn into_response(self, id: Option<Value>) -> Response {
        match self {
            Outcome::Value(value) => Json(JsonRpcResponse::success(id, value)).into_response(),
            Outcome::Stream(stream) => {
                let (task, receiver) = *stream;
                sse_response(id, task, receiver)
            }
        }
    }
}

async fn dispatch(service: &A2aService, request: JsonRpcRequest) -> Result<Outcome, A2aError> {
    let params = request.params;
    match request.method.as_str() {
        method::SEND_MESSAGE => {
            let response = service.send_message(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&response)?))
        }
        method::SEND_STREAMING_MESSAGE => {
            let stream = service.send_streaming_message(parse(params)?).await?;
            Ok(Outcome::Stream(Box::new(stream)))
        }
        method::GET_TASK => {
            let task = service.get_task(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&task)?))
        }
        method::LIST_TASKS => {
            let response = service.list_tasks(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&response)?))
        }
        method::CANCEL_TASK => {
            let task = service.cancel_task(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&task)?))
        }
        method::SUBSCRIBE_TO_TASK => {
            let stream = service.subscribe_to_task(parse(params)?).await?;
            Ok(Outcome::Stream(Box::new(stream)))
        }
        method::CREATE_TASK_PUSH_NOTIFICATION_CONFIG => {
            let config = service
                .create_push_notification_config(parse(params)?)
                .await?;
            Ok(Outcome::Value(to_value(&config)?))
        }
        method::GET_TASK_PUSH_NOTIFICATION_CONFIG => {
            let config = service.get_push_notification_config(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&config)?))
        }
        method::LIST_TASK_PUSH_NOTIFICATION_CONFIGS => {
            let response = service
                .list_push_notification_configs(parse(params)?)
                .await?;
            Ok(Outcome::Value(to_value(&response)?))
        }
        method::DELETE_TASK_PUSH_NOTIFICATION_CONFIG => {
            service
                .delete_push_notification_config(parse(params)?)
                .await?;
            Ok(Outcome::Value(json!({})))
        }
        method::GET_EXTENDED_AGENT_CARD => {
            let card = service.get_extended_agent_card(parse(params)?).await?;
            Ok(Outcome::Value(to_value(&card)?))
        }
        unknown => Err(A2aError::MethodNotFound(unknown.to_string())),
    }
}

/// Streams `task` and everything that follows it as SSE, closing after the final event.
fn sse_response(
    id: Option<Value>,
    task: Task,
    receiver: broadcast::Receiver<StreamResponse>,
) -> Response {
    let initial = stream::once(async move { StreamResponse::Task(task) });

    // The stream has to end *on* the final event rather than one event later: nothing further
    // is coming, so a combinator that needs another item to notice would hang the client.
    let updates = stream::unfold((receiver, false), |(mut receiver, finished)| async move {
        if finished {
            return None;
        }
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let last = event.is_final();
                    return Some((event, (receiver, last)));
                }
                // Lagged: this subscriber fell behind. Keep reading; `GetTask` has the truth.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    let events = initial.chain(updates);

    let events = events.map(move |event| {
        let payload = match serde_json::to_value(&event) {
            Ok(value) => JsonRpcResponse::success(id.clone(), value),
            Err(error) => JsonRpcResponse::failure(
                id.clone(),
                A2aError::Internal(error.to_string()).to_jsonrpc_error(),
            ),
        };
        Ok::<Event, Infallible>(
            Event::default().data(serde_json::to_string(&payload).unwrap_or_default()),
        )
    });

    Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Rejects a client that pins a protocol version this server does not implement.
fn check_version(headers: &HeaderMap) -> Result<(), A2aError> {
    let Some(value) = headers.get(VERSION_HEADER) else {
        return Ok(());
    };
    let requested = value
        .to_str()
        .map_err(|_| A2aError::InvalidRequest(format!("{VERSION_HEADER} is not valid UTF-8")))?;
    if requested == a2a_types::PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(A2aError::VersionNotSupported(format!(
            "this agent speaks {}, not {requested}",
            a2a_types::PROTOCOL_VERSION
        )))
    }
}

fn parse<T: DeserializeOwned>(params: Option<Value>) -> Result<T, A2aError> {
    let params = params.unwrap_or_else(|| json!({}));
    serde_json::from_value(params).map_err(|error| A2aError::InvalidParams(error.to_string()))
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, A2aError> {
    serde_json::to_value(value).map_err(|error| A2aError::Internal(error.to_string()))
}

/// JSON-RPC carries its own error codes, so transport status stays 200 for protocol errors and
/// only a malformed envelope is answered with a 400.
fn error_response(id: Option<Value>, error: A2aError) -> Response {
    let status = match error {
        A2aError::JsonParse => StatusCode::BAD_REQUEST,
        _ => StatusCode::OK,
    };
    (
        status,
        Json(JsonRpcResponse::failure(id, error.to_jsonrpc_error())),
    )
        .into_response()
}
