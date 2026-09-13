# Upstream review diagrams

Diagrams accompanying [`../UPSTREAM_REVIEW.md`](../UPSTREAM_REVIEW.md). They describe the A2A
protocol as it exists upstream at `a2aproject/A2A` `6d6640c` (release `v1.0.1`), not the v0.1
protocol this fork implements.

| Source | Artifact | Shows |
|---|---|---|
| `a2a-v1-architecture.json` | `a2a-v1-architecture.html` | Discovery, the three protocol bindings, and the three result-delivery paths |
| `a2a-v1-task-sequence.json` | `a2a-v1-task-sequence.html` | A full task interaction from Agent Card fetch to completed task |
| `a2a-v1-task-lifecycle.json` | `a2a-v1-task-lifecycle.html` | The nine `TaskState` values from `specification/a2a.proto` |

Each `.html` is a standalone, self-contained page (dark/light themes, guided views, PNG/SVG
export) — open it directly in a browser.

## Regenerating

Built with the [archify](https://github.com/tt-a1i/archify) skill (MIT), v2.17:

```bash
node bin/archify.mjs deliver <type> <source.json> <output.html> --quality showcase --json
```

All three pass showcase validation with 9/9 artifact checks, 0 errors and 0 warnings. Edit the
`.json` source, never the generated `.html`.
