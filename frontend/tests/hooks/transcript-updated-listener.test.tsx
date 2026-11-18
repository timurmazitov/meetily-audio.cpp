import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});

const notify = mock(() => {});
mock.module('sonner', () => ({
  toast: { info: notify, error: notify, success: notify, warning: notify },
}));

const { useTranscriptUpdatedListener } = await import(
  '../../src/hooks/useTranscriptUpdatedListener'
);

type Handler = (event: { payload: Record<string, unknown> }) => void;

/** Fake native event bridge, injected through the hook's seam. */
function fakeListen() {
  const registered = new Map<string, Handler>();
  const unlistened = mock((name: string) => {});
  const listen = async (name: string, handler: Handler) => {
    registered.set(name, handler);
    return () => {
      registered.delete(name);
      unlistened(name);
    };
  };
  return { registered, unlistened, listen };
}

let renderer: ReactTestRenderer | undefined;
let refetches = 0;
let bridge = fakeListen();

function Harness({ meetingId }: { meetingId: string }) {
  useTranscriptUpdatedListener(meetingId, () => { refetches += 1; }, bridge.listen);
  return <span>{meetingId}</span>;
}

async function show(meetingId = 'meeting-a') {
  await act(async () => { renderer = create(<Harness meetingId={meetingId} />); });
}

async function emit(payload: Record<string, unknown>) {
  const handler = bridge.registered.get('transcript-updated');
  expect(handler).toBeDefined();
  await act(async () => { handler!({ payload }); });
}

afterEach(async () => {
  if (renderer) await act(async () => { renderer!.unmount(); });
  renderer = undefined;
  bridge = fakeListen();
  notify.mockClear();
  refetches = 0;
});

test('the open meeting is refetched when its labels arrive', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-a', labeled_segments: 4, total_segments: 5 });

  expect(refetches).toBe(1);
});

test('a refresh is silent: the automatic pass owes the user no toast', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-a', labeled_segments: 4, total_segments: 5 });

  expect(notify).not.toHaveBeenCalled();
});

test('an event for another meeting is ignored', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-b', labeled_segments: 1, total_segments: 1 });

  expect(refetches).toBe(0);
});

test('an event without a meeting id cannot be attributed and is ignored', async () => {
  await show('meeting-a');

  await emit({ labeled_segments: 1, total_segments: 1 });

  expect(refetches).toBe(0);
});

test('a pass that labeled nothing still refreshes the transcript', async () => {
  await show('meeting-a');

  await emit({ meeting_id: 'meeting-a', labeled_segments: 0, total_segments: 3 });

  expect(refetches).toBe(1);
});

test('a missing event bridge does not break the page', async () => {
  const failure = async () => { throw new Error('no bridge'); };
  await act(async () => {
    renderer = create(
      <HarnessWithBridge meetingId="meeting-a" listenImpl={failure as any} />,
    );
  });

  expect(refetches).toBe(0);
});

function HarnessWithBridge({ meetingId, listenImpl }: { meetingId: string; listenImpl: any }) {
  useTranscriptUpdatedListener(meetingId, () => { refetches += 1; }, listenImpl);
  return <span>{meetingId}</span>;
}

test('the listener is removed on unmount', async () => {
  await show('meeting-a');
  expect(bridge.registered.has('transcript-updated')).toBe(true);

  await act(async () => { renderer!.unmount(); });
  renderer = undefined;

  expect(bridge.registered.has('transcript-updated')).toBe(false);
  expect(bridge.unlistened).toHaveBeenCalledWith('transcript-updated');
});
