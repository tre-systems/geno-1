#!/usr/bin/env node
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { take, takeNumber, hasFlag, run } from "./cli.mjs";
import { ensureExecutable } from "./preflight.mjs";

function usage() {
  console.error(`Usage:
  npm run video:captions -- --input in.mp4 --output out.mp4 [options]

Options:
  --start-title "Geno-1: Drift Lattice"
  --start-subtitle "a generative sound sculpture for three spatial voices"
  --end-title "Geno-1: Drift Lattice"
  --end-subtitle "Multivibrator - @mvbrtr"
  --start-at 1.7
  --start-duration 4
  --end-duration 7
  --fade 0.8
  --bitrate 55M
  --video-codec auto|h264|hevc|libx264|hevc_videotoolbox
`);
}

function required(name) {
  const value = take(name);
  if (!value) {
    usage();
    throw new Error(`${name} is required`);
  }
  return value;
}

function probe(path) {
  const json = run("ffprobe", [
    "-hide_banner",
    "-v",
    "error",
    "-show_entries",
    "format=duration:stream=codec_type,width,height",
    "-of",
    "json",
    path,
  ]);
  const data = JSON.parse(json);
  const video = data.streams.find((stream) => stream.codec_type === "video");
  if (!video) throw new Error(`No video stream found in ${path}`);
  return {
    duration: Number(data.format.duration),
    width: Number(video.width),
    height: Number(video.height),
  };
}

function escapeXml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function textBlock(lines, { x, y, fontSize, lineHeight, weight }) {
  if (!lines.length) return "";
  const spans = lines
    .map((line, index) => `<tspan x="${x}" dy="${index === 0 ? 0 : lineHeight}">${escapeXml(line)}</tspan>`)
    .join("");
  return `<text x="${x}" y="${y}" text-anchor="middle"
    font-family="Avenir, Avenir Next, Helvetica, Arial, sans-serif"
    font-size="${fontSize}" font-weight="${weight}" fill="#050505"
    letter-spacing="0">${spans}</text>`;
}

