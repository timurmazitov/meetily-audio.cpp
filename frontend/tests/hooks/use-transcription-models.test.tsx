import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

interface RawModel {
  name: string;
  size_mb: number;
  status: unknown;
}

interface AudioCppRawModel extends RawModel {
  display_name: string;
  task: 'asr' | 'diar';
}

let whisperModels: RawModel[] = [];
let parakeetModels: RawModel[] = [];
let audioCppModels: AudioCppRawModel[] = [];
let audioCppFails = false;

const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'whisper_get_available_models') return whisperModels;
  if (command === 'parakeet_get_available_models') return parakeetModels;
  if (command === 'audiocpp_get_available_models') {
    if (audioCppFails) throw new Error('audio.cpp model manager not initialized');
    return audioCppModels;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));

const { useTranscriptionModels } = await import('../../src/hooks/useTranscriptionModels');

function Harness({ config }: { config: any }) {
  const hook = useTranscriptionModels(config);
  (Harness as any).latest = hook;
  return null;
}

let renderer: ReactTestRenderer | undefined;

async function renderHook(config: any) {
  await act(async () => {
    renderer = create(<Harness config={config} />);
  });
}

/// The hook object is recreated every render; always read the latest one
/// after an `act` block.
function latest() {
  return (Harness as any).latest as ReturnType<typeof useTranscriptionModels>;
}

beforeEach(() => {
  whisperModels = [];
  parakeetModels = [];
  audioCppModels = [];
  audioCppFails = false;
  renderer?.unmount();
  renderer = undefined;
});

describe('useTranscriptionModels with audioCpp', () => {
  test('lists installed audioCpp transcription models alongside Whisper and Parakeet', async () => {
    whisperModels = [{ name: 'small', size_mb: 461, status: 'Available' }];
    parakeetModels = [{ name: 'parakeet-tdt-0.6b-v3-int8', size_mb: 700, status: 'Available' }];
    audioCppModels = [
      { name: 'gigaam-v3-e2e-ctc-f16', display_name: 'GigaAM v3 e2e CTC', task: 'asr', size_mb: 422, status: 'Available' },
      // A diarization model is installed but can never produce a transcript:
      // it must not appear in the model options of these dialogs.
      { name: 'nemotron-3-diarization-bf16', display_name: 'Nemotron-3 Diarization', task: 'diar', size_mb: 190, status: 'Available' },
    ];

    await renderHook({ provider: 'localWhisper', model: 'small' });
    await act(async () => {
      await latest().fetchModels();
    });

    const providers = latest().availableModels.map((m) => `${m.provider}:${m.name}`);
    expect(providers).toContain('whisper:small');
    expect(providers).toContain('parakeet:parakeet-tdt-0.6b-v3-int8');
    expect(providers).toContain('audioCpp:gigaam-v3-e2e-ctc-f16');
    expect(providers).not.toContain('audioCpp:nemotron-3-diarization-bf16');
  });

  test('selects the configured audioCpp model by default', async () => {
    audioCppModels = [
      { name: 'gigaam-v3-e2e-ctc-f16', display_name: 'GigaAM v3 e2e CTC', task: 'asr', size_mb: 422, status: 'Available' },
    ];

    await renderHook({ provider: 'audioCpp', model: 'gigaam-v3-e2e-ctc-f16' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().selectedModelKey).toBe('audioCpp:gigaam-v3-e2e-ctc-f16');
  });

  test('skips audioCpp models that are not fully installed', async () => {
    audioCppModels = [
      { name: 'gigaam-v3-e2e-ctc-f16', display_name: 'GigaAM v3 e2e CTC', task: 'asr', size_mb: 422, status: 'Downloading' },
    ];

    await renderHook({ provider: 'audioCpp', model: 'gigaam-v3-e2e-ctc-f16' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().availableModels).toHaveLength(0);
    // Nothing matched the configured model and no fallback exists.
    expect(latest().selectedModelKey).toBe('');
  });

  test('an audioCpp manager failure does not break the other providers', async () => {
    audioCppFails = true;
    whisperModels = [{ name: 'small', size_mb: 461, status: 'Available' }];

    await renderHook({ provider: 'localWhisper', model: 'small' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().availableModels.map((m) => m.provider)).toEqual(['whisper']);
    expect(latest().selectedModelKey).toBe('whisper:small');
  });
});
