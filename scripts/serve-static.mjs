#!/usr/bin/env node
import { createReadStream } from "node:fs";
import { access, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { take, hasFlag } from "./cli.mjs";

const TYPES = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
  [".wasm", "application/wasm"],
]);

export async function startStaticServer({ dir = "dist", host = "127.0.0.1", port = 8787, cors = false } = {}) {
  const root = resolve(process.cwd(), dir);
  const server = createServer(async (req, res) => {
    if (cors) res.setHeader("Access-Control-Allow-Origin", "*");
    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405, { Allow: "GET, HEAD" });
      res.end("Method Not Allowed");
      return;
    }
    try {
      const file = await resolveRequest(root, req.url || "/");
      res.setHeader("Content-Type", TYPES.get(extname(file)) || "application/octet-stream");
      res.setHeader("Cache-Control", "no-cache");
      if (req.method === "HEAD") {
        res.writeHead(200);
        res.end();
        return;
      }
      createReadStream(file).pipe(res);
    } catch (error) {
      res.writeHead(error.code === "ENOENT" ? 404 : 400, { "Content-Type": "text/plain; charset=utf-8" });
      res.end(error.code === "ENOENT" ? "Not Found" : "Bad Request");
    }
  });
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(port, host, resolveListen);
  });
  const address = server.address();
  return { server, root, url: `http://${address.address}:${address.port}/` };
}

async function resolveRequest(root, rawUrl) {
  const url = new URL(rawUrl, "http://local.invalid");
  let pathname = decodeURIComponent(url.pathname);
  if (pathname.endsWith("/")) pathname += "index.html";
  const candidate = resolve(root, pathname.replace(/^\/+/, ""));
  if (candidate !== root && !candidate.startsWith(root + sep)) {
    throw Object.assign(new Error("Path traversal rejected"), { code: "BAD_PATH" });
  }
  let info;
  try {
    info = await stat(candidate);
  } catch (error) {
    if (error.code === "ENOENT" && !extname(candidate)) {
      const htmlCandidate = `${candidate}.html`;
      if (htmlCandidate.startsWith(root + sep)) {
        const html = await stat(htmlCandidate).catch(() => null);
        if (html?.isFile()) return htmlCandidate;
      }
    }
    throw error;
  }
  if (info.isDirectory()) {
    const index = join(candidate, "index.html");
    await access(index);
    return index;
  }
  if (!info.isFile()) throw Object.assign(new Error("Not a file"), { code: "ENOENT" });
  return candidate;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = take("--dir", process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "dist");
  const port = Number(take("--port", "8787"));
  const host = take("--host", "127.0.0.1");
  startStaticServer({ dir, port, host, cors: hasFlag("--cors") })
    .then(({ url, root }) => {
      console.log(`serve-static: ${root}`);
      console.log(`serve-static: ${url}`);
    })
    .catch((error) => {
      console.error(error.message || error);
      process.exit(1);
    });
}