function writeCaptionSvg(path, { width, height, title, subtitle, y }) {
  const titleLines = title.split("\n").map((line) => line.trim()).filter(Boolean);
  const subtitleLines = subtitle.split("\n").map((line) => line.trim()).filter(Boolean);
  const titleSize = Math.max(54, Math.round(height / 21));
  const subtitleSize = Math.max(28, Math.round(height / 43));
  const subtitleY = y + titleSize * 1.24;
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <rect width="100%" height="100%" fill="none"/>
  ${textBlock(titleLines, { x: width / 2, y, fontSize: titleSize, lineHeight: titleSize * 1.15, weight: 700 })}
  ${textBlock(subtitleLines, { x: width / 2, y: subtitleY, fontSize: subtitleSize, lineHeight: subtitleSize * 1.35, weight: 500 })}
</svg>`;
  writeFileSync(path, svg);
}

function renderPng(svgPath, pngPath, width, height) {
  run("rsvg-convert", ["--format", "png", "--width", String(width), "--height", String(height), "--output", pngPath, svgPath]);
}

function codecCandidates(requested) {
  const value = requested.trim().toLowerCase();
  if (!value || value === "auto") return process.platform === "darwin" ? ["hevc_videotoolbox", "libx264"] : ["libx264"];
  if (value === "h264" || value === "x264") return ["libx264"];
  if (value === "hevc" || value === "h265") return process.platform === "darwin" ? ["hevc_videotoolbox", "libx265"] : ["libx265"];
  return [requested];
}

function parseBitrateBps(value) {
  const trimmed = String(value).trim();
  const match = /^(\d+(?:\.\d+)?)([kKmMgG])?$/.exec(trimmed);
  if (!match) throw new Error(`Invalid bitrate: ${value}`);
  const amount = Number(match[1]);
  const unit = match[2]?.toLowerCase();
  const factor = unit === "g" ? 1_000_000_000 : unit === "m" ? 1_000_000 : unit === "k" ? 1_000 : 1;
  return Math.round(amount * factor);
}

function ffmpegBitrate(value) {
  const bps = parseBitrateBps(value);
  if (bps >= 1_000_000 && bps % 1_000_000 === 0) return `${bps / 1_000_000}M`;
  if (bps >= 1_000 && bps % 1_000 === 0) return `${bps / 1_000}k`;
  return String(bps);
}

function videoCodecArgs(encoder, bitrate) {
  const args = ["-c:v", encoder];
  if (encoder === "libx264" || encoder === "libx265") args.push("-preset", "slow");
  const targetBps = parseBitrateBps(bitrate);
  const maxrate = ffmpegBitrate(Math.round(targetBps * 1.35));
  const bufsize = ffmpegBitrate(Math.round(targetBps * 2.70));
  args.push("-b:v", ffmpegBitrate(targetBps), "-maxrate", maxrate, "-bufsize", bufsize);
  if (/hevc|h265|x265/i.test(encoder)) args.push("-tag:v", "hvc1");
  return args;
}

function encodeWithFallback({ ffmpegArgs, filter, bitrate, requestedCodec, output }) {
  const failures = [];
  for (const encoder of codecCandidates(requestedCodec)) {
    const args = [
      ...ffmpegArgs,
      "-filter_complex",
      filter,
      "-map",
      "[v]",
      "-map",
      "0:a?",
      ...videoCodecArgs(encoder, bitrate),
      "-pix_fmt",
      "yuv420p",
      "-color_primaries",
      "bt709",
      "-color_trc",
      "bt709",
      "-colorspace",
      "bt709",
      "-c:a",
      "copy",
      "-movflags",
      "+faststart",
      output,
    ];
    console.log(`Video codec: ${encoder}`);
    const result = spawnSync("ffmpeg", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    if (!result.error && result.status === 0) return;
    rmSync(output, { force: true });
    failures.push(`${encoder}: ${result.stderr?.split("\n").slice(-10).join("\n") || result.error?.message || result.status}`);
  }
  throw new Error(`Caption encode failed:\n${failures.join("\n")}`);
}

if (hasFlag("--help")) {
  usage();
  process.exit(0);
}

const input = required("--input");
const output = required("--output");
const startTitle = take("--start-title", "Geno-1: Drift Lattice");
const startSubtitle = take("--start-subtitle", "a generative sound sculpture for three spatial voices").replaceAll("\\n", "\n");
const endTitle = take("--end-title", "Geno-1: Drift Lattice");
const endSubtitle = take("--end-subtitle", "Multivibrator - @mvbrtr").replaceAll("\\n", "\n");
const startAt = takeNumber("--start-at", 1.7);
const startDuration = takeNumber("--start-duration", 4.0);
const endDuration = takeNumber("--end-duration", 7.0);
const fade = takeNumber("--fade", 0.8);
const bitrate = take("--bitrate", "55M");
const videoCodec = take("--video-codec", "auto");

if (!existsSync(input)) throw new Error(`Input does not exist: ${input}`);
ensureExecutable("ffmpeg", { args: ["-version"], label: "ffmpeg", installHint: "install ffmpeg (macOS: brew install ffmpeg)" });
ensureExecutable("ffprobe", { args: ["-version"], label: "ffprobe", installHint: "install ffmpeg (macOS: brew install ffmpeg)" });
ensureExecutable("rsvg-convert", { args: ["--version"], label: "rsvg-convert", installHint: "install librsvg (macOS: brew install librsvg)" });

const meta = probe(input);
if (!Number.isFinite(meta.duration)) throw new Error(`Could not read video duration for ${input}`);
mkdirSync(dirname(output), { recursive: true });
const tmp = mkdtempSync(join(tmpdir(), "geno-captions-"));

try {
  const startEnd = Math.min(meta.duration - 0.15, startAt + startDuration);
  const endStart = Math.max(startEnd + 1, meta.duration - endDuration);
  const endEnd = meta.duration - 0.15;
  const overlays = [];

  if (startEnd - startAt >= 0.1 && (startTitle.trim() || startSubtitle.trim())) {
    const svgPath = join(tmp, "start.svg");
    const pngPath = join(tmp, "start.png");
    writeCaptionSvg(svgPath, { width: meta.width, height: meta.height, title: startTitle, subtitle: startSubtitle, y: meta.height * 0.77 });
    renderPng(svgPath, pngPath, meta.width, meta.height);
    overlays.push({ pngPath, start: startAt, end: startEnd, duration: startEnd - startAt });
  }
  if (endEnd - endStart >= 0.1 && (endTitle.trim() || endSubtitle.trim())) {
    const svgPath = join(tmp, "end.svg");
    const pngPath = join(tmp, "end.png");
    writeCaptionSvg(svgPath, { width: meta.width, height: meta.height, title: endTitle, subtitle: endSubtitle, y: meta.height * 0.33 });
    renderPng(svgPath, pngPath, meta.width, meta.height);
    overlays.push({ pngPath, start: endStart, end: endEnd, duration: endEnd - endStart });
  }
  if (!overlays.length) throw new Error("No captions fit inside the input duration");

  const ffmpegArgs = ["-hide_banner", "-y", "-i", input];
  for (const overlay of overlays) ffmpegArgs.push("-loop", "1", "-t", String(overlay.duration), "-i", overlay.pngPath);

  let filter = "";
  let base = "[0:v]";
  overlays.forEach((overlay, index) => {
    const streamName = `caption${index}`;
    const fadeDuration = Math.max(0.01, Math.min(fade, overlay.duration / 2));
    filter += `[${index + 1}:v]format=rgba,fade=t=in:st=0:d=${fadeDuration}:alpha=1,fade=t=out:st=${overlay.duration - fadeDuration}:d=${fadeDuration}:alpha=1,setpts=PTS+${overlay.start}/TB[${streamName}];`;
    const outName = index === overlays.length - 1 ? "[v]" : `[v${index}]`;
    filter += `${base}[${streamName}]overlay=0:0:enable='between(t,${overlay.start},${overlay.end})'${outName}`;
    if (index !== overlays.length - 1) filter += ";";
    base = outName;
  });

  console.log(`Input: ${basename(input)} (${meta.width}x${meta.height}, ${meta.duration.toFixed(3)}s)`);
  console.log(`Output: ${output}`);
  encodeWithFallback({ ffmpegArgs, filter, bitrate, requestedCodec: videoCodec, output });
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
