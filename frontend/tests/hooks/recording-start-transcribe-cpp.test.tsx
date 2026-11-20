/**
 * Recording start with a normalized (transcribe.cpp) configuration.
 *
 * The Rust settings repository soft-migrates removed providers to
 * transcribeCpp on read, so by the time the start flow asks which provider
 * is configured it only ever sees transcribe.cpp (or a cloud id). The gate
 * must pass for that configuration: the readiness check runs the
 * transcribe.cpp commands and recording starts. The client-side fallback
 * when the config is missing must be the same provider, never a removed
 * engine — a Whisper/Parakeet fallback would fail the gate on a machine
 * with only transcribe.cpp models installed.
 */

import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const MOCKED = [
  '@tauri-apps/api/core',
  '@tauri-apps/api/event',
  '../../src/contexts/TranscriptContext',
  '../../src/components/Sidebar/SidebarProvider',
  '../../src/contexts/ConfigContext',
  '../../src/contexts/RecordingStateContext',
  '../../src/services/recordingService',
  '../../src/lib/recordingNotification',
  'sonner',
] as const;

// `any` values: the stubs are spread into mock.module factories, which take
// arbitrary export shapes.
const originals = {} as Record<string, any>;
for (const path of MOCKED) {
  originals[path] = { ...(await import(path)) };
}
afterAll(() => {
  for (const path of MOCKED) mock.module(path, () => originals[path]);
});

class MemoryStorage {
  private readonly items = new Map<string, string>();
  getItem(key: string) { return this.items.get(key) ?? null; }
  setItem(key: string, value: string) { this.items.set(key, value); }
  removeItem(key: string) { this.items.delete(key); }
  clear() { this.items.clear(); }
}
const globals = globalThis as unknown as { window?: unknown; sessionStorage: MemoryStorage };

let transcriptConfigResponse: unknown;
const invokeCalls: string[] = [];
const invoke = mock(async (command: string, _args?: unknown): Promise<unknown> => {
  invokeCalls.push(command);
  if (command === 'api_get_transcript_config') return transcriptConfigResponse;
  if (command === 'transcribecpp_model_init') return undefined;
  if (command === 'transcribecpp_has_transcription_model') return true;
  if (command === 'transcribecpp_get_available_models') return [{ status: 'Available' }];
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originals['@tauri-apps/api/core'], invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originals['@tauri-apps/api/event'], listen: async () => () => {} }));
mock.module('../../src/contexts/TranscriptContext', () => ({
  useTranscripts: () => ({ clearTranscripts: () => {}, setMeetingTitle: () => {} }),
}));
mock.module('../../src/components/Sidebar/SidebarProvider', () => ({
  useSidebar: () => ({ setIsMeetingActive: () => {} }),
}));
mock.module('../../src/contexts/ConfigContext', () => ({
  useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null } }),
}));
mock.module('../../src/contexts/RecordingStateContext', () => ({
  RecordingStatus: {
    IDLE: 'idle',
    STARTING: 'starting',
    RECORDING: 'recording',
    STOPPING: 'stopping',
    PROCESSING_TRANSCRIPTS: 'processing',
    SAVING: 'saving',
    COMPLETED: 'completed',
    ERROR: 'error',
  },
  useRecordingState: () => ({ setStatus: () => {} }),
}));
const startRecordingWithDevices = mock(async () => ({ meetingId: '1' }));
mock.module('../../src/services/recordingService', () => ({
  ...originals['../../src/services/recordingService'],
  recordingService: { startRecordingWithDevices },
}));
mock.module('../../src/lib/recordingNotification', () => ({
  showRecordingNotification: async () => {},
}));
mock.module('sonner', () => ({
  ...originals['sonner'],
  toast: { error: () => {}, success: () => {}, info: () => {} },
}));

