#!/usr/bin/env node
/**
 * Stage the transcribe.cpp native runtime for the transcribeCpp provider.
 *
 * Downloads the release archive for the current platform/arch lane from
 * https://github.com/handy-computer/transcribe.cpp/releases, verifies its
 * SHA-256 against the pin below, and extracts it (flat) into
 * frontend/src-tauri/transcribecpp-runtime/ (gitignored). The directory
 * holds libtranscribe.* + the ggml backend modules + contract.json +
 * licenses/; Tauri maps it next to the executable on Windows via
 * tauri.windows.conf.json ("transcribecpp-runtime": "."), mirroring the
 * audio.cpp runtime bundling.
 *
 * Usage:
 *   node scripts/stage-transcribe-native.mjs            # stage (idempotent)
 *   node scripts/stage-transcribe-native.mjs --verify   # assert staged files
 *
 * Lanes: linux-x86_64 / linux-aarch64 / windows-x86_64 (cpu-vulkan),
 * macos-arm64 (metal). Only Node built-ins are used; runs on Node >= 20 and
 * Bun. Extraction uses the system `tar` (GNU/BSD tar on Linux/macOS, the
 * bsdtar shipped with Windows 10+).
 *
 * Windows system DLL dependencies (documented, NOT bundled — unlike the
 * audio.cpp runtime, which stages the MSVC redist DLLs explicitly):
 *   - The v0.4.2 windows-x86_64-cpu-vulkan archive ships only transcribe.dll,
 *     ggml*.dll, contract.json and licenses/ — no msvcp140/vcruntime140/
 *     vcomp140. Verified by listing the archive at script development time.
 *   - The upstream wheel is repaired with `delvewheel repair --exclude
 *     vulkan-1.dll` (pyproject.toml at tag v0.4.2): the Vulkan loader
 *     vulkan-1.dll is expected from the system (installed with GPU drivers),
 *     "never vendored" per upstream's own comment.
 *   - delvewheel by design does not vendor the MSVC runtime: its default
 *     exclusion list skips all vc_redist-pattern DLLs (vcruntime140*,
 *     msvcp*, vcomp*, …), i.e. upstream assumes the Visual C++ 2015-2022
 *     Redistributable is present on the target machine.
 *   => Nothing extra is bundled here. If we ever need "works without the
 *      VC++ redist" on Windows like audio.cpp does, the MSVC DLLs must be
 *      sourced and pinned separately (see stage-audiocpp-runtime.ps1).
 *
 * Upgrading transcribe.cpp: bump TRANSCRIBE_CPP_VERSION, every lane's
 * sha256, and (per the ABI gate in the Rust module) the contract pins in
 * one coordinated change.
 */

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import fs from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TRANSCRIBE_CPP_VERSION = '0.4.2';

// SHA-256 of each lane's release tarball. Re-computed by downloading every
// lane at staging-script development time; cross-checked against the GitHub
// asset digests (all four matched).
export const LANE_SHA256 = {
  'linux-x86_64-cpu-vulkan': '4fe87db8919ea34e461db7987713b580f54e6f3ed610457c5d8fb8eac1a15046',
  'linux-aarch64-cpu-vulkan': '5e9cc7b244949fc686d1272e796e965411e80cc768533f1704c3c53e4c7d695e',
  'windows-x86_64-cpu-vulkan': '6094102cf57b404b050707ceb258b12ce647b92e801b5d46077214e5b030c8aa',
  'macos-arm64-metal': '75ea3a705f3edec09942fdbb1b2b5509d7ecd75bda55158a47a3ab1e2d1105b3',
};

// Expected contract.json fields for the pinned version. The authoritative
// ABI gate is the Rust module; this is an early sanity check at staging.
export const CONTRACT_PINS = {
  version: TRANSCRIBE_CPP_VERSION,
  header_hash: 'f99ad4169374b8f3',
};

const RELEASE_BASE_URL =
  'https://github.com/handy-computer/transcribe.cpp/releases/download';

const STAMP_FILE = '.staged';

/** Map Node platform/arch to a release lane, or null when unsupported. */
export function resolveLane(platform, arch) {
  switch (`${platform}/${arch}`) {
    case 'linux/x64':
      return 'linux-x86_64-cpu-vulkan';
    case 'linux/arm64':
      return 'linux-aarch64-cpu-vulkan';
    case 'win32/x64':
      return 'windows-x86_64-cpu-vulkan';
    case 'darwin/arm64':
      return 'macos-arm64-metal';
    default:
      return null;
  }
}

