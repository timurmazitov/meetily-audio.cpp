import React from 'react';
import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Capture the pristine module exports BEFORE any mock is installed: a Bun
// module mock persists across test files in one process, so an "original"
// snapshot taken after mocking would restore the stub itself and poison the
// suites that run later.
const originalPaths = [
  '../../src/components/ui/select',
  '../../src/components/AudioCppModelManager',
  '../../src/components/TranscribeCppModelManager',
] as const;
const originals = await Promise.all(
  originalPaths.map(async path => [path, { ...(await import(path)) }] as const),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

// Radix Select needs a DOM and a portal; the component under test only wires
// values and callbacks, so plain containers keep that wiring observable —
// and record each Select's props so a test can drive a change.
interface SelectRender {
  value: unknown;
  onValueChange?: (value: string) => void;
}
const selectRenders: SelectRender[] = [];
mock.module('../../src/components/ui/select', () => {
  const Container = ({ children }: any) => <div>{children}</div>;
  const Select = ({ children, value, onValueChange }: any) => {
    selectRenders.push({ value, onValueChange });
    return <div>{children}</div>;
  };
  return {
    Select,
    SelectContent: Container,
    SelectGroup: Container,
    SelectItem: Container,
    SelectLabel: Container,
    SelectValue: Container,
    SelectTrigger: Container,
    SelectSeparator: Container,
  };
});

// Each manager has its own suite; stubbing them keeps this test free of Tauri
// listeners and of the model catalogs.
const managerRenders: Array<Record<string, unknown>> = [];
mock.module('../../src/components/AudioCppModelManager', () => ({
  AudioCppModelManager: (props: Record<string, unknown>) => {
    managerRenders.push({ ...props, manager: 'audioCpp' });
    return <div data-testid="fake-audiocpp-model-manager" />;
  },
}));

mock.module('../../src/components/TranscribeCppModelManager', () => ({
  TranscribeCppModelManager: (props: Record<string, unknown>) => {
    managerRenders.push({ ...props, manager: 'transcribeCpp' });
    return <div data-testid="fake-transcribecpp-model-manager" />;
  },
}));

// Provider and mode changes persist the config through the Tauri bridge;
// capture that invoke. Sibling suites can leave a partial mock of the bridge
// registered (or restore the real module) around this file's run, so the
// pristine exports are loaded through the file URL — that resolution bypasses
// the mock registry — the fake is re-applied before every test, and
// `window.__TAURI_INTERNALS__.invoke` is shimmed to the same script: the real
// bridge delegates through that seam at call time, so the save lands in
// `invokeCalls` whichever binding the component ended up with. (Same pattern
// as summary-system-prompt-settings.test.tsx.)
const realCorePath = new URL('../../node_modules/@tauri-apps/api/core.js', import.meta.url).href;
const originalCore = { ...(await import(realCorePath)) };
interface InvokeCall {
  command: string;
  args?: unknown;
}
const invokeCalls: InvokeCall[] = [];
const scriptedInvoke = async (command: string, args?: unknown) => {
  invokeCalls.push({ command, args });
  return undefined;
};
const mockCore = () => mock.module('@tauri-apps/api/core', () => ({
  ...originalCore,
  invoke: scriptedInvoke,
}));
mockCore();
beforeEach(() => {
  mockCore();
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: { invoke: scriptedInvoke },
  };
});
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  Reflect.deleteProperty(globalThis, 'window');
});

// The managers and the select are mocked, so nothing else in the module graph
// pulls in the i18next bootstrap; load it explicitly so `t()` resolves keys.
await import('../../src/i18n');

// Re-evaluate the component under THIS file's mocks: another suite may have
// imported it (and bound its Tauri bridge) before this file ran, and a cached
// module keeps the bridge it captured at its own import time. The specifier is
// a variable so TypeScript does not try to resolve the cache-busting query.
const settingsPath = '../../src/components/TranscriptSettings?v=tcp-05';
const settingsModule: typeof import('../../src/components/TranscriptSettings') =
  await import(settingsPath);
const { TranscriptSettings } = settingsModule;

let renderer: ReactTestRenderer | undefined;
let savedConfig: { provider: string; model: string; transcribeBackend?: string } | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  managerRenders.length = 0;
  selectRenders.length = 0;
  savedConfig = undefined;
  invokeCalls.length = 0;
});

