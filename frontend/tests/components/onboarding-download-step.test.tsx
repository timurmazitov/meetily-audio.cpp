import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { AudioCppModelDownloadStatus } from '../../src/lib/onboarding-audiocpp-status';

type ProgressInfo = {
  percent: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
};

const originals = await Promise.all(
  [
    '@tauri-apps/api/core',
    '@tauri-apps/api/event',
    '@tauri-apps/plugin-os',
    '../../src/contexts/OnboardingContext',
  ].map(async path => [path, { ...(await import(path)) }] as const),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

interface StepState {
  selectedSummaryModel: string;
  recommendedSummaryModel: string;
  transcriptionModelDownloaded: boolean;
  transcriptionModelProgressInfo: ProgressInfo;
  transcriptionModelStatus: AudioCppModelDownloadStatus;
  transcriptionModelError: string | null;
  diarizationModelDownloaded: boolean;
  diarizationModelProgressInfo: ProgressInfo;
  diarizationModelStatus: AudioCppModelDownloadStatus;
  diarizationModelError: string | null;
  summaryModelDownloaded: boolean;
  setTranscriptionModelDownloaded: (value: boolean) => void;
  setDiarizationModelDownloaded: (value: boolean) => void;
  setSummaryModelDownloaded: (value: boolean) => void;
  startBackgroundDownloads: (options: Record<string, unknown>) => Promise<void>;
  completeOnboarding: () => Promise<void>;
  goNext: () => void;
}

/** Onboarding state as the context exposes it to the download step. */
const baseState: StepState = {
  selectedSummaryModel: 'qwen3.5:4b',
  recommendedSummaryModel: 'qwen3.5:4b',
  transcriptionModelDownloaded: false,
  transcriptionModelProgressInfo: { percent: 0, downloadedMb: 0, totalMb: 0, speedMbps: 0 },
  transcriptionModelStatus: 'waiting',
  transcriptionModelError: null,
  diarizationModelDownloaded: false,
  diarizationModelProgressInfo: { percent: 0, downloadedMb: 0, totalMb: 0, speedMbps: 0 },
  diarizationModelStatus: 'waiting',
  diarizationModelError: null,
  summaryModelDownloaded: false,
  setTranscriptionModelDownloaded: () => {},
  setDiarizationModelDownloaded: () => {},
  setSummaryModelDownloaded: () => {},
  startBackgroundDownloads: async () => {},
  completeOnboarding: async () => {},
  goNext: () => {},
};

let state: StepState = { ...baseState };

mock.module('../../src/contexts/OnboardingContext', () => ({
  useOnboarding: () => state,
}));
mock.module('@tauri-apps/api/core', () => ({ invoke: async () => undefined }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('@tauri-apps/plugin-os', () => ({ platform: () => 'macos' }));

const { DownloadProgressStep } = await import(
  '../../src/components/onboarding/steps/DownloadProgressStep'
);

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  state = { ...baseState };
});

async function renderStep() {
  await act(async () => {
    renderer = create(<DownloadProgressStep />);
  });
  return JSON.stringify(renderer!.toJSON());
}

function textOf(node: any): string {
  if (typeof node === 'string') return node;
  if (!node || typeof node !== 'object') return '';
  const children = Array.isArray(node.children) ? node.children : [];
  return children.map((child: unknown) => textOf(child)).join('');
}

/** The step's primary action is the last button; cards render above it. */
const continueButton = () => {
  const buttons = renderer!.root.findAllByType('button');
  return buttons[buttons.length - 1];
};

const retryButtons = () =>
  renderer!.root.findAllByType('button').filter(button => textOf(button).includes('Try Again'));

test('the step lists GigaAM, Nemotron and the summary model', async () => {
  const html = await renderStep();
  expect(html).toContain('Transcription Engine');
  expect(html).toContain('Speaker Diarization');
  expect(html).toContain('Summary Engine');
  expect(html.toLowerCase()).not.toContain('parakeet');
  expect(html).not.toContain('~670 MB');
});

test('Continue stays disabled until the transcription model is ready', async () => {
  await renderStep();
  expect(continueButton().props.disabled).toBe(true);

  state = {
    ...baseState,
    transcriptionModelDownloaded: true,
    transcriptionModelStatus: 'completed',
  };
  await act(async () => {
    renderer!.update(<DownloadProgressStep />);
  });
  expect(continueButton().props.disabled).toBe(false);
  expect(textOf(continueButton())).toContain('Continue');
});

test('a failed diarization download does not block Continue', async () => {
  state = {
    ...baseState,
    transcriptionModelDownloaded: true,
    transcriptionModelStatus: 'completed',
    diarizationModelStatus: 'error',
    diarizationModelError: 'network unreachable',
  };
  const html = await renderStep();
  expect(html).toContain('Try Again');
  expect(retryButtons().length).toBe(1);
  expect(continueButton().props.disabled).toBe(false);
});

test('the background notice appears once only the diarization model is pending', async () => {
  const waiting = await renderStep();
  expect(waiting).not.toContain('You can continue while this finishes');

  state = {
    ...baseState,
    transcriptionModelDownloaded: true,
    transcriptionModelStatus: 'completed',
    summaryModelDownloaded: true,
  };
  await act(async () => {
    renderer!.update(<DownloadProgressStep />);
  });
  expect(JSON.stringify(renderer!.toJSON())).toContain('You can continue while this finishes');
});

test('card sizes come from the reported payload', async () => {
  state = {
    ...baseState,
    transcriptionModelStatus: 'downloading',
    transcriptionModelProgressInfo: {
      percent: 40,
      downloadedMb: 168.8,
      totalMb: 422,
      speedMbps: 12.5,
    },
    diarizationModelStatus: 'downloading',
    diarizationModelProgressInfo: {
      percent: 10,
      downloadedMb: 19,
      totalMb: 190,
      speedMbps: 2.5,
    },
  };
  const html = await renderStep();
  expect(html).toContain('~422 MB');
  expect(html).toContain('~190 MB');
  expect(html).toContain('168.8');
  expect(html).toContain('12.5');
});
