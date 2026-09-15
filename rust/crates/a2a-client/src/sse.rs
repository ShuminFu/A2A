//! A minimal Server-Sent Events reader for the streaming methods.

use std::collections::VecDeque;
use std::pin::Pin;

use a2a_types::{A2aError, JsonRpcResponse, StreamResponse};
use futures_util::{Stream, StreamExt};

use crate::error::{ClientError, Result};

type ByteStream = Pin<Box<dyn Stream<Item = reqwest::Result<bytes::Bytes>> + Send>>;

struct SseState {
    bytes: ByteStream,
    buffer: String,
    pending: VecDeque<Result<StreamResponse>>,
    finished: bool,
}

/// Turns an SSE response into a stream of protocol events.
///
/// Each event carries a JSON-RPC response envelope: a `result` becomes a [`StreamResponse`], an
/// `error` becomes a protocol error and ends the stream.
pub fn read_events(response: reqwest::Response) -> impl Stream<Item = Result<StreamResponse>> {
    let state = SseState {
        bytes: Box::pin(response.bytes_stream()),
        buffer: String::new(),
        pending: VecDeque::new(),
        finished: false,
    };

    futures_util::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                return Some((event, state));
            }
            if state.finished {
                return None;
            }
            match state.bytes.next().await {
                Some(Ok(chunk)) => {
                    state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    drain_frames(&mut state);
                }
                Some(Err(error)) => {
                    state.finished = true;
                    state
                        .pending
                        .push_back(Err(ClientError::Transport(error.to_string())));
                }
                None => {
                    // The connection closed: flush whatever the last frame held.
                    state.buffer.push_str("\n\n");
                    drain_frames(&mut state);
                    state.finished = true;
                }
            }
        }
    })
}

/// Pulls every complete `\n\n`-terminated frame out of the buffer.
fn drain_frames(state: &mut SseState) {
    state.buffer = state.buffer.replace("\r\n", "\n");
    while let Some(position) = state.buffer.find("\n\n") {
        let frame: String = state.buffer.drain(..position + 2).collect();
        if let Some(event) = parse_frame(&frame) {
            state.pending.push_back(event);
        }
    }
}

/// Parses one SSE frame. Comments, keep-alives and empty frames yield `None`.
fn parse_frame(frame: &str) -> Option<Result<StreamResponse>> {
    let data: String = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    if data.trim().is_empty() {
        return None;
    }

    let response: JsonRpcResponse = match serde_json::from_str(&data) {
        Ok(response) => response,
        Err(error) => return Some(Err(ClientError::Decode(error.to_string()))),
    };
    if let Some(error) = response.error {
        return Some(Err(ClientError::Protocol(A2aError::from_code(
            error.code,
            error.message,
        ))));
    }
    let result = response.result.unwrap_or(serde_json::Value::Null);
    Some(serde_json::from_value(result).map_err(|error| ClientError::Decode(error.to_string())))
}
