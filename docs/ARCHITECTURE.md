# Architecture Guide

This document explains how Geno-1's code is organized and the set of patterns that explain most of
it. For *what* the system does and the audio/visual pipelines, see [`SPEC.md`](SPEC.md).

## System Overview

Geno-1 is a single Rust crate (`app-web`) compiled to WebAssembly. A `requestAnimationFrame` loop
([`frame.rs`](../src/frame.rs)) advances a deterministic music engine, synthesises any new notes
through a Web Audio graph, modulates global effects from pointer gestures, updates per-voice spatial
audio, and renders an audio-reactive wave field with WebGPU. The core logic (engine, key maps,
picking) is plain host-testable Rust; everything browser-facing is gated to the wasm target.

## Core Patterns

Geno-1 is small, but it leans on a consistent set of patterns. Knowing these explains most of the
code, and new code should fit one of them rather than inventing a parallel mechanism.

### The engine core (`core/`, `input`, `events::keymap`)

- **Host-testable core, wasm-gated shell.** [`lib.rs`](../src/lib.rs) exports `core`, `events`, and
  `input` unconditionally and gates everything browser-facing (`audio`, `render`, `frame`,
  `wasm_app`, `constants`, …) behind `#[cfg(target_arch = "wasm32")]`. Pure logic — the music engine,
  key tables, ray-picking — compiles and is unit-tested on the host; Web Audio / WebGPU / DOM code
  never leaks into it. **New logic that can be expressed without the browser belongs in
  `core`/`input`/`keymap` so it stays testable.**
- **Deterministic, seeded engine.** [`MusicEngine`](../src/core/music.rs) owns per-voice `StdRng`s
  derived from one base seed by hash-mixing (`seed ^ i*0x9E37…`), so a seed reproduces the music and
  voices reseed independently. No wall-clock, no I/O in the engine.
- **Fixed-timestep accumulator.** [`MusicEngine::tick(dt)`](../src/core/music.rs) advances
  `beat_accum += dt` and drains it in fixed eighth-note `step`s (`while beat_accum >= step`), so
  scheduling is frame-rate independent and reproducible. The host owns the clock and feeds elapsed `dt`.
- **Plain structs, not an ECS.** The voice set is tiny and fixed, so the engine holds `Vec<VoiceState>`
  + `Vec<VoiceConfig>` as plain fields and iterates them in `schedule_step`.
- **Pure functions for math/lookup.** [`midi_to_hz`](../src/core/music.rs),
  [`ray_sphere` / `nearest_index_by_uvx`](../src/input.rs),
  [`screen_to_world_ray`](../src/camera.rs), and
  [`root_midi_for_key` / `mode_scale_for_digit`](../src/events/keymap.rs) are pure; the 31 host tests
  target exactly these.

### WASM runtime & shared state

- **`wasm-bindgen` facade.** [`wasm_app::start`](../src/wasm_app.rs) is the only
  `#[wasm_bindgen(start)]` surface; it builds the graph and hands off to the frame loop. JS holds no
  application state.
- **Aggregate / parameter-object structs.** Per-frame state and resources are bundled into one
  [`FrameContext`](../src/frame.rs) instead of being threaded individually; likewise
  [`FxBuses` / `VoiceRouting`](../src/audio.rs) and [`InputWiring`](../src/events/pointer.rs). One
  struct in, one `frame()` method out.
- **Interior mutability with scoped borrows.** Shared state (`engine`, `paused`, `pulses`,
  `hover_index`, `drag_state`, `queued_ripple_uv`) is `Rc<RefCell<_>>` shared between the RAF loop and
  event closures. Borrows are deliberately scoped and dropped before re-borrowing or calling out
  (`drop(ms)`, block-scoped `borrow_mut`, `.take()`) — the single-threaded discipline that keeps
  `RefCell` from panicking.
