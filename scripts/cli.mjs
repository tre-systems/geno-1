import { spawnSync } from "node:child_process";

export const args = process.argv.slice(2);

export function take(name, fallback = null) {
  const index = args.indexOf(name);
  if (index === -1) return fallback;
  const value = args[index + 1];
  if (value == null || value.startsWith("--")) {
    throw new Error(`${name} requires a value`);
  }
  return value;
}

export function takeNumber(name, fallback) {
  const raw = take(name);
  if (raw == null) return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value)) throw new Error(`${name} must be a number`);
  return value;
}

export function hasFlag(name) {
  return args.includes(name);
}

export function passArg(name) {
  const value = take(name);
  return value != null ? [name, value] : [];
}

export function run(command, commandArgs, { inherit = false } = {}) {
  const result = spawnSync(
    command,
    commandArgs,
    inherit ? { stdio: "inherit" } : { stdio: "pipe", encoding: "utf8" },
  );
  if (result.error) {
    throw new Error(`${command} ${commandArgs.join(" ")} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    if (!inherit) {
      process.stderr.write(result.stdout ?? "");
      process.stderr.write(result.stderr ?? "");
    }
    throw new Error(`${command} ${commandArgs.join(" ")} failed (status ${result.status})`);
  }
  return inherit ? "" : result.stdout;
}
