# Geno-1 Backlog

Forward-looking work, roughly in priority order. Current behaviour and architecture are described in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## In-scene controls

- Add interactive 3D control objects so the instrument is playable without the keyboard:
  - Play/pause orb with colour-coded state.
  - Tempo dial (drag to change BPM).
  - Regenerate control to reseed all voices.
  - Scale/tuning selector as in-scene geometry.
- Hover and click feedback: glow, scale, ripple.
- Spatial mixing visuals: visible voice objects, connection lines to the listener, distance-based
  size/brightness, and a clear drag boundary.

## Audio engine

- Synthesis depth: optional FM synthesis, per-voice ADSR envelopes, per-voice lowpass/highpass with
  cutoff automation.
- Generation: configurable scheduling grid (16th notes, triplets, dotted rhythms); pattern memory so
  voices vary previous sequences; gradual key modulation between related keys.
- Just Intonation pentatonic to complete the tuning set.
- AudioWorklet path for sample-accurate synthesis.
- Continuous background-tab audio: drive the scheduler from a Web Worker timer. `setInterval` throttles
  to ~1 Hz when the tab is hidden, so the lookahead scheduler currently goes choppy (not silent) in the
  background; a worker timer keeps it smooth.
- Lower click-to-play latency: schedule click notes immediately on the audio clock instead of via the
  one-frame input command queue.

## Architecture & types

- Give the runtime `Config` tier a consumer (a preset / live-tuning surface) and extend it to the
  remaining tuning groups; until something varies it at runtime, it is indirection ahead of need.
- Generalise beyond three hardcoded voices — the count is baked into the WGSL uniform (`[VoicePacked; 3]`)
  and the pulse arrays (`[f32; 3]`). Needs a dynamic uniform array + `Vec`-based pulses.
- Decompose `FrameContext` (33 fields) into focused sub-states (audio, render, interaction) that systems
  borrow explicitly.
- Capture/restore engine + RNG state for deterministic session replay.
- Extract initialisation and WebGPU pipeline builders into focused submodules.
- Add rustdoc with examples for the public API surface.

## Visuals

- Configurable bloom intensity/threshold.
- Note-triggered particle effects.
- Dynamic per-voice lighting cast into the scene.
- Optional shader-based spectrum/waveform display.
- Adaptive quality that reduces effects on lower-end GPUs.

## Performance

- Reuse GPU buffers and minimise per-frame allocations.
- Minimise JS↔WASM transfers.
- Profile and confirm steady 60 FPS on mid-range desktop GPUs.

## Testing & docs

- Verify cent-level accuracy across all tuning systems.
- Extend the headless test to change tempo and assert the hint reflects the new BPM.
- Add `criterion` benchmarks for the pure engine to inform synthesis / scheduling decisions.
- Migrate the browser smoke test from Puppeteer to Playwright (consistency with the other projects;
  enables visual / snapshot tests).
- Cross-browser WebGPU checks (Chrome / Edge, and Firefox once supported). The app is WebGPU-only by
  design (no WebGL fallback).

## Maintenance

- Upgrade `wgpu` (24 → 29, ~5 majors behind), `rand` (0.8 → 0.9), and `glam`. The `wgpu` jump is mostly
  mechanical given the app only uses uniform buffers, but spans several breaking releases.
