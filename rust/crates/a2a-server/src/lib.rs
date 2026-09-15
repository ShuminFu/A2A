//! An A2A protocol v1.0 agent runtime and JSON-RPC binding.
//!
//! Write an [`AgentExecutor`], hand it to an [`A2aService`] along with an [`AgentCard`], and
//! serve [`http::router`] with axum. The runtime owns task state, streaming fan-out,
//! cancellation and push notification delivery.
//!
//! ```no_run
//! use a2a_server::{http, A2aService, AgentExecutor, EventSender, RequestContext};
//! use a2a_types::{A2aError, AgentCard};
//!
//! struct Echo;
//!
//! #[async_trait::async_trait]
//! impl AgentExecutor for Echo {
//!     async fn execute(&self, ctx: RequestContext, events: EventSender) -> Result<(), A2aError> {
//!         events.complete(format!("you said: {}", ctx.text())).await;
//!         Ok(())
//!     }
//! }
//!
//! # async fn serve(card: AgentCard) -> Result<(), Box<dyn std::error::Error>> {
//! let service = A2aService::new(card, Echo);
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:9999").await?;
//! axum::serve(listener, http::router(service)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! [`AgentCard`]: a2a_types::AgentCard

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod executor;
pub mod http;
pub mod push;
pub mod service;
pub mod store;

pub use executor::{AgentEvent, AgentExecutor, CancelSignal, EventSender, RequestContext};
pub use service::A2aService;
pub use store::{InMemoryTaskStore, PushConfigStore, TaskStore};
