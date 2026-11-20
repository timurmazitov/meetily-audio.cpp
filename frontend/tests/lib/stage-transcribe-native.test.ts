import { describe, expect, test } from 'bun:test';

/**
 * Pure-logic tests for the transcribe.cpp native staging script (ticket 01).
 * Network and tar are exercised by the real run; here we pin lane resolution,
 * SHA comparison, contract validation, and the required-file layout only.
 */

const {
  TRANSCRIBE_CPP_VERSION,
  LANE_SHA256,
  CONTRACT_PINS,
  resolveLane,
  archiveNameFor,
  downloadUrlFor,
  shaMatches,
  validateContract,
  libraryFileNameFor,
  requiredFilesFor,
} = await import('../../scripts/stage-transcribe-native.mjs');

describe('resolveLane', () => {
  test('maps supported platform/arch pairs to release lanes', () => {
    expect(resolveLane('linux', 'x64')).toBe('linux-x86_64-cpu-vulkan');
    expect(resolveLane('linux', 'arm64')).toBe('linux-aarch64-cpu-vulkan');
    expect(resolveLane('win32', 'x64')).toBe('windows-x86_64-cpu-vulkan');
    expect(resolveLane('darwin', 'arm64')).toBe('macos-arm64-metal');
  });

  test('rejects unsupported platform/arch combinations', () => {
    expect(resolveLane('darwin', 'x64')).toBeNull();
    expect(resolveLane('win32', 'arm64')).toBeNull();
    expect(resolveLane('freebsd', 'x64')).toBeNull();
  });
});

describe('lane pins and URLs', () => {
  test('every lane resolves to itself and has a 64-hex SHA-256 pin', () => {
    for (const lane of Object.keys(LANE_SHA256)) {
      expect(LANE_SHA256[lane]).toMatch(/^[0-9a-f]{64}$/);
      expect(archiveNameFor(lane)).toBe(`transcribe-native-${TRANSCRIBE_CPP_VERSION}-${lane}.tar.gz`);
      expect(downloadUrlFor(lane)).toBe(
        `https://github.com/handy-computer/transcribe.cpp/releases/download/v${TRANSCRIBE_CPP_VERSION}/${archiveNameFor(lane)}`
      );
    }
  });

  test('linux-x86_64 pin matches the release cross-check value', () => {
    expect(LANE_SHA256['linux-x86_64-cpu-vulkan']).toBe(
      '4fe87db8919ea34e461db7987713b580f54e6f3ed610457c5d8fb8eac1a15046'
    );
  });
});

describe('shaMatches', () => {
  test('accepts case-insensitive equality only', () => {
    const sha = 'a'.repeat(64);
    expect(shaMatches(sha, sha)).toBe(true);
    expect(shaMatches(sha.toUpperCase(), sha)).toBe(true);
    expect(shaMatches(`${sha}0`, sha)).toBe(false);
    expect(shaMatches(undefined as unknown as string, sha)).toBe(false);
    expect(shaMatches(sha, undefined as unknown as string)).toBe(false);
  });
});

describe('validateContract', () => {
  const good = {
    version: '0.4.2',
    header_hash: 'f99ad4169374b8f3',
    backends: ['vulkan', 'cpu'],
    lane: 'cpu-vulkan',
  };

  test('accepts the published v0.4.2 contract shape', () => {
    expect(validateContract(good)).toEqual([]);
  });

  test('reports mismatched version or header_hash and missing backends', () => {
    expect(validateContract({ ...good, version: '0.4.1' })).toHaveLength(1);
    expect(validateContract({ ...good, header_hash: '0000000000000000' })).toHaveLength(1);
    expect(validateContract({ ...good, backends: [] })).toHaveLength(1);
    expect(validateContract(null as unknown as object).length).toBeGreaterThan(0);
  });

  test('contract pins match the pinned version', () => {
    expect(CONTRACT_PINS.version).toBe(TRANSCRIBE_CPP_VERSION);
  });
});

describe('requiredFilesFor', () => {
  test('uses the per-platform library name next to contract and licenses', () => {
    expect(libraryFileNameFor('linux-x86_64-cpu-vulkan')).toBe('libtranscribe.so');
    expect(libraryFileNameFor('windows-x86_64-cpu-vulkan')).toBe('transcribe.dll');
    expect(libraryFileNameFor('macos-arm64-metal')).toBe('libtranscribe.dylib');
    for (const lane of Object.keys(LANE_SHA256)) {
      expect(requiredFilesFor(lane)).toEqual([libraryFileNameFor(lane), 'contract.json', 'licenses/LICENSE']);
    }
  });
});
