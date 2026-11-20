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
const managerRenders: Array<Record<string, unknown>> = [];
mock.module('../../src/components/AudioCppModelManager', () => ({
  AudioCppModelManager: (props: Record<string, unknown>) => {
    managerRenders.push({ ...props, manager: 'audioCpp' });
    return <div data-testid="fake-audiocpp-model-manager" />;
  },
}));
const originalManager = { ...(await import('../../src/components/AudioCppModelManager')) };
afterAll(() => {
  mock.module('../../src/components/AudioCppModelManager', () => originalManager);
});

mock.module('../../src/components/TranscribeCppModelManager', () => ({
  TranscribeCppModelManager: (props: Record<string, unknown>) => {
    managerRenders.push({ ...props, manager: 'transcribeCpp' });
    return <div data-testid="fake-transcribecpp-model-manager" />;
  },
}));
const originalTcpManager = { ...(await import('../../src/components/TranscribeCppModelManager')) };
afterAll(() => {
  mock.module('../../src/components/TranscribeCppModelManager', () => originalTcpManager);
});

mock.module('sonner', () => ({
  toast: { info: () => {}, error: () => {}, success: () => {}, warning: () => {} },
}));
const originalToast = { ...(await import('sonner')) };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});

const { TranscriptSettings } = await import('../../src/components/TranscriptSettings');

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  managerRenders.length = 0;
});

async function render() {
  await act(async () => {
    renderer = create(
      <TranscriptSettings
        transcriptModelConfig={{ provider: 'transcribeCpp', model: 'some-model' }}
        setTranscriptModelConfig={() => {}}
      />,
    );
  });
  return renderer!;
}

function textOf(node: ReactTestRenderer) {
  return JSON.stringify(node.toJSON());
}

test('the Speaker diarization section is visible', async () => {
  const view = await render();

  expect(textOf(view)).toContain('Speaker diarization');
  expect(managerRenders.filter(props => props.manager === 'audioCpp')).toHaveLength(1);
});

test('the diarization section never offers model selection: it cannot transcribe', async () => {
  await render();

  const [diarization] = managerRenders.filter(props => props.manager === 'audioCpp');
  expect(diarization).toBeDefined();
  // No onModelSelect means the manager's cards stay non-selectable.
  expect(diarization.onModelSelect).toBeUndefined();
});

test('the transcription section offers only the Transcribe.CPP table', async () => {
  await render();

  // Two managers total: the transcribe.cpp model table and the shared
  // diarization section below it — no Whisper/Parakeet/audioCpp-ASR manager,
  // and the GigaAM ASR block is gone (the audio.cpp manager is diarization-
  // only; its own suite asserts the ASR cards never render).
  expect(managerRenders).toHaveLength(2);
  expect(managerRenders.filter(props => props.manager === 'transcribeCpp')).toHaveLength(1);
  expect(managerRenders.filter(props => props.manager === 'audioCpp')).toHaveLength(1);
});
