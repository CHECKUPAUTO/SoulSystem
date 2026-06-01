#!/usr/bin/env node
'use strict';

const { execSync, spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const os = require('os');

const REPO_URL    = 'https://github.com/CHECKUPAUTO/openevolve.git';
const CARGO_BIN   = process.env.CARGO_BIN || 'cargo';
const INSTALL_DIR = process.env.OPENEVOLVE_HOME || path.join(os.homedir(), '.openevolve');
const SRC_DIR     = path.join(INSTALL_DIR, 'src');
const BIN_DIR     = path.join(INSTALL_DIR, 'bin');
const BIN_PATH    = path.join(BIN_DIR, 'openevolve');
const VERSION_FILE = path.join(INSTALL_DIR, '.version');
const BRANCH      = 'main';
const SUBDIR      = '';

function log(msg)   { console.log(`[openevolve] ${msg}`); }
function warn(msg)  { console.warn(`[openevolve] ⚠  ${msg}`); }
function err(msg)   { console.error(`[openevolve] ❌ ${msg}`); process.exit(1); }
function ok(msg)    { console.log(`[openevolve] ✅ ${msg}`); }

function run(cmd, opts = {}) {
    log(`→ ${cmd}`);
    const r = spawnSync('bash', ['-c', cmd], { stdio: 'inherit', ...opts });
    if (r.status !== 0 && !opts.ignoreError) err(`Command failed: ${cmd}`);
    return r;
}

function runSilent(cmd) {
    return execSync(cmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
}

function hasCommand(cmd) {
    try { execSync(`which ${cmd}`, { stdio: 'ignore' }); return true; } catch { return false; }
}

async function install() {
    console.log('╔══════════════════════════════════════════════╗');
    console.log('║   openevolve v0.2.1 Installer          ║');
    console.log('╚══════════════════════════════════════════════╝\n');

    const missing = [];
    if (!hasCommand('cargo')) missing.push('cargo (Rust)');
    if (!hasCommand('git'))   missing.push('git');
    if (missing.length > 0) {
        err(`Missing: ${missing.join(', ')}.\nInstall Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`);
    }

    log(`Install dir: ${INSTALL_DIR}`);
    fs.mkdirSync(INSTALL_DIR, { recursive: true });
    fs.mkdirSync(BIN_DIR, { recursive: true });

    if (fs.existsSync(SRC_DIR)) {
        log('Repo exists, pulling latest...');
        run(`cd "${SRC_DIR}" && git fetch origin && git checkout ${BRANCH} && git pull origin ${BRANCH}`);
    } else {
        log('Cloning repo...');
        run(`git clone --branch ${BRANCH} "${REPO_URL}" "${SRC_DIR}"`);
    }

    log('Building (release mode)...');
    const buildCmd = SUBDIR
        ? `cd "${SRC_DIR}" && ${CARGO_BIN} build --release --manifest-path ${SUBDIR}/Cargo.toml`
        : `cd "${SRC_DIR}" && ${CARGO_BIN} build --release`;
    run(buildCmd);

    const buildDir = SUBDIR ? path.join(SRC_DIR, SUBDIR, 'target', 'release') : path.join(SRC_DIR, 'target', 'release');
    const builtBin = path.join(buildDir, 'openevolve');
    if (!fs.existsSync(builtBin)) err(`Build succeeded but binary not found at: ${builtBin}`);
    fs.copyFileSync(builtBin, BIN_PATH);
    fs.chmodSync(BIN_PATH, 0o755);

    const version = runSilent(`cd "${SRC_DIR}" && git rev-parse --short HEAD`);
    fs.writeFileSync(VERSION_FILE, version);
    ok(`Installed: ${BIN_PATH} (${version})`);

    addToPath();
    console.log(`\n  🚀 ${BIN} ready!\n  Run: ${BIN}\n  Add to PATH: export PATH="${BIN_DIR}:\/root/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/root/.lmstudio/bin:/root/.claude/plugins/cache/claude-plugins-official/pyright-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/rust-analyzer-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/typescript-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/frontend-design/2cb575bc9612/bin:/root/.claude/plugins/cache/claude-plugins-official/superpowers/5.0.7/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/github/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/ralph-loop/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/claude-md-management/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/pr-review-toolkit/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/telegram/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-adr/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-agentdb/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-autopilot/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-core/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-neural-trader/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-intelligence/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-rag-memory/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/serena/bin:/root/.claude/plugins/cache/caveman/caveman/ef6050c5e184/bin"\n`);
}

async function update() {
    if (!fs.existsSync(SRC_DIR)) {
        warn('Not installed. Running install...');
        return install();
    }

    const oldVer = fs.existsSync(VERSION_FILE) ? fs.readFileSync(VERSION_FILE, 'utf8').trim() : 'unknown';
    log(`Current: ${oldVer}`);
    run(`cd "${SRC_DIR}" && git fetch origin && git checkout ${BRANCH} && git pull origin ${BRANCH}`);

    const buildCmd = SUBDIR
        ? `cd "${SRC_DIR}" && ${CARGO_BIN} build --release --manifest-path ${SUBDIR}/Cargo.toml`
        : `cd "${SRC_DIR}" && ${CARGO_BIN} build --release`;
    run(buildCmd);

    const buildDir = SUBDIR ? path.join(SRC_DIR, SUBDIR, 'target', 'release') : path.join(SRC_DIR, 'target', 'release');
    const builtBin = path.join(buildDir, 'openevolve');
    if (!fs.existsSync(builtBin)) err(`Binary not found: ${builtBin}`);
    fs.copyFileSync(builtBin, BIN_PATH);
    fs.chmodSync(BIN_PATH, 0o755);

    const newVer = runSilent(`cd "${SRC_DIR}" && git rev-parse --short HEAD`);
    fs.writeFileSync(VERSION_FILE, newVer);
    ok(`Updated: ${oldVer} → ${newVer}`);
}

function addToPath() {
    const profiles = [
        path.join(os.homedir(), '.bashrc'),
        path.join(os.homedir(), '.zshrc'),
        path.join(os.homedir(), '.config', 'fish', 'config.fish'),
    ];
    for (const pf of profiles) {
        if (!fs.existsSync(pf)) continue;
        let content = fs.readFileSync(pf, 'utf8');
        if (content.includes(BIN_DIR)) { log(`PATH already in ${pf}`); continue; }
        if (pf.includes('fish')) {
            fs.appendFileSync(pf, `\n# openevolve\nfish_add_path "${BIN_DIR}"\n`);
        } else {
            fs.appendFileSync(pf, `\n# openevolve\nexport PATH="${BIN_DIR}:\/root/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/root/.lmstudio/bin:/root/.claude/plugins/cache/claude-plugins-official/pyright-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/rust-analyzer-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/typescript-lsp/1.0.0/bin:/root/.claude/plugins/cache/claude-plugins-official/frontend-design/2cb575bc9612/bin:/root/.claude/plugins/cache/claude-plugins-official/superpowers/5.0.7/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/github/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/ralph-loop/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/claude-md-management/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/plugins/pr-review-toolkit/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/telegram/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-adr/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-agentdb/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-autopilot/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-core/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-neural-trader/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-intelligence/bin:/root/.claude/plugins/marketplaces/ruflo/plugins/ruflo-rag-memory/bin:/root/.claude/plugins/marketplaces/claude-plugins-official/external_plugins/serena/bin:/root/.claude/plugins/cache/caveman/caveman/ef6050c5e184/bin"\n`);
        }
        ok(`Added to ${pf}`);
    }
}

function postinstall() {
    console.log('');
    console.log('╔══════════════════════════════════════════════╗');
    console.log('║   openevolve installed via npm           ║');
    console.log('╚══════════════════════════════════════════════╝');
    console.log('');
    console.log('  Install binary:  npx openevolve-install install');
    console.log('  Update binary:   npx openevolve-install update');
    console.log('');
}

const cmd = process.argv[2] || 'help';
(async () => {
    switch (cmd) {
        case 'install': return install();
        case 'update': return update();
        case 'postinstall': return postinstall();
        case 'version': case '--version': case '-v':
            console.log('openevolve npm installer v0.2.1'); break;
        default:
            console.log('openevolve Installer\n');
            console.log('  npx openevolve-install install   Install from source');
            console.log('  npx openevolve-install update    Update to latest');
            console.log('  openevolve                           Run after install\n');
    }
})();
