# Geno-1 Video Production

`npm run produce` creates a finished Drift Lattice render:

- deterministic arrangement from one seed and duration;
- browser-captured WebGPU canvas;
- matching offline-rendered, mastered WebAudio mix from the same seed;
- burned-in start/end captions as plain black text with transparent background;
- high-bitrate MP4 output with AAC audio, plus the separate 24-bit WAV master.

For release work, treat the MP4 as the video upload master and the WAV as the
SoundCloud/audio master. Do not extract audio back out of the MP4 for SoundCloud;
that would use the AAC transcode instead of the lossless render.

## One-Command Render

```bash
npm run produce -- --seed 7 --duration 600
```

Defaults:

| Setting | Default |
| --- | --- |
| Duration | `600` seconds |
| Resolution | `3840x2160` |
| Frame rate | `60` fps |
| Audio target | `-16` LUFS |
| Output directory | `renders/proofs/` |

Typical output:

```text
renders/proofs/geno-drift-lattice-7.mp4
renders/proofs/geno-drift-lattice-7.wav
renders/proofs/geno-drift-lattice-7-preview.png
renders/proofs/geno-drift-lattice-7-chunks/
```

Useful flags:

```bash
npm run produce -- \
  --seed 7 \
  --duration 600 \
  --width 3840 \
  --height 2160 \
  --fps 60 \
  --lufs -16 \
  --bitrate 80000000 \
  --video-codec hevc \
  --label geno-drift-lattice-7 \
  --out-dir renders/proofs
```

Caption flags are passed through:

```bash
--start-title "Geno-1: Drift Lattice"
--start-subtitle "a generative sound sculpture for three spatial voices"
--end-title "Geno-1: Drift Lattice"
--end-subtitle "Multivibrator - @mvbrtr"
```

## Required Tools

- Node and lockfile npm dependencies.
- Rust, `wasm-pack`, and the wasm target.
- Chrome or Chromium for canvas capture.
- `ffmpeg` and `ffprobe`.
- `rsvg-convert` from librsvg for transparent caption overlays.

## Pipeline

1. `scripts/produce.mjs` builds `dist/` and serves it locally.
2. `scripts/capture-canvas-video.mjs --produce --compose <seed>` opens Chrome,
   hides the UI overlays, starts `window.geno.startArrangement(duration, seed)`,
   captures `#app-canvas`, and writes WebM chunks.
3. The same page calls `window.geno.renderPieceTo(...)`, which runs the WASM
   offline render and pure Rust mastering path.
4. `ffmpeg` muxes video and WAV.
5. `scripts/add-video-captions.mjs` burns in the title/credit captions.

The capture is real-time; a 10-minute piece takes roughly 10 minutes plus build,
offline audio, mux, and caption time.

## Release Targets

For a high-quality 4K release render:

```bash
npm run produce -- \
  --seed 7 \
  --duration 600 \
  --width 3840 \
  --height 2160 \
  --fps 60 \
  --lufs -16 \
  --bitrate 80000000 \
  --video-codec hevc \
  --label geno-drift-lattice-7 \
  --out-dir renders/release
```

Artifacts:

- `renders/release/geno-drift-lattice-7.mp4` — 4K video upload master with captions and AAC audio.
- `renders/release/geno-drift-lattice-7.wav` — 48 kHz / 24-bit mastered stereo WAV for SoundCloud.
- `renders/release/geno-drift-lattice-7-preview.png` — first-frame inspection still.

The default capture bitrate is 80 Mbps for 4K. YouTube's current SDR 2160p/60
recommendation is 53-68 Mbps, so `80000000` is intentionally above the published
range to leave headroom for the re-encode. Use `--bitrate 68000000` for a smaller
upload that still sits at the top of the official range.

Geno's WAV master is already a good SoundCloud source: lossless stereo, 48 kHz,
24-bit, `-16 LUFS`, and a `-1 dBTP` true-peak ceiling. SoundCloud recommends
lossless uploads such as WAV/FLAC/AIFF/ALAC and asks for roughly `-0.5` to
`-1 dBFS` headroom to avoid clipping during transcoding.

Upload this file to SoundCloud:

```text
renders/release/geno-drift-lattice-7.wav
```

If SoundCloud rejects a long or unusual WAV, make a conservative compatibility
copy and upload that instead:

```bash
ffmpeg -hide_banner -y \
  -i renders/release/geno-drift-lattice-7.wav \
  -af aresample=44100:resampler=soxr:dither_method=triangular_hp \
  -c:a pcm_s16le \
  renders/release/geno-drift-lattice-7-soundcloud-16_44.wav
```

References:

- <https://support.google.com/youtube/answer/1722171>
- <https://help.soundcloud.com/hc/en-us/articles/115003452847-Uploading-requirements>
- <https://help.soundcloud.com/hc/en-us/articles/360039171614-Upload-Requirements>
