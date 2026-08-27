#!/usr/bin/env node
// openwifidiag launcher: resolves the platform-specific optional package
// that contains the Rust binary, then spawns it with inherited stdio.
"use strict";

const { spawnSync } = require("node:child_process");

const { platform, arch } = process;
const PLATFORMS = {
  "darwin-arm64": "openwifidiag-darwin-arm64",
  "darwin-x64": "openwifidiag-darwin-x64",
  "linux-arm64": "openwifidiag-linux-arm64",
  "linux-x64": "openwifidiag-linux-x64",
  // Windows on Arm supports the x64 binary through Windows' x64 emulation.
  "win32-arm64": "openwifidiag-win32-x64",
  "win32-x64": "openwifidiag-win32-x64",
};

const key = `${platform}-${arch}`;
const pkg = PLATFORMS[key];
if (!pkg) {
  console.error(
    `openwifidiag: no pre-built binary for platform '${key}'. Supported: ${Object.keys(PLATFORMS).join(", ")}`
  );
  process.exit(1);
}

let binDir;
try {
  const pkgJsonPath = require.resolve(`${pkg}/package.json`);
  binDir = require("node:path").dirname(pkgJsonPath);
} catch {
  console.error(`openwifidiag: platform package '${pkg}' is not installed — try reinstalling.`);
  process.exit(1);
}

const isWindows = platform === "win32";
const exe = isWindows ? "openwifidiag.exe" : "openwifidiag";
const binPath = require("node:path").join(binDir, "bin", exe);

const result = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`openwifidiag: failed to run '${binPath}': ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? (result.signal ? 1 : 0));