- **Closure-and-`forget` event wiring.** Every listener follows `Closure::wrap(Box::new(move |ev| …))`
  → `add_event_listener_with_callback` → `closure.forget()` for a `'static` lifetime; the RAF loop is a
  self-rescheduling `Rc<RefCell<Option<Closure>>>` ([`start_loop`](../src/frame.rs)).
- **Optional subsystems degrade gracefully.** `gpu: Option<GpuState>` and
  `analyser: Option<AnalyserNode>` let the app run (and the headless test pass) without WebGPU or an
  analyser; [`init_gpu`](../src/frame.rs) returns `None` and surfaces a DOM message instead of panicking.
- **Once-guard and module singletons.** A `static STARTED: AtomicBool` guards one-time init; a
  `thread_local! MASTER_UNMUTED_GAIN` remembers pre-mute gain ([`keyboard.rs`](../src/events/keyboard.rs)).

### Audio graph (`audio.rs`)

- **Construction via factories returning bundle structs.** [`build_fx_buses`](../src/audio.rs) →
  `FxBuses`, [`wire_voices`](../src/audio.rs) → `VoiceRouting`, and [`create_analyser`](../src/audio.rs)
  build the Web Audio graph once and return a struct of the nodes the frame loop later modulates.
- **Fire-and-forget JS calls (`_ = …`).** Node `connect` / `set_value` / ramp calls return `Result`s
  whose failure is non-fatal; they are discarded with `_ = …`, reserving real handling for construction.
- **Errors surfaced at construction boundaries.** `init()` uses `anyhow::Result` + `?` and shows a
  user-facing DOM message on failure; the `build_*` / `wire_*` factories return `Result<_, ()>` and
  abort init with a logged message. Per-frame code never returns `Result`.

### Rendering (`render/`)

- **Resource-bundle structs + factories** (the GPU mirror of the audio side):
  [`WavesResources` / `create_waves_resources`](../src/render/waves.rs), `PostResources`,
  `RenderTargets`, with shared builders [`create_color_texture` / `make_post_pipeline`](../src/render/helpers.rs).
- **`#[repr(C)]` Pod uniforms.** GPU-facing data is `bytemuck::Pod` / `Zeroable` (`WavesUniforms`,
  `VoicePacked`) uploaded straight to uniform buffers ([`waves.rs`](../src/render/waves.rs)).
- **Ping-pong offscreen targets.** Bloom runs HDR → half-res `bloom_a` / `bloom_b` ping-pong, recreated
  on resize (`RenderTargets`). The full pass list is in [`SPEC.md`](SPEC.md).

### Input

- **Pure key tables, effectful handlers.** [`keymap.rs`](../src/events/keymap.rs) is pure lookup
  (host-tested); [`keyboard.rs`](../src/events/keyboard.rs) and [`pointer.rs`](../src/events/pointer.rs)
  apply the effects. Picking math (`ray_sphere`) is pure; the pointer handler wires it to engine + audio.
- **One-slot command queue for deferred input.** A tap stores `queued_ripple_uv = Some(uv)`; the frame
  loop consumes it with `.take()` and forwards it to the GPU, decoupling the event from the render.

### Tuning

- **Centralized tuning constants.** [`constants.rs`](../src/constants.rs) holds the named runtime tuning
  values (decay τ, swirl spring, FX/send weights, click-to-note mapping, analyser response) so behavior
  is tuned in one place. Audio-graph *construction* values (sample rate, node defaults, IR length) stay
  local to [`audio.rs`](../src/audio.rs) on purpose — they describe the graph's shape, not its tuning.

## Consistency Notes

Places where the code does not yet follow the patterns above. None are bugs; they are where new work
should converge.

- **`T` (random root + mode) bypasses the seeded engine.** [`keyboard.rs`](../src/events/keyboard.rs)
  calls `js_sys::Math::random()` directly — non-deterministic, browser-only, and untestable — instead
  of routing through a seeded `MusicEngine` method like the rest of the engine.
