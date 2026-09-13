# Upstream Review — 2026-09-13

Review of the current state of the upstream A2A project against this fork.

| | Revision | Date | Protocol |
|---|---|---|---|
| This fork (`ShuminFu/A2A`) | `fb2f1dc` | 2025-04-21 | v0.1-era draft |
| Upstream (`a2aproject/A2A` @ `main`) | `6d6640c` | 2026-09-10 | **v1.0.1** |

**Headline: this fork cannot be updated by merging.** Upstream replaced its history
during the move from `google/A2A` to `a2aproject/A2A`, so the two trees share no
common ancestor (`git merge-base` returns nothing) and the fork is three protocol
generations behind (v0.1 → v0.2/0.3 → v1.0).

```console
$ git rev-list --left-right --count HEAD...upstream/main
58      200
$ git merge-base HEAD upstream/main   # empty — unrelated histories
```

## 1. The latest upstream changes

Most recent upstream activity is documentation and governance work on a repository
that is now spec-and-docs only:

- **Docs readability sweep** — `what-is-a2a` (#2223), `a2a-and-mcp` (#2224),
  `key-concepts` (#2225), `life-of-a-task` (#2226).
- **Blog surfacing** (#2195), including the post announcing that **A2A joined the
  Agentic AI Foundation (AAIF)**. The donation is visible in the spec itself: the
  proto package is now `lf.a2a.v1` (`option java_package = "com.google.lf.a2a.v1"`).
- **Spec doc fixes** — `PushNotificationConfig` → `TaskPushNotificationConfig`
  (#1981), pagination field names in the migration guide (#2165), in-task
  authorization scope semantics (#2081), gRPC URL example in `AgentInterface` (#1997).
- **Build change: JSON Schema is now generated** (#2074). `specification/json/a2a.json`
  is a *non-normative build artifact* produced from `specification/a2a.proto` and is
  deliberately **no longer committed**. `a2a.proto` is the normative source of truth.
- **Governance/process** — maintainers and CODEOWNERS updates, Rust SDK added,
  vulnerability intake moved to GitHub Security Advisories (#2086), daily
  docs link-check workflow (#2059), ADRs directory.
- **Partners/integrations** additions (FAF, OpenAgents, TiEQi-A2A, Noorle, Dealer Handshake).

## 2. Structural changes to the upstream repository

| Area | Fork (April 2025) | Upstream today |
|---|---|---|
| `samples/`, `demo/`, `tests/` | present (Python + JS agents, mesop demo UI) | removed → [`a2aproject/a2a-samples`](https://github.com/a2aproject/a2a-samples) |
| Reference implementations | vendored under `samples/python/common` | six official SDKs (Python, Go, JS, Java, .NET, Rust) |
| Normative spec | `specification/json/a2a.json` (committed) | `specification/a2a.proto`; JSON schema generated, uncommitted |
| Docs | `README.md` + `llms.txt` | MkDocs site (`docs/`, `mkdocs.yml`), ADRs, blog |
| Governance | Google-led | Linux Foundation / AAIF (`GOVERNANCE.md`, `MAINTAINERS.md`) |

## 3. Protocol breaking changes affecting this fork

The fork's transport surface is the original `tasks/*` RPC family. Every one of
those method names is gone upstream:

| Fork (v0.1) | Upstream v1.0 | Note |
|---|---|---|
| `tasks/send` | `SendMessage` (`POST /message:send`) | renamed via v0.3 `message/send` |
| `tasks/sendSubscribe` | `SendStreamingMessage` (`POST /message:stream`) | renamed via v0.3 `message/stream` |
| `tasks/get` | `GetTask` | history semantics clarified |
| `tasks/cancel` | `CancelTask` | |
| `tasks/resubscribe` | `SubscribeToTask` | |
| `tasks/pushNotification/set` \| `/get` | `CreateTaskPushNotificationConfig`, `GetTaskPushNotificationConfig`, `ListTaskPushNotificationConfigs`, `DeleteTaskPushNotificationConfig` | model flattened; payloads use `StreamResponse` |
| — | `ListTasks` | **new**: filtering + cursor pagination |

Model-level breaks that invalidate the fork's `a2a.json` and its Python/JS types:

- **`TaskState` enum.** Fork: `submitted`, `working`, `input-required`, `completed`,
  `canceled`, `failed`, `unknown`. Upstream: `TASK_STATE_SUBMITTED`,
  `TASK_STATE_WORKING`, `TASK_STATE_INPUT_REQUIRED`, `TASK_STATE_COMPLETED`,
  `TASK_STATE_CANCELED`, `TASK_STATE_FAILED`, `TASK_STATE_REJECTED`,
  `TASK_STATE_AUTH_REQUIRED`, `TASK_STATE_UNSPECIFIED`.
  `rejected` and `auth-required` are new; `unknown` became `UNSPECIFIED`.
- **`Message.role`.** `user`/`agent` → `ROLE_USER`/`ROLE_AGENT`.
- **`Part` redesign.** The `kind` discriminator is gone across the model; `Part` is a
  single unified message using JSON member names (`text`, `url`/`bytes` + `filename`,
  `data`) with `mediaType` replacing `mimeType`.
- **`TaskStatusUpdateEvent.final` removed** — stream closure is binding-specific.
- **Agent Card.** `protocolVersion` moved onto each `AgentInterface`;
  `preferredTransport` + `additionalInterfaces` consolidated into `supportedInterfaces[]`;
  `supportsAuthenticatedExtendedCard` → `capabilities.extendedAgentCard`;
  discovery path is `/.well-known/agent-card.json`; JWS + RFC 8785 card signing.
- **IDs are simple literals** — no compound `tasks/{id}` resource names.
- **No `/v1` prefix** on HTTP+JSON paths.
- **Multi-tenancy** — `tenant` on requests and on `AgentInterface`.
- **OAuth 2.0 modernized** — implicit and password flows removed, Device Code
  (RFC 8628) added, `pkce_required` on Authorization Code.
- **Timestamps** — ISO 8601 UTC with millisecond precision.

## 4. What that means for the code in this fork

- `specification/json/a2a.json` is stale *and* structurally obsolete: upstream no
  longer hand-maintains this file. Regenerating it requires `a2a.proto` plus
  `protoc-gen-jsonschema` (see upstream `specification/json/README.md`).
- `samples/python/common/` (types, client, server, task manager) encodes the
  `tasks/send` family, lowercase enums and `kind`-tagged parts — i.e. the wire format
  at every layer. This is a rewrite, not a patch.
- `samples/python/agents/*`, `samples/js/`, `demo/ui/` and `tests/` all sit on top of
  that common layer and move with it.
- Nothing in this fork is local work: all 58 commits are upstream commits from the
  April 2025 snapshot, so there is no fork-specific behaviour to preserve.

## 5. Recommended follow-up

**Adopt the SDKs rather than forward-port this tree.** The vendored `samples/python/common`
layer existed because no SDK did; six official SDKs now do, and upstream deleted its own
copy of exactly this code. Concretely:

1. Treat this fork as a **read-only v0.1 archive** and say so in `README.md`.
2. For new work, start from [`a2aproject/a2a-samples`](https://github.com/a2aproject/a2a-samples)
   and the SDK for your language; track the spec at
   [`a2aproject/A2A`](https://github.com/a2aproject/A2A).
3. If a v1.0 fork of *this* tree is genuinely wanted, re-baseline instead of merging —
   the histories are unrelated, so any sync is an orphan-branch replacement:

   ```bash
   git remote add upstream https://github.com/a2aproject/A2A
   git fetch upstream main
   git checkout --orphan v1-baseline upstream/main
   ```

4. Migration reference: upstream `docs/whats-new-v1.md` (v0.3 → v1.0) and `CHANGELOG.md`.
   Note that v0.1 → v0.3 changes are *not* covered there; that hop has to be read from
   the v0.2/v0.3 spec history.

## 6. How this review was produced

```bash
git fetch https://github.com/google/A2A main --depth=200
git rev-list --left-right --count HEAD...FETCH_HEAD   # 58 / 200
git merge-base HEAD FETCH_HEAD                        # empty
git show FETCH_HEAD:CHANGELOG.md
git show FETCH_HEAD:docs/whats-new-v1.md
git show FETCH_HEAD:specification/a2a.proto
```
