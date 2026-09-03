#!/usr/bin/env node
import { createServer } from "node:http";
import { createWriteStream, mkdirSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import puppeteer from "puppeteer";
import { take, takeNumber, hasFlag, passArg, run } from "./cli.mjs";
import { ensureChrome, ensureExecutable, findChrome } from "./preflight.mjs";

function usage() {
  console.error(`Usage:
  npm run video:capture -- --url http://localhost:8787/ --compose 7 --duration 600 [options]

Options:
  --compose <seed>   play the deterministic Drift Lattice arrangement
  --produce          render matching mastered audio, mux, and add captions
  --reuse-chunks     skip capture and assemble an existing chunk directory
  --out-dir renders/proofs
  --chrome "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
  --width 3840 --height 2160 --fps 60
  --bitrate 80000000
  --video-codec auto|h264|hevc|libx264|hevc_videotoolbox
  --no-headless
  --no-remux
`);
}

function startChunkServer(chunkDir, audioPath) {
  let doneResolve;
  const done = new Promise((resolveDone) => {
    doneResolve = resolveDone;
  });
  let audioResolve;
  const audioDone = new Promise((resolveAudio) => {
    audioResolve = resolveAudio;
  });
  let count = 0;
  const server = createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");
    res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    if (req.method === "OPTIONS") {
      res.writeHead(204);
      res.end();
      return;
    }
    if (req.method !== "POST") {
      res.writeHead(405, { Allow: "POST, OPTIONS" });
      res.end("method not allowed");
      return;
    }
    const parsed = new URL(req.url, "http://127.0.0.1");
    if (parsed.searchParams.get("done") === "1") {
      res.writeHead(200);
      res.end("ok");
      doneResolve(count);
      return;
    }
    if (parsed.searchParams.get("audio") === "1" && audioPath) {
      const out = createWriteStream(audioPath);
      req.pipe(out);
      out.on("finish", () => {
        res.writeHead(200);
        res.end("ok");
        audioResolve(true);
      });
      out.on("error", (error) => {
        res.writeHead(500);
        res.end(String(error));
      });
      return;
    }
    const index = parsed.searchParams.get("i") ?? String(count);
    if (!/^(0|[1-9]\d{0,6})$/.test(index)) {
      res.writeHead(400);
      res.end("invalid chunk index");
      return;
    }
    const file = join(chunkDir, `${index.padStart(6, "0")}.webm.part`);
    const out = createWriteStream(file);
    req.pipe(out);
    out.on("finish", () => {
      count += 1;
      res.writeHead(200);
      res.end("ok");
    });
    out.on("error", (error) => {
      res.writeHead(500);
      res.end(String(error));
    });
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      resolveServer({ server, port, done, audioDone });
    });
  });
}

async function captureChunks(page, opts) {
  const { screenshotPath, composeSeed, durationSec, fps, bitrate, captureDuration, chunkPort } = opts;
  if (composeSeed != null) {
    await page.evaluate(
      async ({ durationSec, composeSeed }) => {
        const started = performance.now();
        while (!window.geno && performance.now() - started < 10000) {
          await new Promise((r) => setTimeout(r, 50));
        }
        while (window.geno?.isReady && !window.geno.isReady() && performance.now() - started < 30000) {
          await new Promise((r) => setTimeout(r, 50));
        }
        if (!window.geno?.isReady?.()) throw new Error("geno was not ready before capture");
        window.geno.startArrangement(durationSec, Number(composeSeed));
      },
      { durationSec, composeSeed },
    );
  }

  await page.screenshot({ path: screenshotPath });
  console.log(`preview: ${screenshotPath}`);

  const postUrl = `http://127.0.0.1:${chunkPort}/chunk`;
  const recorded = await page.evaluate(
    async ({ fps, bitrate, captureDuration, postUrl }) => {
      const canvas = document.getElementById("app-canvas");
      if (!canvas) throw new Error("No #app-canvas");
      const mime = ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"].find((item) =>
        MediaRecorder.isTypeSupported(item),
      );
      if (!mime) throw new Error("No supported MediaRecorder WebM mime type");
      const stream = canvas.captureStream(fps);
      const recorder = new MediaRecorder(stream, { mimeType: mime, videoBitsPerSecond: bitrate });
      let index = 0;
      const uploads = [];
      const fpsSamples = [];
      const fpsTimer = setInterval(() => {
        const value = window.geno?.fps?.() ?? 0;
        if (value > 0) fpsSamples.push(+value.toFixed(1));
      }, 500);
      const stopped = new Promise((resolve, reject) => {
        recorder.onerror = () => reject(recorder.error || new Error("MediaRecorder error"));
        recorder.ondataavailable = (event) => {
          if (event.data && event.data.size > 0) {
            const i = index++;
            uploads.push(fetch(`${postUrl}?i=${i}`, { method: "POST", body: event.data }));
          }
        };
        recorder.onstop = async () => {
          clearInterval(fpsTimer);
          await Promise.all(uploads);
          await fetch(`${postUrl}?done=1`, { method: "POST", body: new Blob([]) });
          const s = fpsSamples.slice(2);
          const fpsMin = s.length ? Math.min(...s) : 0;
          const fpsAvg = s.length ? +(s.reduce((a, b) => a + b, 0) / s.length).toFixed(1) : 0;
          resolve({ chunks: index, mime, width: canvas.width, height: canvas.height, fpsMin, fpsAvg });
        };
      });
      recorder.start(1000);
      setTimeout(() => recorder.stop(), Math.round(captureDuration * 1000));
      return await stopped;
    },
    { fps, bitrate, captureDuration, postUrl },
  );
  console.log("recorded:", JSON.stringify(recorded));
  if (recorded.fpsAvg) {
    const smooth = recorded.fpsMin >= fps * 0.85;
    console.log(`${smooth ? "ok" : "warn"} frame rate: ${recorded.fpsAvg} avg / ${recorded.fpsMin} min (target ${fps})`);
  }
  return recorded;
}

