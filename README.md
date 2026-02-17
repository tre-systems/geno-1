# Geno-1: Generative Music Visualizer

<div align="center">

![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=for-the-badge&logo=webassembly&logoColor=white)
![WebGPU](https://img.shields.io/badge/WebGPU-005A9C?style=for-the-badge&logo=gpu&logoColor=white)

[![CI](https://github.com/rgilks/geno-1/actions/workflows/ci.yml/badge.svg)](https://github.com/rgilks/geno-1/actions/workflows/ci.yml)

</div>

<div align="center">
 <img src="/docs/screenshot.png" alt="geno-1 Screenshot" width="902" />
  <br />
  <a href='https://ko-fi.com/N4N31DPNUS' target='_blank'><img height='36' style='border:0px;height:36px;' src='https://storage.ko-fi.com/cdn/kofi2.png?v=6' border='0' alt='Buy Me a Coffee at ko-fi.com' /></a>
  <hr />
</div>

### Project Status (v1.2 - A Grade)

**🎵 Advanced Audio Engine:**

- 3-voice polyphonic system with configurable parameters (trigger probability, octave offset, duration)
- Complete musical alphabet support (A-G keys) with 7 diatonic modes (1-7 keys)
- **Microtonality system**: global detune (±200¢), alternative tuning systems (19-TET, 24-TET, 31-TET)
- Professional spatial audio: per-voice `PannerNode` with real-time 3D positioning
- Master effects chain: convolution reverb, dark feedback delay, saturation, per-voice sends
- Gesture-based audio unlock with professional start overlay

**🎨 Immersive Visuals:**

- Ambient waves background with voice-reactive displacement and proximity effects
- Advanced post-processing: HDR bright pass, separable blur, ACES tonemap, vignette, film grain
- Pointer-driven swirl distortion with inertial physics and exponential falloff
- Click ripple propagation with configurable timing and amplitude
- Real-time performance monitoring with FPS measurement

**🎮 Interactive Controls:**

- Comprehensive keyboard mapping: A-G (root), 1-7 (mode), R (regenerate), T (random), Space (pause)
- Voice interaction: click (mute), Alt+click (solo), Shift+click (reseed), drag (spatial position)
- Tempo (←/→), volume (↑/↓), fullscreen (Enter/Escape) with dynamic BPM display
- Ray-picking system for precise voice positioning with visual feedback

**🛠️ Professional Quality:**

- 31 comprehensive tests including property-based testing for mathematical functions
- Zero compilation warnings with strict linting (`clippy -D warnings`)
- Enhanced error handling with user-friendly WebGPU failure messages
- Professional CI/CD with automated testing, performance validation, and deployment

### Demo

- Local: see Run (Web) below. After `npm run dev`, open `http://localhost:8787`.
- Hosted: [https://geno-1.tre.systems/](https://geno-1.tre.systems/)

### Requirements

- Node 20+
- Rust (stable, 2021 edition)
- wasm-pack (install: `curl -sSfL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh`)
- Desktop browser with WebGPU enabled

Notes:

- WebGL fallback is intentionally avoided; WebGPU is required.
- If audio does not start, click the Start overlay.
- Input coordinates: canvas UV origin is top-left (uv.y = 0 at top). Pointer-driven swirl and click ripple use this convention.

### Run

- `npm run dev` (builds, serves at http://localhost:8787, and opens the browser)

Additional scripts:

- `npm run clean` (removes build artifacts)
- `npm run nuke` (full reset: removes node_modules, reinstalls, and runs dev)
- `npm run deps` (check for dependency updates)
- `npm run deps:update` (update dependencies and run nuke)

**Quick Controls:**

**🎹 Musical Controls:**

- **A-G**: Set root note (complete musical alphabet)
- **1-7**: Select diatonic mode (Ionian, Dorian, Phrygian, Lydian, Mixolydian, Aeolian, Locrian)
- **8-0**: Alternative tuning systems (8=19-TET, 9=24-TET, 0=31-TET pentatonic)
- **R**: Regenerate all voice sequences
- **T**: Random root note + mode combination

**🎵 Microtonality Controls:**

- **,**: Decrease global detune by 50¢ (Shift+, for 10¢ fine adjustment)
- **.**: Increase global detune by 50¢ (Shift+. for 10¢ fine adjustment)
- **/**: Reset detune to 0¢

**🎛️ Playback Controls:**

- **Space**: Pause/resume playback
- **←/→**: Adjust tempo (BPM shown in hint overlay)
- **↑/↓**: Adjust master volume
- **M**: Mute/unmute master output
- **Enter/Escape**: Toggle fullscreen

**🎯 Voice Interaction:**

- **Click voice**: Toggle mute (shows "muted" in hint)
- **Alt+Click**: Solo voice (mutes others)
- **Shift+Click**: Reseed voice sequence
- **Drag voice**: Reposition in 3D space (spatial audio feedback)

**🎨 Visual Effects:**

- **Mouse movement**: Creates trailing swirl distortion with inertial physics
- **Click canvas**: Generates ripple effects that propagate outward
- **Corner proximity**: Affects master saturation (clean ↔ distorted) and delay emphasis

### Pre-commit Check

- Run all checks and tests locally: `npm run check`
  - Rust: `cargo fmt --check`, `cargo clippy` (deny warnings), `cargo test` (workspace)
  - Web: build, serve, and execute the headless browser test

### Git hooks (local safety checks)

This repo uses native Git hooks in `.githooks` (no Husky dependency). Enable them once per clone:

```bash
git config core.hooksPath .githooks
```

Or run the convenience script (also ensures hooks are executable):

```bash
npm run setup
```

Hooks provided:

- `pre-commit`: runs fast Rust checks (`npm run check:rust`) to keep commits quick
- `pre-push`: runs the full project check (`npm run check`) before code leaves your machine

The full check enforces Rust fmt/clippy, runs unit tests, builds the web bundle, serves it, and executes the headless Puppeteer test. If any step fails, Git aborts the commit/push.

### Deploy (Cloudflare Workers)

This repo is configured to deploy via Cloudflare Workers; cache-control headers for static assets are set in `worker.js`.

- Build: `npm run build`
- Deploy: `npx --yes wrangler deploy`
  - Config: `wrangler.toml` (assets directory is `dist/`)
  - Build populates `dist/` with only production runtime files: `index.html` and `pkg/{app_web.js, app_web_bg.wasm, env.js}`
- Worker sets cache-control headers for `.wasm`/`.js`/HTML

#### Build cache-busting

- The build generates `pkg/env.js` with a `version` derived from the current git commit (short SHA, with CI env fallbacks). `index.html` appends `?v=<version>` to the dynamic import of `app_web.js` to ensure deterministic cache busting across deploys.

Headless test:

- `npm run ci` builds, serves, and runs a Puppeteer test locally

### Continuous Integration

- GitHub Actions workflow runs on push/PR:
  - Builds the web bundle and executes the headless browser test
  - On push to `main`, deploys to Cloudflare Workers via Wrangler
  - Requires repo secrets: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
  - Workflow file: `.github/workflows/ci.yml`
  - CI tolerates missing WebGPU in headless by skipping engine-coupled assertions

### Live Demo

- Use the hosted app: [`https://geno-1.tre.systems/`](https://geno-1.tre.systems/)

Notes:

- Keeping screenshots/GIFs up to date is intentionally avoided; refer to the live app instead.

### Project Structure

**🦀 Core Rust/WASM Application:**

- `src/lib.rs`: Main WASM entry point and application initialization
- `src/core/music.rs`: Generative music engine with configurable voice parameters
- `src/audio.rs`: Web Audio API integration and spatial audio management
- `src/render.rs`: WebGPU rendering orchestration and pipeline management
- `src/render/`: Specialized rendering modules (waves, post-processing, targets)
- `src/events/`: Input handling (keyboard, pointer) with comprehensive key mappings
- `src/frame.rs`: Animation loop and GPU state management

**🌐 Web Frontend:**

- `index.html`: Main application entry with canvas and overlay UI
- `worker.js`: Cloudflare Workers deployment with cache-control headers
- `shaders/`: WGSL shaders for ambient waves and post-processing effects

**🔧 Development & Deployment:**

- `package.json`: Node.js build scripts and development dependencies
- `Cargo.toml`: Rust dependencies with WebGPU and Web Audio features
- `.github/workflows/ci.yml`: Comprehensive CI/CD with testing and deployment
- `tests/`: Comprehensive test suite with property-based testing
- `web-test.js`: End-to-end browser testing with Puppeteer

**📚 Documentation:**

- `docs/SPEC.md`: Comprehensive technical specification and S-tier roadmap
- `docs/TODO.md`: Strategic development roadmap and implementation priorities

### Docs

- Project Spec: `docs/SPEC.md`
- Project TODO: `docs/TODO.md`
