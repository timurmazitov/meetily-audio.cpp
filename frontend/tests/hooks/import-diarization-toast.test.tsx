/**
 * The import listener for `diarization-unavailable`.
 *
 * An imported meeting is saved before the diarization pass runs, and native
 * code emits this event only when the pass failed. The hook still checks the
 * payload status: a configuration reason (`no_model`, `no_runtime`,
 * `deferred`) must stay silent, because the user never asked for the pass.
 *
 * Tauri modules are mocked at module scope with an `afterAll` restore that
 * spreads the real module, the same pattern the other hook tests use: mocking
 * a Tauri module leaks into other files in the shared bun process.
 */

import React from 'react';
import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { DIARIZATION_FAILED_MESSAGE } from '../../src/lib/diarization';

const originals = await Promise.all(
  ['@tauri-apps/api/core', '@tauri-apps/api/event', 'sonner'].map(
    async path => [path, { ...(await import(path)) }] as const,
  ),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

type Handler = (event: { payload: Record<string, unknown> }) => void;

/** Native event handlers registered by the hook, keyed by event name. */
const registered = new Map<string, Handler>();

mock.module('@tauri-apps/api/core', () => ({
  invoke: async () => undefined,
}));

mock.module('@tauri-apps/api/event', () => ({
  listen: async (name: string, handler: Handler) => {
    registered.set(name, handler);
    return () => {
      registered.delete(name);
    };
  },
}));

const toastError = mock(() => {});
const toastInfo = mock(() => {});
const toastSuccess = mock(() => {});
const toastWarning = mock(() => {});

mock.module('sonner', () => ({
  toast: {
    info: toastInfo,
    error: toastError,
    success: toastSuccess,
    warning: toastWarning,
  },
}));

const { useImportAudio } = await import('../../src/hooks/useImportAudio');

let renderer: ReactTestRenderer | undefined;

function Harness() {
  useImportAudio();
  return <span>import</span>;
}

async function show() {
  await act(async () => {
    renderer = create(<Harness />);
  });
}

async function emitDiarizationUnavailable(payload: Record<string, unknown>) {
  // Listeners are registered asynchronously; the progress listener is
  // registered first, so waiting for the diarization one needs a tick.
  for (let attempt = 0; attempt < 50 && !registered.has('diarization-unavailable'); attempt += 1) {
    await act(async () => {
      await Promise.resolve();
    });
  }
  const handler = registered.get('diarization-unavailable');
  expect(handler).toBeDefined();
  await act(async () => {
    handler!({ payload });
  });
}

beforeEach(() => {
  registered.clear();
  toastError.mockClear();
  toastInfo.mockClear();
  toastSuccess.mockClear();
  toastWarning.mockClear();
});

afterEach(async () => {
  if (renderer) {
    await act(async () => {
      renderer!.unmount();
    });
  }
  renderer = undefined;
});

test('a failed pass is reported because the user should know labels are missing', async () => {
  await show();

  await emitDiarizationUnavailable({
    meeting_id: 'meeting-a',
    status: 'failed',
    reason: 'engine exploded',
    message: DIARIZATION_FAILED_MESSAGE,
  });

  expect(toastError).toHaveBeenCalledTimes(1);
  const [title, options] = toastError.mock.calls[0] as unknown as [
    string,
    { description?: string },
  ];
  expect(title).toBe('Diarization failed');
  expect(options.description).toBe(DIARIZATION_FAILED_MESSAGE);
});

test('configuration reasons stay silent: the user did not request the pass', async () => {
  await show();

  for (const status of ['no_model', 'no_runtime', 'deferred', 'ran']) {
    await emitDiarizationUnavailable({ meeting_id: 'meeting-a', status });
  }

  expect(toastError).not.toHaveBeenCalled();
  expect(toastInfo).not.toHaveBeenCalled();
});

test('an event without a status cannot be attributed and stays silent', async () => {
  await show();

  await emitDiarizationUnavailable({ meeting_id: 'meeting-a', reason: 'old caller' });

  expect(toastError).not.toHaveBeenCalled();
  expect(toastInfo).not.toHaveBeenCalled();
});
