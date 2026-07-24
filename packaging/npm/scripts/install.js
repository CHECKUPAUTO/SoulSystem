#!/usr/bin/env node
// postinstall: download the prebuilt `soulsystem` binary for this platform from
// the matching GitHub release into bin/. Mirrors install.sh, for `npm i -g`.
"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const zlib = require("zlib");
const { execFileSync } = require("child_process");

const REPO = "Memorithm/SoulSystem";
const VERSION = process.env.SOULSYSTEM_VERSION || `v${require("../package.json").version}`;

function target() {
  const arch = { x64: "x86_64", arm64: "aarch64" }[process.arch];
  const os = { linux: "unknown-linux-gnu", darwin: "apple-darwin" }[process.platform];
  if (!arch || !os) {
    throw new Error(`unsupported platform: ${process.platform}/${process.arch}`);
  }
  return `${arch}-${os}`;
}

function get(url, cb, redirects = 0) {
  https.get(url, { headers: { "User-Agent": "soulsystem-installer" } }, (res) => {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      if (redirects > 10) return cb(new Error("too many redirects"));
      return get(res.headers.location, cb, redirects + 1);
    }
    if (res.statusCode !== 200) return cb(new Error(`HTTP ${res.statusCode} for ${url}`));
    cb(null, res);
  }).on("error", cb);
}

function main() {
  const tgt = target();
  const url = `https://github.com/${REPO}/releases/download/${VERSION}/soulsystem-${tgt}.tar.gz`;
  const binDir = path.join(__dirname, "..", "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const tarPath = path.join(binDir, "soulsystem.tar.gz");

  console.log(`Downloading ${url}`);
  get(url, (err, res) => {
    if (err) {
      console.error(`\nCould not download a prebuilt binary: ${err.message}`);
      console.error("Build from source instead:");
      console.error("  cargo install --git https://github.com/" + REPO + " soulsystem\n");
      process.exit(1);
    }
    const out = fs.createWriteStream(tarPath);
    res.pipe(out);
    out.on("finish", () => {
      out.close(() => {
        get(`${url}.sha256`, (checksumErr, checksumRes) => {
          if (checksumErr) {
            console.error(`Could not download release checksum: ${checksumErr.message}`);
            process.exit(1);
          }
          let checksumText = "";
          checksumRes.setEncoding("utf8");
          checksumRes.on("data", (chunk) => { checksumText += chunk; });
          checksumRes.on("end", () => {
            const expected = checksumText.trim().split(/\s+/)[0];
            const actual = crypto
              .createHash("sha256")
              .update(fs.readFileSync(tarPath))
              .digest("hex");
            if (!/^[a-f0-9]{64}$/i.test(expected) || actual !== expected.toLowerCase()) {
              console.error("Release checksum verification failed.");
              process.exit(1);
            }
            // Extract the single binary from the verified gzipped tarball via `tar`.
            execFileSync("tar", ["-xzf", tarPath, "-C", binDir]);
            fs.chmodSync(path.join(binDir, "soulsystem"), 0o755);
            fs.unlinkSync(tarPath);
            console.log("soulsystem installed.");
          });
        });
      });
    });
  });
}

main();
