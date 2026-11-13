import React from 'react';
import { afterEach, expect, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import type {
  AudioCppCancelDownloadOutcome,
  AudioCppDownloadCompleteEvent,
  AudioCppDownloadErrorEvent,
  AudioCppDownloadProgressEvent,
  AudioCppModelClient,
  AudioCppModelInfo,
} from '../../src/lib/audiocpp';

const baseModel = (overrides: Partial<AudioCppModelInfo> = {}): AudioCppModelInfo => ({
  name: 'gigaam-v3-e2e-ctc-f16',
  display_name: 'GigaAM v3 e2e CTC',
  family: 'gigaam_asr',
  task: 'asr',
  quantization: 'F16',
  filename: 'gigaam-v3-e2e-ctc-f16.gguf',
  path: '/models/audiocpp/gigaam-v3-e2e-ctc-f16.gguf',
  size_mb: 422,
  size_bytes: 442520704,
  description: 'Russian speech recognition',
  status: 'Missing',
  ...overrides,
});

/** Fake native client: records calls and lets the test emit events. */
class FakeClient implements AudioCppModelClient {
  models: AudioCppModelInfo[] = [];
  calls: string[] = [];
  downloadError: Error | null = null;
  cancelOutcome: AudioCppCancelDownloadOutcome = 'cancelled';

  private progressHandler?: (event: AudioCppDownloadProgressEvent) => void;
  private completeHandler?: (event: AudioCppDownloadCompleteEvent) => void;
  private errorHandler?: (event: AudioCppDownloadErrorEvent) => void;

  async init() { this.calls.push('init'); }
  async getAvailableModels() { this.calls.push('getAvailableModels'); return this.models.map(m => ({ ...m })); }
  async hasAvailableModels() { return this.models.some(m => m.status === 'Available'); }
  async getModelsDirectory() { return '/models/audiocpp'; }
  async openModelsFolder() { this.calls.push('openModelsFolder'); }

  async downloadModel(modelName: string) {
    this.calls.push(`download:${modelName}`);
    if (this.downloadError) throw this.downloadError;
  }

  async cancelDownload(modelName: string) {
    this.calls.push(`cancel:${modelName}`);
    return this.cancelOutcome;
  }

  async deleteModel(modelName: string) {
    this.calls.push(`delete:${modelName}`);
    this.models = this.models.map(model =>
      model.name === modelName ? { ...model, status: 'Missing' as const } : model
    );
    return 'deleted';
  }

  async listenProgress(handler: (event: AudioCppDownloadProgressEvent) => void) {
    this.progressHandler = handler;
    return () => { this.progressHandler = undefined; };
  }

  async listenComplete(handler: (event: AudioCppDownloadCompleteEvent) => void) {
    this.completeHandler = handler;
    return () => { this.completeHandler = undefined; };
  }

  async listenError(handler: (event: AudioCppDownloadErrorEvent) => void) {
    this.errorHandler = handler;
    return () => { this.errorHandler = undefined; };
  }

  emitProgress(event: AudioCppDownloadProgressEvent) { this.progressHandler?.(event); }
  emitComplete(event: AudioCppDownloadCompleteEvent) { this.completeHandler?.(event); }
  emitError(event: AudioCppDownloadErrorEvent) { this.errorHandler?.(event); }
}

const { AudioCppModelManager } = await import('../../src/components/AudioCppModelManager');

let renderer: ReactTestRenderer | undefined;

async function render(client: FakeClient) {
  await act(async () => {
    renderer = create(<AudioCppModelManager client={client} />);
  });
  return renderer!;
}

function textOf(node: ReactTestRenderer) {
  function stringify(value: unknown): string {
    if (value === null || value === undefined) return '';
    if (typeof value === 'string' || typeof value === 'number') return String(value);
    if (Array.isArray(value)) return value.map(stringify).join('');
    const element = value as { children?: unknown; props?: { children?: unknown } };
    return stringify(element.children) + stringify(element.props?.children);
  }
  return stringify(node.toJSON());
}

function buttonWithText(root: ReactTestRenderer, text: string) {
  return root.root.findAllByType('button')
    .find(button => JSON.stringify(button.children).includes(text));
}

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

test('lists every catalogued model with its purpose and size', async () => {
  const client = new FakeClient();
  client.models = [
    baseModel(),
    baseModel({
      name: 'nemotron-3-diarization-bf16',
      display_name: 'Nemotron-3 Diarization',
      family: 'nemotron_3_diar',
      task: 'diar',
      quantization: 'BF16',
      filename: 'nemotron-3-diarization-bf16.gguf',
      size_mb: 190,
      size_bytes: 198720928,
      description: 'Speaker diarization',
    }),
  ];

  const text = textOf(await render(client));

  expect(client.calls).toContain('init');
  expect(text).toContain('GigaAM v3 e2e CTC');
  expect(text).toContain('Nemotron-3 Diarization');
  expect(text).toContain('Transcription');
  expect(text).toContain('Diarization');
  expect(text).toContain('422MB');
  expect(text).toContain('190MB');
  expect(text).toContain('F16');
  expect(text).toContain('BF16');
});

test('downloading a model invokes the download command and shows progress', async () => {
  const client = new FakeClient();
  client.models = [baseModel()];
  const view = await render(client);

  const download = buttonWithText(view, 'Download');
  expect(download).toBeDefined();
  await act(async () => { download!.props.onClick(); });

  expect(client.calls).toContain('download:gigaam-v3-e2e-ctc-f16');

  await act(async () => {
    client.emitProgress({
      modelName: 'gigaam-v3-e2e-ctc-f16',
      progress: 42,
      status: 'downloading',
    });
  });
  expect(textOf(view)).toContain('42%');

  await act(async () => {
    client.emitComplete({ modelName: 'gigaam-v3-e2e-ctc-f16' });
  });
  expect(textOf(view)).toContain('Ready');
});

test('a failed download surfaces the error and offers a retry', async () => {
  const client = new FakeClient();
  client.models = [baseModel()];
  client.downloadError = new Error('Download failed');
  const view = await render(client);

  await act(async () => { buttonWithText(view, 'Download')!.props.onClick(); });

  expect(textOf(view)).toContain('Retry');
});

test('cancelling an in-flight download calls the cancel command', async () => {
  const client = new FakeClient();
  client.models = [baseModel()];
  const view = await render(client);

  await act(async () => { buttonWithText(view, 'Download')!.props.onClick(); });
  await act(async () => {
    client.emitProgress({
      modelName: 'gigaam-v3-e2e-ctc-f16',
      progress: 10,
      status: 'downloading',
    });
  });

  const cancel = buttonWithText(view, 'Cancel');
  expect(cancel).toBeDefined();
  await act(async () => { cancel!.props.onClick(); });
  expect(client.calls).toContain('cancel:gigaam-v3-e2e-ctc-f16');

  await act(async () => {
    client.emitProgress({
      modelName: 'gigaam-v3-e2e-ctc-f16',
      progress: 0,
      status: 'cancelled',
    });
  });
  expect(textOf(view)).toContain('Download');
});

test('an installed model can be deleted', async () => {
  const client = new FakeClient();
  client.models = [baseModel({ status: 'Available' })];
  const view = await render(client);

  expect(textOf(view)).toContain('Ready');

  const card = view.root.findByProps({ 'data-testid': 'audiocpp-model-gigaam-v3-e2e-ctc-f16' });
  await act(async () => { card.props.onMouseEnter(); });

  const del = view.root.findAllByType('button')
    .find(button => typeof button.props['aria-label'] === 'string'
      && button.props['aria-label'].startsWith('Delete'));
  expect(del).toBeDefined();
  await act(async () => { del!.props.onClick(); });

  expect(client.calls).toContain('delete:gigaam-v3-e2e-ctc-f16');
  expect(textOf(view)).toContain('Download');
});

test('an installed model survives a restart: state comes from the native catalog', async () => {
  const client = new FakeClient();
  client.models = [baseModel({ status: 'Available' })];

  const first = textOf(await render(client));
  expect(first).toContain('Ready');
  await act(async () => renderer!.unmount());
  renderer = undefined;

  // A fresh mount re-reads the persisted state instead of assuming Missing.
  const restarted = textOf(await render(client));
  expect(restarted).toContain('Ready');
});

test('a corrupted artifact can be deleted or re-downloaded', async () => {
  const client = new FakeClient();
  client.models = [
    baseModel({ status: { Corrupted: { file_size: 3, expected_min_size: 442520704 } } }),
  ];
  const view = await render(client);

  const text = textOf(view);
  expect(text).toContain('Delete');
  expect(text).toContain('Re-download');
});