- **Two note-trigger implementations.** [`audio::trigger_one_shot`](../src/audio.rs) (click notes) and
  an inlined envelope block in [`frame.rs`](../src/frame.rs) (scheduled notes) duplicate the
  oscillator+envelope wiring with slightly different timings; one should call the other.
- **Input isn't uniformly funneled.** Ripples use the one-slot queue, but keyboard and pointer
  otherwise mutate the engine directly from their closures — the command-queue pattern is applied in
  one place rather than as the general input path.
- **Two `window` keydown listeners.** The main handler ([`wire_global_keydown`](../src/events/keyboard.rs))
  and a separate `H`-only listener (`wire_overlay_toggle_h`) both bind `keydown`; folding `H` into the
  main dispatch would leave a single entry point.
- **Two error idioms.** `anyhow::Result` at the init boundary vs. `Result<(), ()>` in the audio
  factories — harmless, but worth standardizing.

## Patterns To Adopt

Patterns used in sibling projects (notably [pongo](https://github.com/tre-systems/pongo)) or simply
worth adding here.

- **Strongly-typed domain newtypes.** pongo names a side with `PlayerId(u8)`; geno-1 passes raw `usize`
  voice indices, `i32` MIDI, and `f32` cents/BPM/Hz. Newtypes (`MidiNote`, `Cents`, `Bpm`, `Hz`,
  `VoiceIndex`) would prevent unit mix-ups and document ranges at the type level. **Top gap.**
- **A runtime `Config` tier.** pongo seeds a cloneable `Config` from `const Params`; geno-1 has only the
  const tier ([`constants.rs`](../src/constants.rs)). A runtime config would enable presets and live
  tuning without rebuilds.
- **A unified input queue.** Funnel all input (keyboard, pointer, future MIDI/touch) through one queue
  the frame loop drains — generalizing the ripple slot and removing direct engine mutation from closures.
- **Named, ordered frame systems.** `FrameContext::frame()` already delegates to helpers
  (`smooth_pulses`, `update_swirl`, `apply_global_fx_swirl`, …); making the pipeline an explicit ordered
  list (pulses → swirl → FX → spatialize → analyser → camera → render) would make the ordering a
  documented contract, like pongo's `step`.
- **Audio-node lifecycle / pooling.** Per-note `OscillatorNode` + `GainNode` are created and left for
  GC; a small pool / polyphony cap with explicit disconnect would bound allocation.

## Codebase Map

| Module | Path | Role | Host-testable |
| --- | --- | --- | --- |
| **core** | [`src/core/`](../src/core/) | Generative engine: voices, scales, seeded scheduling. The heart. | ✅ |
| **input** | [`src/input.rs`](../src/input.rs) | Pure picking math (`ray_sphere`, nearest-by-UV) + pointer state. | ✅ |
| **events** | [`src/events/`](../src/events/) | `keymap` (pure tables, tested) + `keyboard`/`pointer` (wasm handlers). | partial |
| **audio** | [`src/audio.rs`](../src/audio.rs) | Web Audio graph construction and per-voice routing. | wasm-only |
| **render** | [`src/render.rs`](../src/render.rs), [`src/render/`](../src/render/) | WebGPU pipelines: waves, bloom, post, targets. | wasm-only |
| **frame** | [`src/frame.rs`](../src/frame.rs) | RAF loop, swirl physics, FX modulation, spatialization, render. | wasm-only |
| **wasm_app** | [`src/wasm_app.rs`](../src/wasm_app.rs) | `#[wasm_bindgen(start)]` entry; builds the graph, starts the loop. | wasm-only |
| **constants** | [`src/constants.rs`](../src/constants.rs) | Centralized runtime tuning values. | wasm-only |
| **dom / overlay / camera** | [`src/`](../src/) | Canvas sizing, hint/help overlay, view math. | mixed |
