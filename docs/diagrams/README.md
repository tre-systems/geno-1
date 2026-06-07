# Diagrams

Graphviz / DOT sources plus rendered PNGs. The `.dot` files are the source of truth; the PNGs are
committed for in-browser viewing on GitHub.

## Files

| Diagram | Source | Rendered |
| --- | --- | --- |
| System overview | `system-overview.dot` | `system-overview.png` |
| Frame pipeline (per-frame ordered systems) | `frame-pipeline.dot` | `frame-pipeline.png` |
| Audio graph (Web Audio signal flow) | `audio-graph.dot` | `audio-graph.png` |
| Render pipeline (WebGPU passes) | `render-pipeline.dot` | `render-pipeline.png` |

## Reading Order

1. **System overview** for the whole Browser / `app-web` (WASM) / Cloudflare shape.
2. **Frame pipeline** for how one `requestAnimationFrame` tick drains input and runs the ordered systems.
3. **Audio graph** when touching the Web Audio routing, FX buses, or note triggering.
4. **Render pipeline** when touching the WebGPU passes, bloom, or post-processing.

## Conventions

Mermaid is used for small inline diagrams; Graphviz/DOT is used here for the larger graphs. Color
coding by domain:

- **Green** — host-testable core (engine, key/command maps, picking) and input that only enqueues.
- **Amber** — the per-frame `requestAnimationFrame` loop and its ordered systems.
- **Teal** — the Web Audio node graph (voices, panners, FX buses, the note pool).
- **Purple** — WebGPU passes and WGSL shaders; cylinders are GPU render targets.
- **Blue** — browser surface (canvas, DOM, events), hosting, and external endpoints.
- Diamonds — decisions.
- Bold green outline — a terminal output (e.g. `AudioContext.destination`, swapchain present).
- Dashed grey edges — secondary or optional relationships (sends, taps, error paths).

Fonts: Avenir. Rendered at 220 DPI.

## Render

```
npm run diagrams          # render all .dot files to PNG next to the source
npm run check:diagrams    # verify each .dot renders cleanly and the PNG exists
```

Both scripts assume Graphviz is on PATH (`brew install graphviz`). CI installs Graphviz before
`npm run check`. On a local machine without `dot`, `npm run check:diagrams` skips with a clear message;
regenerate the PNGs with `npm run diagrams` before committing diagram changes.

To render one manually:

```
dot -Tpng:cairo docs/diagrams/<name>.dot -Gdpi=220 -o docs/diagrams/<name>.png
```
