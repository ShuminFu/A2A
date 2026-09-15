# A2A v1.0 in Rust

A Rust implementation of the [A2A protocol](https://github.com/a2aproject/A2A) **v1.0**: the data
model, a JSON-RPC server runtime, a client, a sample agent and a CLI.

This replaces the Python and JavaScript samples at the root of this fork, which implement the
v0.1 protocol (`tasks/send`, `/.well-known/agent.json`) that upstream removed. See
[`../UPSTREAM_REVIEW.md`](../UPSTREAM_REVIEW.md) for what changed and why a port rather than a
translation was the right move.

## Crates

| Crate | What it is |
|---|---|
| `a2a-types` | The protocol data model. Transport-agnostic, no async runtime, transcribed from `specification/a2a.proto` at `v1.0.1`. |
| `a2a-server` | The agent runtime: task lifecycle, streaming fan-out, cancellation, push notifications, plus the JSON-RPC/SSE binding as an axum router. |
| `a2a-client` | A client for the JSON-RPC binding, including an SSE reader for the streaming methods. |
| `a2a-echo-agent` | A sample agent (`a2a-echo-agent` binary) serving text tools. |
| `a2a-cli` | A command line client (`a2a` binary). |

## Try it

```bash
cargo run -p a2a-echo-agent            # serves on 127.0.0.1:9999

cargo run -p a2a-cli -- card           # the Agent Card from /.well-known/agent-card.json
cargo run -p a2a-cli -- send echo hello world
cargo run -p a2a-cli -- stream slow a long document
cargo run -p a2a-cli -- list
```

`stream` shows the parts that matter: the task snapshot arrives first, then status and artifact
updates, and the stream closes on the terminal state.

A task that needs more input pauses rather than failing:

```bash
cargo run -p a2a-cli -- send reverse            # -> TASK_STATE_INPUT_REQUIRED
cargo run -p a2a-cli -- send --task-id <id> hello   # resumes the same task
```

## Writing an agent

Implement one trait. The runtime owns everything else — task state, history, streaming
subscribers, cancellation signalling and webhook delivery.

```rust
use a2a_server::{AgentExecutor, EventSender, RequestContext};
use a2a_types::{A2aError, Artifact};

struct MyAgent;

#[async_trait::async_trait]
impl AgentExecutor for MyAgent {
    async fn execute(&self, ctx: RequestContext, events: EventSender) -> Result<(), A2aError> {
        events.working("thinking").await;
        events.artifact(Artifact::text("result", "answer", ctx.text())).await;
        events.complete("done").await;
        Ok(())
    }
}
```

Then serve it:

```rust
let service = a2a_server::A2aService::new(card, MyAgent);
let listener = tokio::net::TcpListener::bind("127.0.0.1:9999").await?;
axum::serve(listener, a2a_server::http::router(service)).await?;
```

Returning `Ok(())` without reaching a terminal or interrupted state completes the task;
returning `Err` fails it. Calling `events.input_required(..)` pauses it, and the next message
carrying the same `taskId` resumes it.

## What is implemented

All eleven `A2AService` operations over the JSON-RPC binding:

`SendMessage`, `SendStreamingMessage`, `GetTask`, `ListTasks`, `CancelTask`, `SubscribeToTask`,
`CreateTaskPushNotificationConfig`, `GetTaskPushNotificationConfig`,
`ListTaskPushNotificationConfigs`, `DeleteTaskPushNotificationConfig`, `GetExtendedAgentCard`.

Protocol behaviour that the tests pin down:

- **Wire format** — camelCase fields, ProtoJSON enum names (`TASK_STATE_WORKING`, `ROLE_USER`),
  ISO 8601 timestamps, `oneof`s as single-key objects, `canceled` spelled the v1.0 way.
- **Task lifecycle** — all nine states, with terminal and interrupted states distinguished;
  a blocking `SendMessage` returns on either, and a terminal task never moves again.
- **Multi-turn** — `INPUT_REQUIRED` pauses a task; reusing its `taskId` continues it in the same
  context, extending one history.
- **Streaming** — SSE, opening with the task snapshot and closing on the final event;
  `SubscribeToTask` attaches mid-flight and is refused on a terminal task (`-32004`).
- **Cancellation** — cooperative: the runtime marks the task `CANCELED` and signals the agent,
  which decides where to stop.
- **Push notifications** — full config CRUD, delivery of the terminal task to the webhook with
  the configuration's token in `X-A2A-Notification-Token`.
- **Errors** — the specification's code table, each carrying a `google.rpc.ErrorInfo` detail.
- **Versioning** — a client pinning a version this server does not speak gets `-32009`.
- **Discovery** — the card at `/.well-known/agent-card.json`, `supportedInterfaces` with
  per-interface binding, version and tenant; the client picks its interface from the card.

## Not implemented

- The **gRPC** and **HTTP+JSON** bindings. The service layer is transport-independent, so they
  are additional bindings rather than a rewrite.
- **Agent Card signing and verification** (JWS, specification 8.4). Signatures are modelled and
  round-trip, but nothing signs or checks them.
- **Authentication.** Security schemes are modelled and served in the card; enforcing them is
  left to the deployment, which is where the credentials live.
- Persistent storage: `InMemoryTaskStore` is the only `TaskStore`, and the trait is the seam for
  a real one.

## Tests

```bash
cargo test --workspace
```

`a2a-types` tests pin the wire format against the specification. `a2a-server` tests run a real
server and drive it with the real client over HTTP, including an SSE stream and a webhook
receiver.
