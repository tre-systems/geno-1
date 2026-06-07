# Project Rules (Agent-Agnostic)

This file is intentionally tool-neutral and should be usable by both GPT Codex and Claude Code.

## Mission

- Keep the current stack: Rust + WebAssembly + WebGPU + WebAudio + Node tooling.
- Make changes that improve clarity, reliability, and creative distinctiveness.
- Preserve Geno-1's identity: an ambient generative instrument built around three
  spatialised voices, a pointer-driven swirl field, and click ripples. It is the
  foundational Geno instrument and a sibling to Geno-2, not a copy of it.

## Engineering Standards

- Prefer small, reviewable changes over large rewrites.
- Keep code understandable; split functions/modules when complexity grows.
- Remove dead code and stale comments.
- Add comments only where non-obvious logic needs context.
- Do not introduce unrelated refactors during focused fixes.

## Stack and Architecture

- WebGPU via `wgpu` is required; do not add a WebGL fallback unless explicitly requested.
- Keep Rust logic host-testable where practical (engine, keymap, picking live in plain
  modules; browser-only code is gated to WASM).
- Keep browser-specific behaviour gated to WASM/web modules.
- Avoid adding new runtime dependencies without clear benefit.
- Any stack change (language/runtime/framework/deploy platform) requires explicit user
  approval and a short rationale in the change summary.

## Validation Paths

Use the smallest reliable gate during development, then run the full gate before push:

- Fast path (small/local change): `npm run check:rust`
- Full path (behaviour/audio/render/input/deploy changes): `npm run check`

Expected checks:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- production wasm build (`wasm-pack`)
- browser integration test (`web-test.js`, Puppeteer)

The web test tolerates a headless environment without WebGPU by skipping engine-coupled
assertions, so it stays green in CI.

## Audio/Visual Direction

- Maintain a coherent visual identity across `shaders/`, post-processing, and overlay styling.
- Keep interaction responsive; avoid effects that make controls feel laggy.
- Validate that audio changes still preserve reliable browser unlock behaviour (the Start
  overlay / first gesture must resume the `AudioContext`).

## UX Regression Guards

- Do not break existing keyboard/pointer controls unless explicitly requested.
- Keep the help panel behaviour stable (`H` toggle, close/reopen flows).
- Keep `web-test.js` green before push.

## Documentation

- Update docs only when behaviour, controls, architecture, or deployment expectations change.
- Docs describe the current state in the present tense; keep history in git, not in prose.
- Typical targets:
  - `README.md`
  - `docs/SPEC.md` (when architecture/intent changes)
  - `docs/ARCHITECTURE.md` (when code structure/patterns change)
  - `docs/TODO.md` (when priorities change)

## Git Workflow

- Use clear commit messages that describe user-visible intent.
- Do not rewrite history unless explicitly requested.
- Keep `main` deploy-safe (green checks before push); pushes to `main` deploy to Cloudflare.
