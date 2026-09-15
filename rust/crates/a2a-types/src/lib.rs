//! The A2A protocol v1.0 data model in Rust.
//!
//! Types are transcribed by hand from the canonical `specification/a2a.proto` of
//! [`a2aproject/A2A`](https://github.com/a2aproject/A2A) at release `v1.0.1`, and serialize to
//! the JSON form the specification requires: camelCase fields (section 5.5), ProtoJSON enum
//! names such as `TASK_STATE_WORKING`, ISO 8601 timestamps, and proto `oneof`s rendered as a
//! single-key object.
//!
//! The crate is transport-agnostic and has no async runtime: [`a2a-server`] implements the
//! JSON-RPC binding on top of it and [`a2a-client`] speaks to it.
//!
//! [`a2a-server`]: https://docs.rs/a2a-server
//! [`a2a-client`]: https://docs.rs/a2a-client

//! Field-level semantics are those of the proto messages the types mirror, so the field
//! documentation lives there rather than being restated on every field here; each type links
//! its proto counterpart by name.

#![forbid(unsafe_code)]

pub mod card;
pub mod core;
pub mod error;
pub mod jsonrpc;
pub mod push;
pub mod request;

pub use card::{
    AgentCapabilities, AgentCard, AgentCardSignature, AgentExtension, AgentInterface,
    AgentProvider, AgentSkill, SecurityRequirement, SecurityScheme, AGENT_CARD_WELL_KNOWN_PATH,
    PROTOCOL_VERSION,
};
pub use core::{
    Artifact, Message, Metadata, Part, PartContent, Role, Task, TaskArtifactUpdateEvent, TaskState,
    TaskStatus, TaskStatusUpdateEvent,
};
pub use error::A2aError;
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, JSONRPC_PATH, JSONRPC_VERSION};
pub use push::{AuthenticationInfo, TaskPushNotificationConfig};
pub use request::{
    method, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageConfiguration, SendMessageRequest,
    SendMessageResponse, StreamResponse, SubscribeToTaskRequest,
};

/// A convenient result type for protocol operations.
pub type Result<T> = std::result::Result<T, A2aError>;