// Re-applied before every test so a later-loaded suite's `invoke` fake (or
// this file's own earlier `beforeEach` state) cannot leave stale stubs in
// place — same pattern as `recording-controls-stop.test.tsx`.
function applyTestMocks() {
  mock.module('@tauri-apps/api/core', () => ({ ...originals['@tauri-apps/api/core'], invoke }));
  mock.module('@tauri-apps/api/event', () => ({ ...originals['@tauri-apps/api/event'], listen: async () => () => {} }));
  mock.module('../../src/contexts/TranscriptContext', () => ({
    useTranscripts: () => ({ clearTranscripts: () => {}, setMeetingTitle: () => {} }),
  }));
  mock.module('../../src/components/Sidebar/SidebarProvider', () => ({
    useSidebar: () => ({ setIsMeetingActive: () => {} }),
  }));
  mock.module('../../src/contexts/ConfigContext', () => ({
    useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null } }),
  }));
  mock.module('../../src/contexts/RecordingStateContext', () => ({
    RecordingStatus: {
      IDLE: 'idle',
      STARTING: 'starting',
      RECORDING: 'recording',
      STOPPING: 'stopping',
      PROCESSING_TRANSCRIPTS: 'processing',
      SAVING: 'saving',
      COMPLETED: 'completed',
      ERROR: 'error',
    },
    useRecordingState: () => ({ setStatus: () => {} }),
  }));
  mock.module('../../src/services/recordingService', () => ({
    ...originals['../../src/services/recordingService'],
    recordingService: { startRecordingWithDevices },
  }));
  mock.module('../../src/lib/recordingNotification', () => ({
    showRecordingNotification: async () => {},
  }));
  mock.module('sonner', () => ({
    ...originals['sonner'],
    toast: { error: () => {}, success: () => {}, info: () => {} },
  }));
}
applyTestMocks();
beforeEach(applyTestMocks);

const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');

function Harness() {
  const hook = useRecordingStart(false, () => {}, undefined);
  (Harness as unknown as { latest: typeof hook }).latest = hook;
  return null;
}

let renderer: ReactTestRenderer | undefined;

beforeEach(() => {
  invokeCalls.length = 0;
  startRecordingWithDevices.mockClear();
  transcriptConfigResponse = { provider: 'transcribeCpp', model: 'parakeet-ultra-Q8_0' };
  globals.window = {
    addEventListener: () => {},
    removeEventListener: () => {},
    // The same shim `copy-export-operations.test.tsx` installs: when bun's
    // cross-file mock order leaves the REAL `@tauri-apps/api/core` bound in
    // the hook, its invoke delegates to this global instead of throwing.
    __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => invoke(command, args) },
  };
  globals.sessionStorage = new MemoryStorage();
});

afterEach(() => {
  renderer?.unmount();
  renderer = undefined;
});

async function start() {
  await act(async () => {
    renderer = create(<Harness />);
  });
  await act(async () => {
    await (Harness as unknown as { latest: ReturnType<typeof useRecordingStart> }).latest
      .handleRecordingStart();
  });
}

describe('recording start with a normalized configuration', () => {
  test('the readiness gate passes through the transcribe.cpp commands', async () => {
    await start();

    expect(invokeCalls).toContain('transcribecpp_model_init');
    expect(invokeCalls).toContain('transcribecpp_has_transcription_model');
    expect(startRecordingWithDevices).toHaveBeenCalledTimes(1);
  });

  test('a missing config falls back to transcribe.cpp, not a removed engine', async () => {
    transcriptConfigResponse = null;
    await start();

    expect(invokeCalls).toContain('transcribecpp_has_transcription_model');
    for (const removed of [
      'whisper_init',
      'whisper_has_available_models',
      'parakeet_init',
      'parakeet_has_available_models',
      'audiocpp_model_init',
      'audiocpp_has_transcription_model',
    ]) {
      expect(invokeCalls).not.toContain(removed);
    }
    expect(startRecordingWithDevices).toHaveBeenCalledTimes(1);
  });
});
