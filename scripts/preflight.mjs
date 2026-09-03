import { existsSync } from "node:fs";
import { createServer } from "node:net";
import { spawnSync } from "node:child_process";

export function findChrome() {
  return [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  ].find((path) => existsSync(path));
}

export function ensureExecutable(command, { args = ["--version"], label = command, installHint = "" } = {}) {
  const result = spawnSync(command, args, { stdio: "ignore" });
  if (result.error) {
    const detail = result.error.code === "ENOENT" ? "not found" : `could not start: ${result.error.message}`;
    throw new Error(`${label} is required (${detail})${installHint ? `. ${installHint}` : ""}`);
  }
  if (result.status !== 0) {
    throw new Error(`${label} is required, but "${command} ${args.join(" ")}" exited with status ${result.status}`);
  }
}

export function ensureChrome(chromePath) {
  ensureExecutable(chromePath, {
    args: ["--version"],
    label: "Chrome/Chromium",
    installHint: "install Chrome/Chromium, set CHROME=/path/to/browser, or pass --chrome /path/to/browser",
  });
}

export async function getFreePort(host = "127.0.0.1") {
  const server = createServer();
  server.unref();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, host, resolve);
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : null;
  await new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  if (!port) throw new Error("could not allocate a free TCP port");
  return port;
}