async function main() {
  if (hasFlag("--help")) {
    usage();
    return;
  }
  const durationSec = takeNumber("--duration", 10);
  const composeSeed = take("--compose");
  const baseUrl = take("--url", "http://localhost:8787/");
  const width = takeNumber("--width", 1920);
  const height = takeNumber("--height", 1080);
  const fps = takeNumber("--fps", 60);
  const label = take("--label", `geno-drift-lattice-${composeSeed ?? durationSec}`);
  const outDir = resolve(take("--out-dir", "renders/proofs"));
  const chromePath = take("--chrome") || process.env.CHROME || findChrome();
  const headless = !hasFlag("--no-headless");
  const remux = !hasFlag("--no-remux");
  const produce = hasFlag("--produce");
  const reuseChunks = hasFlag("--reuse-chunks");
  const lufs = takeNumber("--lufs", -16);
  const bitrate = takeNumber("--bitrate", width * height >= 3840 * 2160 ? 80_000_000 : 28_000_000);
  const captureDuration = produce ? durationSec + 6 : durationSec;

  if (produce && composeSeed == null) throw new Error("--produce requires --compose <seed>");
  if (!chromePath) throw new Error("Chrome/Chromium not found; set CHROME=/path/to/browser or pass --chrome");
  ensureChrome(chromePath);
  if (remux || produce) ensureExecutable("ffmpeg", { args: ["-version"], label: "ffmpeg", installHint: "install ffmpeg" });
  if (produce) {
    ensureExecutable("ffprobe", { args: ["-version"], label: "ffprobe", installHint: "install ffmpeg" });
    ensureExecutable("rsvg-convert", { args: ["--version"], label: "rsvg-convert", installHint: "install librsvg" });
  }

  const chunkDir = join(outDir, `${label}-chunks`);
  const webmPath = join(outDir, `${label}.webm`);
  const remuxPath = join(outDir, `${label}-video.webm`);
  const audioPath = join(outDir, `${label}.wav`);
  const muxPath = join(outDir, `${label}-muxed.mkv`);
  const finalPath = join(outDir, `${label}.mp4`);
  const screenshotPath = join(outDir, `${label}-preview.png`);
  mkdirSync(outDir, { recursive: true });
  if (!reuseChunks) {
    rmSync(chunkDir, { recursive: true, force: true });
    mkdirSync(chunkDir, { recursive: true });
  }

  const { server, port: chunkPort, done, audioDone } = await startChunkServer(chunkDir, produce ? audioPath : null);
  let browser;
  try {
    browser = await puppeteer.launch({
      executablePath: chromePath,
      headless,
      protocolTimeout: Math.max(120_000, Math.ceil((captureDuration + durationSec) * 1000) + 120_000),
      args: [
        "--autoplay-policy=no-user-gesture-required",
        "--enable-unsafe-webgpu",
        "--ignore-gpu-blocklist",
        "--disable-background-timer-throttling",
        "--disable-renderer-backgrounding",
        "--disable-backgrounding-occluded-windows",
        "--hide-scrollbars",
        `--window-size=${width},${height}`,
      ],
      defaultViewport: { width, height, deviceScaleFactor: 1 },
    });
    const page = await browser.newPage();
    page.setDefaultTimeout(Math.max(60_000, Math.ceil(captureDuration * 1000) + 60_000));
    page.on("console", (msg) => console.log(`[browser:${msg.type()}] ${msg.text()}`));
    page.on("pageerror", (error) => console.error("[browser:exception]", error));
    const url = composeSeed != null ? `${baseUrl}?compose=${composeSeed}&dur=${durationSec}` : baseUrl;
    await page.goto(url, { waitUntil: "networkidle2", timeout: 60_000 });
    await page.waitForSelector("#app-canvas", { timeout: 30_000 });
    await page.waitForFunction(() => window.geno?.isReady?.() || document.getElementById("no-webgpu")?.style.display === "block", {
      timeout: 45_000,
    });
    const ready = await page.evaluate(() => ({
      ok: !!window.geno?.isReady?.(),
      canvas: !!document.getElementById("app-canvas"),
      noWebgpu: getComputedStyle(document.getElementById("no-webgpu")).display !== "none",
    }));
    console.log("ready:", JSON.stringify(ready));
    if (!ready.ok) throw new Error(`Page did not become ready: ${JSON.stringify(ready)}`);
    await page.addStyleTag({
      content:
        "#start-overlay,#hint-overlay,#audio-error,#no-webgpu{display:none!important} body{cursor:none!important;background:#000!important;overflow:hidden!important}",
    });

    let chunkCount;
    if (reuseChunks) {
      chunkCount = readdirSync(chunkDir).filter((file) => file.endsWith(".part")).length;
      console.log(`reusing ${chunkCount} chunks from ${chunkDir}`);
    } else {
      const recorded = await captureChunks(page, {
        screenshotPath,
        composeSeed,
        durationSec,
        fps,
        bitrate,
        captureDuration,
        chunkPort,
      });
      if (!recorded.chunks) throw new Error("capture produced no chunks");
      chunkCount = await done;
      console.log(`chunks received: ${chunkCount}`);
    }
    if (chunkCount <= 0) throw new Error(`no recorded chunks found in ${chunkDir}`);

    const files = readdirSync(chunkDir).filter((file) => file.endsWith(".part")).sort();
    const out = createWriteStream(webmPath);
    for (const file of files) {
      if (!out.write(readFileSync(join(chunkDir, file)))) {
        await new Promise((resolveDrain) => out.once("drain", resolveDrain));
      }
    }
    await new Promise((resolveEnd, rejectEnd) => {
      out.on("error", rejectEnd);
      out.end(resolveEnd);
    });
    console.log(`raw video: ${webmPath} (${(statSync(webmPath).size / 1e9).toFixed(2)} GB)`);

    if (remux) {
      run("ffmpeg", ["-hide_banner", "-y", "-i", webmPath, "-c", "copy", remuxPath]);
      console.log(`remuxed video: ${remuxPath}`);
    }

    if (produce) {
      const audioUrl = `http://127.0.0.1:${chunkPort}/?audio=1`;
      const report = await page.evaluate(
        ({ audioUrl, durationSec, composeSeed, lufs }) =>
          window.geno.renderPieceTo(audioUrl, durationSec, Number(composeSeed), lufs),
        { audioUrl, durationSec, composeSeed, lufs },
      );
      await audioDone;
      console.log(`audio: ${audioPath}`);
      console.log(`master: ${String(report).replace(/\n/g, " | ")}`);
      const videoSource = remux ? remuxPath : webmPath;
      run("ffmpeg", [
        "-hide_banner",
        "-y",
        "-i",
        videoSource,
        "-i",
        audioPath,
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "320k",
        "-shortest",
        muxPath,
      ]);
      run("node", [
        "scripts/add-video-captions.mjs",
        "--input",
        muxPath,
        "--output",
        finalPath,
        ...passArg("--start-title"),
        ...passArg("--start-subtitle"),
        ...passArg("--end-title"),
        ...passArg("--end-subtitle"),
        "--bitrate",
        String(bitrate),
        ...passArg("--video-codec"),
      ]);
      console.log(`\nFinished piece: ${finalPath}`);
    }
  } finally {
    server.close();
    if (browser) await browser.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
