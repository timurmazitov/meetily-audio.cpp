import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import {
  AUDIOCPP_DIARIZATION_MODEL,
  AUDIOCPP_TRANSCRIPTION_MODEL,
} from '../../src/lib/onboarding-audiocpp-status';
import type { AudioCppModelInfo } from '../../src/lib/audiocpp';

const originals = await Promise.all(
  ['@tauri-apps/api/core', '@tauri-apps/api/event'].map(
    async path => [path, { ...(await import(path)) }] as const,
  ),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

/** Per-test native responses, keyed by command name. */
class Native {
  transcriptionInstalled = false;
  models: AudioCppModelInfo[] = [];
  completedModel: string | undefined;

  async invoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
    switch (command) {
      case 'get_onboarding_status':
        return null;
      case 'check_first_launch':
        return false;
      case 'builtin_ai_get_recommended_model':
        return 'qwen3.5:4b';
      case 'builtin_ai_is_model_ready':
        return true;
      case 'audiocpp_model_init':
        return undefined;
      case 'audiocpp_has_transcription_model':
        return this.transcriptionInstalled;
      case 'audiocpp_get_available_models':
        return this.models;
      case 'audiocpp_download_model':
        return undefined;
      case 'complete_onboarding':
        this.completedModel = (args as { model?: string } | undefined)?.model;
        return undefined;
      case 'save_onboarding_status_cmd':
      case 'builtin_ai_download_model':
        return undefined;
      default:
        throw new Error(`unexpected command in test: ${command}`);
    }
  }
}

let native = new Native();

mock.module('@tauri-apps/api/core', () => ({
  invoke: (command: string, args?: Record<string, unknown>) => native.invoke(command, args),
}));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));

const { OnboardingProvider, useOnboarding } = await import(
  '../../src/contexts/OnboardingContext'
);

type Context = ReturnType<typeof useOnboarding>;

let captured: Context | undefined;
function Probe() {
  captured = useOnboarding();
  return null;
}

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  captured = undefined;
  native = new Native();
});

async function renderProvider() {
  await act(async () => {
    renderer = create(
      <OnboardingProvider>
        <Probe />
      </OnboardingProvider>,
    );
  });
  // Let the mount effects settle (status load, database check, recommendation).
  await act(async () => {
    await Promise.resolve();
  });
  return captured!;
}

function installedModels(transcription: boolean, diarization: boolean): AudioCppModelInfo[] {
  const model = (name: string, available: boolean): AudioCppModelInfo => ({
    name,
    display_name: name,
    family: 'audio-cpp',
    task: name === AUDIOCPP_TRANSCRIPTION_MODEL ? 'asr' : 'diar',
    quantization: 'F16',
    filename: `${name}.gguf`,
    path: `/models/${name}.gguf`,
    size_mb: 422,
    size_bytes: 422 * 1024 * 1024,
    description: '',
    status: available ? 'Available' : 'Missing',
  });

  return [
    model(AUDIOCPP_TRANSCRIPTION_MODEL, transcription),
    model(AUDIOCPP_DIARIZATION_MODEL, diarization),
  ];
}

test('completion does not remember a missing transcription model as downloaded', async () => {
  native.transcriptionInstalled = false;
  native.models = installedModels(false, true);

  const context = await renderProvider();
  await act(async () => {
    await context.completeOnboarding();
  });

  expect(native.completedModel).toBe('qwen3.5:4b');
  expect(captured!.transcriptionModelDownloaded).toBe(false);
  expect(captured!.diarizationModelDownloaded).toBe(true);
});

test('completion records the transcription model only when it is installed', async () => {
  native.transcriptionInstalled = true;
  native.models = installedModels(true, false);

  const context = await renderProvider();
  await act(async () => {
    await context.completeOnboarding();
  });

  expect(captured!.transcriptionModelDownloaded).toBe(true);
  expect(captured!.diarizationModelDownloaded).toBe(false);
});

test('starting the onboarding downloads seeds the card sizes from the catalog', async () => {
  native.models = installedModels(false, false);

  const context = await renderProvider();
  expect(context.transcriptionModelProgressInfo.totalMb).toBe(0);

  await act(async () => {
    await context.startBackgroundDownloads({
      includeAudioCppTranscription: true,
      includeAudioCppDiarization: true,
      includeSummary: false,
    });
  });

  expect(captured!.transcriptionModelProgressInfo.totalMb).toBe(422);
  expect(captured!.diarizationModelProgressInfo.totalMb).toBe(422);
});
