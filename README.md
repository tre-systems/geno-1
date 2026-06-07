# Geno-1: Generative Music Visualizer

<div align="center">

![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=for-the-badge&logo=webassembly&logoColor=white)
![WebGPU](https://img.shields.io/badge/WebGPU-005A9C?style=for-the-badge&logo=gpu&logoColor=white)
[![CI](https://github.com/tre-systems/geno-1/actions/workflows/ci.yml/badge.svg)](https://github.com/tre-systems/geno-1/actions/workflows/ci.yml)

</div>

<div align="center">
  <img src="docs/screenshot.png" alt="geno-1 screenshot" width="902" />
</div>

Geno-1 is a generative audiovisual instrument built with Rust + WebAssembly + WebGPU + WebAudio.
Three spatialised voices improvise over selectable scales and tunings while an ambient wave field
reacts to the music and to pointer gestures. It is the foundational Geno instrument; its sibling
[Geno-2](https://github.com/tre-systems/geno-2) takes the same stack in a different artistic direction.

- Live: [https://geno-1.tre.systems/](https://geno-1.tre.systems/)
- [Buy me a coffee](https://ko-fi.com/N4N31DPNUS)

## Highlights

- Three-voice generative engine (sine / saw / triangle) on an eighth-note grid, with per-voice
  trigger probability, octave offset, and note duration.
- Full diatonic mode set (`A`–`G` roots, Ionian through Locrian) plus microtonal tunings: global
  ±200¢ detune and 19/24/31-TET pentatonic scales.
- Spatial audio: per-voice HRTF panners feed a shared convolution reverb, feedback delay, and
  saturation bus; send levels follow each voice's position in 3D space.
- WebGPU wave field: layered noise sheets with voice-reactive displacement, a pointer-driven swirl
  with inertial physics, and click ripples.
- HDR post-processing: bright-pass bloom, separable blur, ACES tonemap, vignette, and film grain.
- Host-tested core (36 tests) plus a headless browser smoke test; `clippy -D warnings` clean.

## Stack

- Rust 2021, single `app-web` crate
- WebAssembly (`wasm-pack`)
- WebGPU (`wgpu`, WGSL shaders) — required, no WebGL fallback
- WebAudio (procedural synthesis + FX graph)
- Cloudflare Workers static hosting (`wrangler`)

## Controls

The Start overlay lists every binding; press `H` at any time to toggle it.

**Keyboard**

- `A`–`G`: set root note
- `1`–`7`: set diatonic mode (Ionian, Dorian, Phrygian, Lydian, Mixolydian, Aeolian, Locrian)
- `8` / `9` / `0`: select 19 / 24 / 31-TET pentatonic
- `P`: reset to C Major pentatonic
- `R`: regenerate all voice sequences
- `T`: randomise root + mode
- `Space`: pause / resume
- `,` / `.`: detune ±50¢ (hold `Shift` for ±10¢ fine steps)
- `/`: reset detune to 0¢
- `←` / `→`: tempo (BPM, clamped 40–240)
- `↑` / `↓`: master volume
- `M`: mute / unmute master
- `Enter` / `Esc`: enter / exit fullscreen
- `H`: toggle the help panel

**Pointer**

- Click empty space: play a one-shot note (pitch from horizontal position, velocity from vertical) and spawn a ripple
- Click a voice: toggle mute
- `Alt`+click a voice: solo
- `Shift`+click a voice: reseed that voice's sequence
- Drag a voice: reposition it on the floor plane; spatial audio follows
- Move the pointer: drive the swirl distortion field

## Requirements

- Node 20+
- Rust (stable, 2021 edition)
- `wasm-pack` (`curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`)
- A WebGPU-capable browser (Chrome / Edge 113+). If audio does not start, click the Start overlay.

## Local Development

- `npm install`
- `npm run dev` — builds, serves at <http://localhost:8787>, and live-reloads

Additional scripts:

- `npm run clean` — remove build artifacts
- `npm run nuke` — full reset (remove `node_modules`, reinstall, run dev)
- `npm run deps` / `npm run deps:update` — check / apply dependency updates

## Quality Gate

`npm run check` runs the full gate (also enforced by the git hooks below):

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- Graphviz diagram render check (`check:diagrams`)
- production wasm build
- headless browser integration test (`web-test.js`, Puppeteer)

For quick local iteration, `npm run check:rust` runs the Rust checks only.

## Git Hooks

This repo uses native Git hooks in `.githooks` (no Husky). Enable them once per clone:

```bash
npm run setup   # or: git config core.hooksPath .githooks
```

- `pre-commit`: fast Rust checks (`npm run check:rust`)
- `pre-push`: the full project check (`npm run check`)

## Deployment

Deployed to Cloudflare Workers as static assets.

- Build: `npm run build` — populates `dist/` with `index.html`, `favicon.svg`, and
  `pkg/{app_web.js, app_web_bg.wasm, env.js}`
- Deploy: `npx --yes wrangler deploy` (config in `wrangler.toml`)
- `worker.js` sets cache-control headers; `pkg/env.js` carries a git-SHA version that `index.html`
  appends to the wasm entry (`app_web.js?v=<version>`) for deterministic cache-busting.

CI (`.github/workflows/ci.yml`) runs `npm run check` on every push/PR and deploys on push to `main`
when `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` are configured.

## Project Structure

- `src/core/music.rs` — generative music engine (voices, scales, scheduling)
- `src/audio.rs` — WebAudio graph and spatial routing
- `src/render.rs`, `src/render/` — WebGPU pipeline (waves, bloom, post-processing, targets)
- `src/events/` — keyboard and pointer input
- `src/frame.rs` — animation loop, swirl physics, FX modulation
- `src/wasm_app.rs` — WASM entry point and initialisation
- `shaders/` — WGSL shaders (`waves.wgsl`, `post.wgsl`)
- `index.html`, `worker.js`, `wrangler.toml` — web front-end and deployment

## Docs

- Architecture, patterns & pipelines: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Diagrams (Graphviz): [`docs/diagrams/`](docs/diagrams/README.md)
- Backlog: [`docs/TODO.md`](docs/TODO.md)
