import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalEvents = { ...await import('@tauri-apps/api/event') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/event', () => originalEvents);
  mock.module('sonner', () => originalToast);
});

const handlers = new Map<string, (event: { payload: any }) => void>();
const unlistened = mock((name: string) => {});
mock.module('@tauri-apps/api/event', () => ({
  listen: async (name: string, handler: any) => {
    handlers.set(name, handler);
    return () => { handlers.delete(name); unlistened(name); };
  },
}));

const notify = mock(() => {});
mock.module('sonner', () => ({
  toast: { info: notify, error: notify, success: notify, warning: notify },
}));

const { useSpeakerNamesResetListener } = await import('../../src/hooks/useSpeakerNamesResetListener');

let renderer: ReactTestRenderer | undefined;
let refetches = 0;

function Harness({ meetingId }: { meetingId: string }) {
  useSpeakerNamesResetListener(meetingId, () => { refetches += 1; });
  return <span>{meetingId}</span>;
}

async function show(meetingId = 'meeting-a') {
  await act(async () => { renderer = create(<Harness meetingId={meetingId} />); });
}

async function emit(payload: Record<string, unknown>) {
  const handler = handlers.get('speaker-names-reset');
  expect(handler).toBeDefined();
  await act(async () => { handler!({ payload }); });
}

afterEach(async () => {
  if (renderer) await act(async () => { renderer!.unmount(); });
  renderer = undefined;
  handlers.clear();
  notify.mockClear();
  unlistened.mockClear();
  refetches = 0;
});

test('the reset hint is shown and the transcript refetched for the open meeting', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-a', message: 'Names went away.' });

  expect(notify).toHaveBeenCalledTimes(1);
  const [title, options] = notify.mock.calls[0] as unknown as [string, { description?: string }];
  expect(title).toBe('Speaker names were reset');
  expect(options.description).toBe('Names went away.');
  expect(refetches).toBe(1);
});

test('the hint falls back to the shared text when the payload has no message', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-a' });

  const [title, options] = notify.mock.calls[0] as unknown as [string, { description?: string }];
  expect(title).toBe('Speaker names were reset');
  expect(options.description).toBe(
    'Speaker names were reset because the voices were recomputed.',
  );
});

test('the hint for another meeting is ignored', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-b', message: 'Not yours.' });

  expect(notify).not.toHaveBeenCalled();
  expect(refetches).toBe(0);
});

test('the listener is removed on unmount', async () => {
  await show('meeting-a');
  expect(handlers.has('speaker-names-reset')).toBe(true);

  await act(async () => { renderer!.unmount(); });
  renderer = undefined;

  expect(handlers.has('speaker-names-reset')).toBe(false);
  expect(unlistened).toHaveBeenCalledWith('speaker-names-reset');
});
