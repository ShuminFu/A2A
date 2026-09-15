# A2A v1.0 diagrams

Typed diagram sources for the upstream review in [`UPSTREAM_REVIEW.md`](../../UPSTREAM_REVIEW.md).
Both describe **upstream A2A v1.0**, not the v0.1-era protocol this fork snapshots.

| Source | Type | Shows |
|---|---|---|
| `a2a-v1-surface.architecture.json` | architecture | Discovery, the three equivalent bindings, task state, streaming and push delivery |
| `a2a-v1-task.lifecycle.json` | lifecycle | The `TASK_STATE_*` state machine: forward path, interrupted states, terminal states |

## Rendering

These are [archify](https://github.com/tt-a1i/archify) (MIT) specifications. They compile to
self-contained interactive HTML — dark/light themes, pan/zoom, search, relationship tracing,
and PNG/SVG/WebM export — with no runtime dependency on the renderer:

```bash
npx skills add tt-a1i/archify -g     # or clone the repo
node bin/archify.mjs deliver architecture <path>/a2a-v1-surface.architecture.json out.html --quality showcase
node bin/archify.mjs deliver lifecycle    <path>/a2a-v1-task.lifecycle.json      out.html --quality showcase
```

Both sources pass archify's `showcase` profile: 9/9 artifact checks, 0 composition errors,
0 warnings. Source facts come from upstream `specification/a2a.proto`,
`docs/whats-new-v1.md` and `docs/topics/life-of-a-task.md` at `a2aproject/A2A@6d6640c`.

## Known rendering note

The lifecycle artifact contains its width at every checked desktop viewport but exceeds the
viewport *height* below 2048x1320 (1440x900 overflows by 339px), so the page scrolls
vertically. Three lanes of states plus conclusion cards do not fit one 900px-tall screen at the
viewer's chosen reading width; the diagram itself is unclipped and fully readable. The
architecture artifact is fully contained at 1440x900, 1600x1000, 1920x1080 and 2048x1320.
