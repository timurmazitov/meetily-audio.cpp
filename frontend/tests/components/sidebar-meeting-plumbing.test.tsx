import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The provider loads Tauri and the recording state on mount; both are replaced
// with a scripted fake so the meeting list itself is what is exercised.
const originals = await Promise.all(
  ['@tauri-apps/api/core', 'next/navigation', '../../src/contexts/RecordingStateContext'].map(
    async path => [path, { ...(await import(path)) }] as const,
  ),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

let listRows: unknown[] = [];
const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_get_meetings') return listRows;
  if (command === 'doodle_sync') return null;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));
mock.module('next/navigation', () => ({ usePathname: () => '/', useRouter: () => ({}) }));
mock.module('../../src/contexts/RecordingStateContext', () => ({
  useRecordingState: () => ({ isRecording: false }),
}));

const { SidebarProvider, useSidebar } = await import('../../src/components/Sidebar/SidebarProvider');

let latest: ReturnType<typeof useSidebar> | undefined;
function Probe() {
  latest = useSidebar();
  return <output>{JSON.stringify(latest.sidebarItems)}</output>;
}

let renderer: ReactTestRenderer | undefined;
const originalDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  latest = undefined;
  invoke.mockClear();
  if (originalDocument) Object.defineProperty(globalThis, 'document', originalDocument);
  else Reflect.deleteProperty(globalThis, 'document');
});

function stubVisibility() {
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: Object.assign(new EventTarget(), { visibilityState: 'visible' }),
  });
}

async function show() {
  stubVisibility();
  await act(async () => {
    renderer = create(<SidebarProvider><Probe /></SidebarProvider>);
  });
  // The provider fetches once its address is known; drain that microtask turn.
  await act(async () => {});
  return renderer!;
}

test('the sidebar row carries the meeting moment and length from the list command', async () => {
  listRows = [
    {
      id: 'meeting-1',
      title: 'Planning call',
      recorded_at: '2026-11-18T14:32:05+00:00',
      duration_seconds: 4_975.5,
    },
  ];

  await show();

  const meeting = latest!.sidebarItems
    .find(item => item.id === 'meetings')!
    .children!.find(item => item.id === 'meeting-1')!;
  expect(meeting.title).toBe('Planning call');
  expect(meeting.recordedAt).toBe('2026-11-18T14:32:05+00:00');
  expect(meeting.durationSeconds).toBe(4_975.5);
});

test('a row the backend left blank stays blank instead of gaining a placeholder', async () => {
  listRows = [{ id: 'meeting-2', title: 'Old call' }];

  await show();

  const meeting = latest!.sidebarItems
    .find(item => item.id === 'meetings')!
    .children!.find(item => item.id === 'meeting-2')!;
  expect(meeting.recordedAt).toBeUndefined();
  expect(meeting.durationSeconds).toBeUndefined();
});
