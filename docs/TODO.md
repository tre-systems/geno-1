# Geno-1 Backlog

Forward-looking work, roughly in priority order. Current behaviour and architecture are described in
[`SPEC.md`](SPEC.md).

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
- AudioWorklet path for sample-accurate timing.
- Cap polyphony / pool oscillators and audit Web Audio node lifetimes.

## Architecture & types

- Introduce domain newtypes (`MidiNote`, `Frequency`, `Cents`, `BPM`) with range validation.
- Separate RNG state from engine state to allow deterministic replay.
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
- Extend the headless test to change tempo and assert the hint reflects the new BPM, and to assert a
  voice click toggles its mute state in the hint.
- Cross-browser WebGPU checks (Chrome / Edge, and Firefox once supported).

## Maintenance

- Keep dependencies current (`wgpu` and `rand` are a few versions behind).
