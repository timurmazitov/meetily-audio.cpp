/**
 * Client-side transcription defaults after the transcribe.cpp consolidation.
 *
 * The settings repository normalizes stored removed providers to
 * transcribeCpp on read (Rust seam); the client must agree with that on every
 * default it owns, so a fresh install or a config read with nothing in the
 * database lands on transcribe.cpp with the catalog default model. The
 * Whisper engine's fields (`whisperModel`, whisper provider values) are out
 * of the client defaults.
 */

import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const DEFAULT_MODEL = 'parakeet-ultra-Q8_0';

const originalCore = { ...(await import('@tauri-apps/api/core')) };
const originalEvent = { ...(await import('@tauri-apps/api/event')) };
const originalConfigService = { ...(await import('../../src/services/configService')) };
const originalSonner = { ...(await import('sonner')) };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvent);
  mock.module('../../src/services/configService', () => originalConfigService);
  mock.module('sonner', () => originalSonner);
});

// Bun has no DOM; ConfigContext reads `window`/`localStorage` behind guards.
class MemoryStorage {
  private readonly items = new Map<string, string>();
  getItem(key: string) { return this.items.get(key) ?? null; }
  setItem(key: string, value: string) { this.items.set(key, value); }
  removeItem(key: string) { this.items.delete(key); }
  clear() { this.items.clear(); }
}
const globals = globalThis as unknown as {
  window?: Record<string, unknown>;
  localStorage?: MemoryStorage;
};

// What `api_get_transcript_config` / `api_get_model_config` answer per test.
let transcriptResponse: unknown = null;
let modelResponse: unknown = null;

const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_get_transcript_config') return transcriptResponse;
  if (command === 'api_get_model_config') return modelResponse;
  if (command === 'get_ollama_models') return [];
  if (command === 'api_get_api_key') throw new Error('no key');
  if (command === 'set_language_preference') return undefined;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({
  ...originalEvent,
  listen: async () => () => {},
}));
mock.module('../../src/services/configService', () => ({
  ...originalConfigService,
  configService: {
    getTranscriptConfig: async () => transcriptResponse,
    getModelConfig: async () => modelResponse,
    getCustomOpenAIConfig: async () => null,
    getRecordingPreferences: async () => ({
      device_selection_version: [1, 0],
      preferred_mic_device: null,
      preferred_system_device: null,
    }),
  },
}));
// Re-applied before every test: bun installs every file's load-time mocks
// before running any test, so a later-loaded suite's stubs otherwise stay in
// place while this file's tests run (same pattern as
// `recording-controls-stop.test.tsx`).
function applyTestMocks() {
  mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
  mock.module('@tauri-apps/api/event', () => ({
    ...originalEvent,
    listen: async () => () => {},
  }));
  mock.module('../../src/services/configService', () => ({
    ...originalConfigService,
    configService: {
      getTranscriptConfig: async () => transcriptResponse,
      getModelConfig: async () => modelResponse,
      getCustomOpenAIConfig: async () => null,
      getRecordingPreferences: async () => ({
        device_selection_version: [1, 0],
        preferred_mic_device: null,
        preferred_system_device: null,
      }),
    },
  }));
  mock.module('sonner', () => ({
    ...originalSonner,
    toast: { error: () => {}, success: () => {}, info: () => {} },
  }));
}
applyTestMocks();
beforeEach(applyTestMocks);

// Cache-busting query: earlier suites in the run mock the context module
// (the sidebar suites bind it to a fixed stub), and without a distinct
// specifier we would inherit one of their instances — same pattern as
// `transcript-settings-transcribecpp-section.test.tsx`.
const contextPath = '../../src/contexts/ConfigContext?v=tcp-01-defaults';
const { ConfigProvider, useConfig } = await import(contextPath);
const { MODEL_DEFAULTS, DEFAULT_TRANSCRIBE_CPP_MODEL } = await import(
  '../../src/constants/modelDefaults'
);

function Probe() {
  const context = useConfig();
  (Probe as unknown as { latest: typeof context }).latest = context;
  return null;
}

let renderer: ReactTestRenderer | undefined;

async function renderProvider() {
  await act(async () => {
    renderer = create(
      <ConfigProvider>
        <Probe />
      </ConfigProvider>,
    );
  });
  // Let the mount effects' promises settle.
  await act(async () => {});
}

function latest() {
  return (Probe as unknown as { latest: ReturnType<typeof useConfig> }).latest;
}

beforeEach(() => {
  renderer?.unmount();
  renderer = undefined;
  transcriptResponse = null;
  modelResponse = null;
  globals.window = {};
  globals.localStorage = new MemoryStorage();
});

describe('transcription client defaults', () => {
  test('the model defaults name only transcribe.cpp and its default model', () => {
    expect(DEFAULT_TRANSCRIBE_CPP_MODEL).toBe(DEFAULT_MODEL);
    // The removed engines' provider values are not defaults anywhere.
    expect(Object.keys(MODEL_DEFAULTS).sort()).toEqual(['transcribeCpp']);
    expect(MODEL_DEFAULTS.transcribeCpp).toBe(DEFAULT_MODEL);
  });

  test('with an empty database the transcript config defaults to transcribe.cpp', async () => {
    transcriptResponse = null;
    await renderProvider();

    expect(latest().transcriptModelConfig.provider).toBe('transcribeCpp');
    expect(latest().transcriptModelConfig.model).toBe(DEFAULT_MODEL);
  });

  test('a saved config without provider or model falls back to transcribe.cpp', async () => {
    // The repository normalizes on read; the client's own fallback must land
    // on the same pair, never on a removed engine.
    transcriptResponse = { provider: '', model: '', apiKey: null };
    await renderProvider();

    expect(latest().transcriptModelConfig.provider).toBe('transcribeCpp');
    expect(latest().transcriptModelConfig.model).toBe(DEFAULT_MODEL);
  });

  test('a normalized saved config round-trips unchanged', async () => {
    transcriptResponse = {
      provider: 'transcribeCpp',
      model: 'gigaam-v3-ctc-Q8_0',
      apiKey: null,
      transcribeBackend: 'vulkan',
    };
    await renderProvider();

    expect(latest().transcriptModelConfig.provider).toBe('transcribeCpp');
    expect(latest().transcriptModelConfig.model).toBe('gigaam-v3-ctc-Q8_0');
    expect(latest().transcriptModelConfig.transcribeBackend).toBe('vulkan');
  });

  test('the summary model config defaults carry no Whisper model name', async () => {
    modelResponse = { provider: 'builtin-ai', model: 'qwen3.5:2b' };
    await renderProvider();

    // The legacy `settings.whisperModel` column stays in the schema but the
    // client no longer seeds it with a Whisper engine model.
    expect(latest().modelConfig.whisperModel || '').toBe('');
  });
});
