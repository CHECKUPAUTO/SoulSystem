#!/usr/bin/env node
// Thin launcher: exec the downloaded native `soulsystem` binary, forwarding all
// args and the exit code.
"use strict";

const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const bin = path.join(__dirname, process.platform === "win32" ? "soulsystem.exe" : "soulsystem");

if (!fs.existsSync(bin)) {
  console.error("soulsystem binary not found; reinstall the package (postinstall downloads it).");
  process.exit(1);
}

const res = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
process.exit(res.status === null ? 1 : res.status);
