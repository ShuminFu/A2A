# Upstream review — `a2aproject/A2A`

Review date: 2026-09-13
Fork: `ShuminFu/A2A` (`main`, `claude/modest-turing-2o9nx0`)
Fork point: `fb2f1dc` — *Update agent.py: fixing 1 typo (#241)*, 2025-04-21
Upstream head reviewed: `6d6640c`, 2026-09-10 (release `v1.0.1`)

## Bottom line

This fork is **570 commits behind upstream and 0 commits ahead**. Everything in it is a
snapshot of the pre-v0.1 era of the protocol. In the ~17 months since the fork point the
upstream project changed its name, its repository layout, the source of truth for the
specification, and the protocol surface itself. A merge is not a realistic operation —
the directories this fork is made of (`samples/`, `demo/`, `tests/`, `specification/json/a2a.json`)
no longer exist upstream.

## What changed upstream

### 1. The project moved and re-organised

| | At fork point | Upstream today |
|---|---|---|
| Repository | `google/A2A` | `a2aproject/A2A` (Agentic AI Foundation) |
| Latest release | pre-`v0.1.0` | `v1.0.1` (spec `v1.0.0`, 2026-03-12) |
| Samples | `samples/python`, `samples/js` | moved to [`a2aproject/a2a-samples`](https://github.com/a2aproject/a2a-samples) |
| Demo UI | `demo/ui` | removed |
| Tests | `tests/` | removed |
| SDKs | in-repo | [`a2a-python`](https://github.com/a2aproject/a2a-python), [`a2a-js`](https://github.com/a2aproject/a2a-js), [`a2a-java`](https://github.com/a2aproject/a2a-java), [`a2a-go`](https://github.com/a2aproject/a2a-go), [`a2a-dotnet`](https://github.com/a2aproject/a2a-dotnet), [`a2a-rs`](https://github.com/a2aproject/a2a-rs) |
| Governance | none | `GOVERNANCE.md`, `MAINTAINERS.md`, `adrs/`, TSC with 8 member companies |

The removal happened in `729988a` — *Remove Samples/Demo/test directories and change
references to google/A2A (#668)*. Upstream is now a specification + documentation site
(`mkdocs`) repository, nothing else.

### 2. The specification source of truth changed

`specification/json/a2a.json` — the hand-maintained JSON Schema this fork carries — **is gone
from version control**. The canonical definition is now `specification/a2a.proto`, and
`a2a.json` is a *generated, non-committed build artifact* produced by
`scripts/proto_to_json_schema.sh` (`protoc` + `protoc-gen-jsonschema`, wired into
`scripts/build_docs.sh` by #2074).

Any tooling in this fork that reads `specification/json/a2a.json` from the repo — including
`tests/test_a2a_spec.py` — is reading a file upstream no longer publishes there.

### 3. The protocol surface changed from task-centric to message-centric

v0.1 exposed JSON-RPC methods only. v1.0 defines one gRPC service, `A2AService`, with 11 RPCs
and three official protocol bindings (`JSONRPC`, `GRPC`, `HTTP+JSON`) generated from the same
proto.

| v0.1 (this fork) | v1.0 (upstream) |
|---|---|
| `tasks/send` | `SendMessage` |
| `tasks/sendSubscribe` | `SendStreamingMessage` |
| `tasks/get` | `GetTask` |
| — | `ListTasks` (filtering + pagination) |
| `tasks/cancel` | `CancelTask` |
| `tasks/resubscribe` | `SubscribeToTask` |
| `tasks/pushNotification/set` | `CreateTaskPushNotificationConfig` |
| `tasks/pushNotification/get` | `GetTaskPushNotificationConfig` |
| — | `ListTaskPushNotificationConfigs`, `DeleteTaskPushNotificationConfig` |
| — | `GetExtendedAgentCard` |

The client no longer names the task: it sends a `Message`, and the server creates the `Task`.
Multiple push-notification configs per task are supported (v0.2.5), and all four config
operations are full CRUD.

### 4. Task lifecycle grew from 6 states to 9

Added: `TASK_STATE_REJECTED` (an agent may decline outright, including at creation) and
`TASK_STATE_AUTH_REQUIRED` (interrupted, resumable), plus the proto-mandated
`TASK_STATE_UNSPECIFIED` zero value. `COMPLETED`, `FAILED`, `CANCELED` and `REJECTED` are
terminal; `INPUT_REQUIRED` and `AUTH_REQUIRED` are interrupted but resumable with the same
`task_id`/`context_id`. `SubscribeToTask` returns `UnsupportedOperationError` on a terminal task.

Note the spelling: v1.0 standardised on American `canceled` (#1283).

### 5. Agent Card and security

- Well-known URI changed from `/.well-known/agent.json` to `/.well-known/agent-card.json` (v0.3.0).
  Seven files in this fork still use the old path.
- `supported_interfaces[]` replaces a single URL: each entry carries `url`, `protocol_binding`,
  `protocol_version` and an optional `tenant`, which is what makes version negotiation and
  progressive migration possible.
- `signatures[]` — JWS-signed Agent Cards for cross-organisation trust.
- `AgentExtension` and the extension/binding governance process are new.
- Security schemes gained mutual TLS and OAuth 2.0 device code + PKCE; the implicit and
  password flows were **removed** (#1303).
- `supportsAuthenticatedExtendedCard` became `supportsExtendedAgentCard` and moved onto
  `AgentCapabilities` (#1222, #1307).
- Multi-tenancy is native: an opaque `tenant` field routes many agents behind one endpoint (#1195).
- `TaskStatusUpdateEvent` lost its redundant `final` field (#1308); `Part` was flattened (#1411).

### 6. Release history since the fork point

`v0.1.0` → `v0.2.0` … `v0.2.6` → `v0.3.0` → `v1.0.0-rc` → `v1.0.0` (2026-03-12) → `v1.0.1`
(2026-05-26). Breaking changes landed in v0.2.2, v0.2.5, v0.3.0 and v1.0.0; the v1.0.0 entry
alone lists 14 of them. A2A joined the Agentic AI Foundation in 2026.

## What this means for this fork

Every directory in this fork is v0.1-era material that upstream has since deleted:

- `samples/python`, `samples/js` — 8 files still call `tasks/send` / `tasks/sendSubscribe`
  and resolve `/.well-known/agent.json`. They cannot talk to a v1.0 agent.
- `demo/ui` — the multi-agent demo, removed upstream.
- `tests/` — validates the deleted `specification/json/a2a.json`.
- `specification/json/a2a.json` — superseded by `specification/a2a.proto`.
- `llms.txt`, `README.md` — describe the v0.1 protocol and point at `google/A2A`.

## Recommended follow-up

1. **Do not attempt a merge.** `git merge upstream/main` resolves as "delete everything this
   fork contains, add a documentation site". Nothing useful survives the conflict resolution.
2. **Decide what this fork is for.** Three coherent options:
   - *Track the specification*: `git checkout main && git reset --hard upstream/main`. The fork
     becomes a clean mirror of the spec repo. Cheapest, and correct if the fork exists to follow A2A.
   - *Keep sample code*: drop the fork's `samples/`, `demo/` and `tests/`, and start from
     `a2aproject/a2a-samples` plus the v1.0 SDK for the target language. The v0.1 sample code is
     not worth porting — the client/server contracts it wraps no longer exist.
   - *Preserve a v0.1 snapshot*: tag the current `main` as `v0.1-snapshot`, note in the README that
     it is a historical archive, and point readers at upstream.
3. **If any downstream code depends on this fork's protocol types**, migrate against
   `specification/a2a.proto`, not against `specification/json/a2a.json`, and regenerate the JSON
   Schema with upstream's `scripts/proto_to_json_schema.sh` rather than hand-editing it.

The diagrams in [`diagrams/`](diagrams/) show the v1.0 architecture, a full task interaction,
and the nine-state task lifecycle as they exist upstream today.
