import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Radix Select needs a DOM and a portal; the component under test only wires
// values and callbacks, so plain containers keep that wiring observable.
mock.module('../../src/components/ui/select', () => {
  const Container = ({ children }: any) => <div>{children}</div>;
  return {
    Select: Container,
    SelectContent: Container,
    SelectGroup: Container,
    SelectItem: Container,
    SelectLabel: Container,
    SelectValue: Container,
    SelectTrigger: Container,
    SelectSeparator: Container,
  };
});
const originalSelect = { ...(await import('../../src/components/ui/select')) };
afterAll(() => {
  mock.module('../../src/components/ui/select', () => originalSelect);
});

// The transcript-settings section only decides *which* manager to render and
// with which task; each manager has its own suite. Stubbing them keeps this
// test free of Tauri listeners and of the model catalogs.
mock.module('../../src/components/WhisperModelManager', () => ({
  ModelManager: () => <div data-testid="fake-whisper-model-manager" />,
}));
const originalWhisper = { ...(await import('../../src/components/WhisperModelManager')) };
afterAll(() => {
  mock.module('../../src/components/WhisperModelManager', () => originalWhisper);
});

mock.module('../../src/components/ParakeetModelManager', () => ({
  ParakeetModelManager: () => <div data-testid="fake-parakeet-model-manager" />,
}));
const originalParakeet = { ...(await import('../../src/components/ParakeetModelManager')) };
afterAll(() => {
  mock.module('../../src/components/ParakeetModelManager', () => originalParakeet);
});

const managerRenders: Array<Record<string, unknown>> = [];
mock.module('../../src/components/AudioCppModelManager', () => ({
  AudioCppModelManager: (props: Record<string, unknown>) => {
    managerRenders.push(props);
    return <div data-testid="fake-audiocpp-model-manager" />;
  },
}));
const originalManager = { ...(await import('../../src/components/AudioCppModelManager')) };
afterAll(() => {
  mock.module('../../src/components/AudioCppModelManager', () => originalManager);
});

mock.module('sonner', () => ({
  toast: { info: () => {}, error: () => {}, success: () => {}, warning: () => {} },
}));
const originalToast = { ...(await import('sonner')) };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});

const { TranscriptSettings } = await import('../../src/components/TranscriptSettings');

type Provider = 'localWhisper' | 'parakeet' | 'audioCpp';

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  managerRenders.length = 0;
});

async function render(provider: Provider) {
  await act(async () => {
    renderer = create(
      <TranscriptSettings
        transcriptModelConfig={{ provider, model: 'some-model' }}
        setTranscriptModelConfig={() => {}}
      />,
    );
  });
  return renderer!;
}

function textOf(node: ReactTestRenderer) {
  return JSON.stringify(node.toJSON());
}

test.each(['localWhisper', 'parakeet', 'audioCpp'] as Provider[])(
  'the Speaker diarization section is visible with the %s provider',
  async provider => {
    const view = await render(provider);

    expect(textOf(view)).toContain('Speaker diarization');
    const diarizationSections = managerRenders.filter(props => props.task === 'diar');
    expect(diarizationSections).toHaveLength(1);
  },
);

test('the diarization section never offers model selection: it cannot transcribe', async () => {
  await render('localWhisper');

  const [diarization] = managerRenders.filter(props => props.task === 'diar');
  expect(diarization).toBeDefined();
  // No onModelSelect means the manager's cards stay non-selectable.
  expect(diarization.onModelSelect).toBeUndefined();
});

test('the audioCpp provider keeps its transcription manager next to the diarization section', async () => {
  await render('audioCpp');

  // Two managers: the in-provider transcription list, and the shared
  // diarization section below it.
  expect(managerRenders).toHaveLength(2);
  expect(managerRenders.filter(props => props.task !== 'diar')).toHaveLength(1);
  expect(managerRenders.filter(props => props.task === 'diar')).toHaveLength(1);
});

test('a non-audioCpp provider renders no transcription models from audio.cpp', async () => {
  await render('parakeet');

  expect(managerRenders).toHaveLength(1);
  expect(managerRenders[0]!.task).toBe('diar');
});