export function archiveNameFor(lane) {
  return `transcribe-native-${TRANSCRIBE_CPP_VERSION}-${lane}.tar.gz`;
}

export function downloadUrlFor(lane) {
  return `${RELEASE_BASE_URL}/v${TRANSCRIBE_CPP_VERSION}/${archiveNameFor(lane)}`;
}

/** Compare a hex SHA-256 digest with the lane pin (case-insensitive). */
export function shaMatches(actual, expected) {
  return (
    typeof actual === 'string' &&
    typeof expected === 'string' &&
    actual.toLowerCase() === expected.toLowerCase()
  );
}

/** Validate parsed contract.json against the version/header_hash pins. */
export function validateContract(contract, pins = CONTRACT_PINS) {
  const problems = [];
  if (!contract || typeof contract !== 'object') {
    return ['contract.json missing or not an object'];
  }
  for (const key of Object.keys(pins)) {
    if (contract[key] !== pins[key]) {
      problems.push(
        `contract.json ${key}=${JSON.stringify(contract[key])} (expected ${JSON.stringify(pins[key])})`
      );
    }
  }
  if (!Array.isArray(contract.backends) || contract.backends.length === 0) {
    problems.push('contract.json backends missing or empty');
  }
  return problems;
}

/** Per-platform name of the main native library inside the bundle. */
export function libraryFileNameFor(lane) {
  if (lane.startsWith('windows-')) return 'transcribe.dll';
  if (lane.startsWith('macos-')) return 'libtranscribe.dylib';
  return 'libtranscribe.so';
}

/** Files that must exist for the staging directory to count as complete. */
export function requiredFilesFor(lane) {
  // Forward slashes: fs handles them on every platform, incl. Windows.
  return [libraryFileNameFor(lane), 'contract.json', 'licenses/LICENSE'];
}

export function stagingDir(rootDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..')) {
  return path.join(rootDir, 'src-tauri', 'transcribecpp-runtime');
}

function readStamp(dir) {
  try {
    return JSON.parse(fs.readFileSync(path.join(dir, STAMP_FILE), 'utf8'));
  } catch {
    return null;
  }
}

function isStagedComplete(dir, lane) {
  for (const rel of requiredFilesFor(lane)) {
    if (!fs.existsSync(path.join(dir, rel))) return false;
  }
  const stamp = readStamp(dir);
  return Boolean(
    stamp &&
      stamp.version === TRANSCRIBE_CPP_VERSION &&
      stamp.lane === lane &&
      shaMatches(stamp.sha256, LANE_SHA256[lane])
  );
}

function getWithRedirects(url, redirectsLeft = 10) {
  return new Promise((resolve, reject) => {
    const client = url.startsWith('http://') ? http : https;
    const req = client.get(
      url,
      { headers: { 'User-Agent': 'Samobranka/stage-transcribe-native' } },
      (res) => {
        const status = res.statusCode ?? 0;
        if (status >= 300 && status < 400 && res.headers.location) {
          res.resume();
          if (redirectsLeft <= 0) {
            reject(new Error(`Too many redirects fetching ${url}`));
            return;
          }
          resolve(getWithRedirects(new URL(res.headers.location, url).toString(), redirectsLeft - 1));
          return;
        }
        if (status !== 200) {
          res.resume();
          reject(new Error(`HTTP ${status} fetching ${url}`));
          return;
        }
        resolve(res);
      }
    );
    req.on('error', reject);
    req.setTimeout(120_000, () => req.destroy(new Error(`Timeout fetching ${url}`)));
  });
}

async function downloadArchive(url, expectedSha, destFile) {
  const res = await getWithRedirects(url);
  const hash = createHash('sha256');
  await new Promise((resolve, reject) => {
    const out = createWriteStream(destFile);
    res.on('data', (chunk) => hash.update(chunk));
    res.on('error', (err) => {
      out.destroy();
      reject(err);
    });
    out.on('error', reject);
    out.on('close', resolve);
    res.pipe(out);
  });
  const actual = hash.digest('hex');
  if (!shaMatches(actual, expectedSha)) {
    fs.rmSync(destFile, { force: true });
    throw new Error(
      `SHA-256 mismatch for ${path.basename(url)}:\n` +
        `  expected: ${expectedSha}\n` +
        `  actual:   ${actual}\n` +
        `Partial download deleted.`
    );
  }
  return actual;
}

