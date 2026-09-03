#!/usr/bin/env node
import { spawn } from "node:child_process";
import { take, hasFlag, passArg, run } from "./cli.mjs";
import { ensureChrome, ensureExecutable, findChrome } from "./preflight.mjs";
import { startStaticServer } from "./serve-static.mjs";

function usage() {
  console.error(`Usage:
  npm run produce -- --seed 7 --duration 600 [options]

Options:
  --seed <N>         deterministic arrangement seed (default 1)
  --duration <sec>  composed-piece duration before reverb tail (default 600)
  --width <px>      capture width (default 3840)
  --height <px>     capture height (default 2160)
  --fps <N>         capture frame rate (default 60)
  --label <name>    output label (default geno-drift-lattice-<seed>)
  --port <N>        local static-server port (default 8787)
  --chrome <path>   Chrome/Chromium executable
  --no-build        use an already-built dist/
  --no-headless     show Chrome during capture
  --video-codec auto|h264|hevc|libx264|hevc_videotoolbox
`);
}

function runStreaming(command, commandArgs) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, commandArgs, { stdio: "inherit" });
    child.on("error", (error) => reject(new Error(`${command} ${commandArgs.join(" ")} failed to start: ${error.message}`)));
    child.on("close", (status) => {
      if (status === 0) resolve();
      else reject(new Error(`${command} ${commandArgs.join(" ")} failed (status ${status})`));
    });
  });
}

if (hasFlag("--help")) {
  usage();
  process.exit(0);
}

const seed = take("--seed", "1");
const duration = take("--duration", "600");
const width = take("--width", "3840");
const height = take("--height", "2160");
const fps = take("--fps", "60");
const label = take("--label", `geno-drift-lattice-${seed}`);
const port = Number(take("--port", "8787"));
const chromePath = take("--chrome") || process.env.CHROME || findChrome();

if (!chromePath) {
  throw new Error("Chrome/Chromium not found; set CHROME=/path/to/browser or pass --chrome /path/to/browser");
}
ensureChrome(chromePath);
ensureExecutable("ffmpeg", { args: ["-version"], label: "ffmpeg", installHint: "install ffmpeg (macOS: brew install ffmpeg)" });
ensureExecutable("ffprobe", { args: ["-version"], label: "ffprobe", installHint: "install ffmpeg (macOS: brew install ffmpeg)" });
ensureExecutable("rsvg-convert", { args: ["--version"], label: "rsvg-convert", installHint: "install librsvg (macOS: brew install librsvg)" });

if (!hasFlag("--no-build")) {
  console.log("Building...");
  run("npm", ["run", "build"], { inherit: true });
}

console.log("Serving dist/...");
const { server, url } = await startStaticServer({ dir: "dist", port, cors: true });
try {
  console.log(`Producing ${duration}s Drift Lattice (seed ${seed}) at ${width}x${height}...`);
  await runStreaming("node", [
    "scripts/capture-canvas-video.mjs",
    "--produce",
    "--compose",
    seed,
    "--duration",
    duration,
    "--url",
    url,
    "--width",
    width,
    "--height",
    height,
    "--fps",
    fps,
    "--label",
    label,
    "--chrome",
    chromePath,
    ...["--start-title", "--start-subtitle", "--end-title", "--end-subtitle", "--lufs", "--out-dir", "--bitrate", "--video-codec"]
      .flatMap(passArg),
    ...(hasFlag("--no-headless") ? ["--no-headless"] : []),
  ]);
} finally {
  server.close();
}
