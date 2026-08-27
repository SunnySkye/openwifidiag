// `npm run pack` — assembles per-platform binary packages into ./npm/ from
// artifacts/<platform-key>/ (binaries produced by cargo builds) and stamps
// them with the root package.json version.
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const rootPkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = rootPkg.version;

const PLATFORMS = {
  "darwin-arm64": { cpu: "arm64", os: "darwin", exe: "openwifidiag" },
  "darwin-x64": { cpu: "x64", os: "darwin", exe: "openwifidiag" },
  "linux-arm64": { cpu: "arm64", os: "linux", exe: "openwifidiag" },
  "linux-x64": { cpu: "x64", os: "linux", exe: "openwifidiag" },
  "win32-x64": { cpu: "x64", os: "win32", exe: "openwifidiag.exe" },
  "win32-arm64": { cpu: "arm64", os: "win32", exe: "openwifidiag.exe" },
};

const npmRoot = path.join(root, "npm");
if (fs.existsSync(npmRoot)) fs.rmSync(npmRoot, { recursive: true });
fs.mkdirSync(npmRoot, { recursive: true });

let packed = 0;
for (const [key, { cpu, os, exe }] of Object.entries(PLATFORMS)) {
  const name = `openwifidiag-${key}`;
  const artifacts = path.join(root, "artifacts", key);
  const binary = path.join(artifacts, exe);
  const pkgDir = path.join(npmRoot, name);

  const manifest = {
    name,
    version,
    description: `Prebuilt openwifidiag binary (${os}-${cpu})`,
    license: "MIT",
    os: [os],
    cpu: [cpu],
    bin: { openwifidiag: `bin/${exe}` },
    files: ["bin"],
  };

  fs.mkdirSync(path.join(pkgDir, "bin"), { recursive: true });
  fs.writeFileSync(path.join(pkgDir, "package.json"), JSON.stringify(manifest, null, 2) + "\n");

  if (fs.existsSync(binary)) {
    fs.copyFileSync(binary, path.join(pkgDir, "bin", exe));
    fs.chmodSync(path.join(pkgDir, "bin", exe), 0o755);
    packed += 1;
  } else {
    console.warn(`warning: no binary for '${key}' at ${binary}`);
  }
}
console.log(`created ${Object.keys(PLATFORMS).length} platform package manifests in ./npm; ${packed} contain binaries`);