function runTar(args) {
  const result = spawnSync('tar', args, { stdio: 'inherit' });
  if (result.error) {
    throw new Error(`Failed to run tar (${result.error.message}). Is tar available on PATH?`);
  }
  if (result.status !== 0) {
    throw new Error(`tar exited with code ${result.status} for args: ${args.join(' ')}`);
  }
}

function extractArchive(archiveFile, dir) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  // The archive has a single top-level transcribe-native-<lane>/ dir; strip
  // it so libtranscribe.*, the ggml backend modules, contract.json and the
  // flattened licenses/ tree land directly in the staging directory.
  runTar(['-xzf', archiveFile, '--strip-components', '1', '-C', dir]);

  const contractPath = path.join(dir, 'contract.json');
  let contract;
  try {
    contract = JSON.parse(fs.readFileSync(contractPath, 'utf8'));
  } catch (err) {
    fs.rmSync(dir, { recursive: true, force: true });
    throw new Error(`contract.json missing or unreadable after extraction: ${err.message}`);
  }
  const problems = validateContract(contract);
  if (problems.length > 0) {
    fs.rmSync(dir, { recursive: true, force: true });
    throw new Error(`contract.json does not match the pinned ABI:\n  ${problems.join('\n  ')}`);
  }
}

function currentLaneOrExit() {
  const lane = resolveLane(os.platform(), os.arch());
  if (!lane) {
    console.error(
      `stage-transcribe-native: unsupported platform ${os.platform()}/${os.arch()}. ` +
        `Supported lanes: ${Object.keys(LANE_SHA256).join(', ')}.`
    );
    process.exit(1);
  }
  return lane;
}

export async function stage() {
  const lane = currentLaneOrExit();
  const expectedSha = LANE_SHA256[lane];
  const dir = stagingDir();

  if (isStagedComplete(dir, lane)) {
    console.log(
      `transcribe.cpp v${TRANSCRIBE_CPP_VERSION} (${lane}) already staged at ${dir} — skipping.`
    );
    return;
  }

  console.log(`Staging transcribe.cpp v${TRANSCRIBE_CPP_VERSION} lane ${lane}...`);
  const url = downloadUrlFor(lane);
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'transcribecpp-'));
  const archiveFile = path.join(tmpRoot, archiveNameFor(lane));
  try {
    console.log(`Downloading ${url}`);
    const actual = await downloadArchive(url, expectedSha, archiveFile);
    console.log(`SHA-256 verified: ${actual}`);
    extractArchive(archiveFile, dir);
    fs.writeFileSync(
      path.join(dir, STAMP_FILE),
      JSON.stringify({ version: TRANSCRIBE_CPP_VERSION, lane, sha256: actual }, null, 2) + '\n'
    );
    console.log(`Staged to ${dir}`);
  } finally {
    fs.rmSync(tmpRoot, { recursive: true, force: true });
  }
}

export function verify() {
  const lane = currentLaneOrExit();
  const dir = stagingDir();
  const missing = requiredFilesFor(lane)
    .map((rel) => path.join(dir, rel))
    .filter((p) => !fs.existsSync(p));
  if (missing.length > 0) {
    console.error('Missing staged transcribe.cpp runtime files:');
    for (const p of missing) console.error(`  ${p}`);
    console.error('Run: node scripts/stage-transcribe-native.mjs');
    process.exit(1);
  }
  const stamp = readStamp(dir);
  if (
    !stamp ||
    stamp.version !== TRANSCRIBE_CPP_VERSION ||
    stamp.lane !== lane ||
    !shaMatches(stamp.sha256, LANE_SHA256[lane])
  ) {
    console.error(
      `Staging stamp in ${dir} does not match the v${TRANSCRIBE_CPP_VERSION} pin for ${lane}.`
    );
    process.exit(1);
  }
  console.log(`transcribe.cpp runtime OK (${lane}) at ${dir}`);
}

const invokedDirectly =
  process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  if (process.argv[2] === '--verify') {
    verify();
  } else {
    stage().catch((err) => {
      console.error(`stage-transcribe-native: ${err.message}`);
      process.exit(1);
    });
  }
}