async function render(
  model = 'some-model',
  transcribeBackend?: 'auto' | 'vulkan' | 'cpu',
) {
  await act(async () => {
    renderer = create(
      <TranscriptSettings
        transcriptModelConfig={{ provider: 'transcribeCpp', model, transcribeBackend }}
        setTranscriptModelConfig={config => { savedConfig = config; }}
      />,
    );
  });
  return renderer!;
}

function textOf(node: ReactTestRenderer) {
  return JSON.stringify(node.toJSON());
}

test('the engine selector is gone: no dropdown offers a removed provider', async () => {
  const view = await render('parakeet-ultra-Q8_0');

  // The selector's labels and options must not appear anywhere.
  const text = textOf(view);
  expect(text).not.toContain('Select provider');
  for (const removedLabel of ['Local Whisper', 'Parakeet (Recommended', 'audioCpp']) {
    expect(text).not.toContain(removedLabel);
  }
});

test('the Transcribe.CPP model table shows without any engine choice', async () => {
  await render('parakeet-ultra-Q8_0');

  const transcription = managerRenders.filter(props => props.manager === 'transcribeCpp');
  expect(transcription.length).toBeGreaterThan(0);
  expect(transcription.at(-1)!.autoSave).toBe(true);
  expect(typeof transcription.at(-1)!.onModelSelect).toBe('function');
  // The shared diarization section stays audio.cpp's.
  expect(managerRenders.some(props => props.manager === 'audioCpp')).toBe(true);
});

test('the transcribeCpp manager receives the configured model', async () => {
  await render('parakeet-ultra-Q8_0');

  const [transcription] = managerRenders.filter(props => props.manager === 'transcribeCpp');
  expect(transcription.selectedModel).toBe('parakeet-ultra-Q8_0');
});

test('exactly one dropdown remains: the compute-mode selector', async () => {
  // Every rendered Select is a dropdown. With the engine selector gone only
  // the compute-mode selector is left: no cloud model list, no provider list.
  await render('parakeet-ultra-Q8_0');

  expect(selectRenders).toHaveLength(1);
  const values = selectRenders.map(entry => entry.value);
  expect(values).toContain('auto'); // compute-mode selector
  expect(values).not.toContain('transcribeCpp'); // no provider selector
  expect(values).not.toContain('parakeet-ultra-Q8_0'); // no cloud model list
});

test('the compute-mode selector offers Auto, Vulkan and CPU', async () => {
  const view = await render('parakeet-ultra-Q8_0');

  const text = textOf(view);
  expect(text).toContain('Compute mode');
  expect(text).toContain('Auto');
  expect(text).toContain('Vulkan');
  expect(text).toContain('CPU');
});

test('choosing Vulkan persists the mode through the transcript config save', async () => {
  await render('parakeet-ultra-Q8_0');

  const modeSelect = selectRenders.find(entry => entry.value === 'auto');
  expect(modeSelect?.onValueChange).toBeDefined();
  await act(async () => { modeSelect!.onValueChange?.('vulkan'); });

  const saveCall = invokeCalls.find(
    call => call.command === 'api_save_transcript_config',
  );
  expect(saveCall).toBeDefined();
  expect(saveCall!.args).toEqual({
    provider: 'transcribeCpp',
    model: 'parakeet-ultra-Q8_0',
    apiKey: null,
    transcribeBackend: 'vulkan',
  });
  expect(savedConfig?.transcribeBackend).toBe('vulkan');
});

test('the manager autoSave carries the saved mode so a model change keeps it', async () => {
  await render('parakeet-ultra-Q8_0', 'vulkan');

  const [transcription] = managerRenders.filter(props => props.manager === 'transcribeCpp');
  const saveSelection = transcription!.saveSelection as (config: {
    provider: string;
    model: string;
    apiKey: null;
  }) => Promise<void>;
  expect(typeof saveSelection).toBe('function');

  await act(async () => {
    await saveSelection({ provider: 'transcribeCpp', model: 'gigaam-v3-ctc-Q8_0', apiKey: null });
  });

  const saveCall = invokeCalls.find(
    call => call.command === 'api_save_transcript_config',
  );
  expect(saveCall).toBeDefined();
  // The model payload from the manager gains the current mode: selecting a
  // model must not reset the user's «режим вычислений».
  expect(saveCall!.args).toEqual({
    provider: 'transcribeCpp',
    model: 'gigaam-v3-ctc-Q8_0',
    apiKey: null,
    transcribeBackend: 'vulkan',
  });
});
