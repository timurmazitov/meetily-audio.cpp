import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The real sidebar is rendered: only the native edge and the heavy chrome
// around it are replaced, so the meeting row is what this suite observes.
const originals = await Promise.all(
  [
    '@tauri-apps/api/core',
    '@tauri-apps/api/event',
    'next/navigation',
    '../../src/contexts/RecordingStateContext',
    '../../src/contexts/ImportDialogContext',
    '../../src/components/ui/dialog',
    '../../src/components/ui/tooltip',
    '../../src/components/Logo',
    'sonner',
  ].map(async path => [path, { ...(await import(path)) }] as const),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

const Container = ({ children }: { children?: React.ReactNode }) => <div>{children}</div>;

let listRows: unknown[] = [];
const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_get_meetings') return listRows;
  if (command === 'doodle_sync') return null;
  if (command === 'api_get_model_config') return null;
  if (command === 'api_get_transcript_config') return null;
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke, convertFileSrc: (path: string) => path }));
// The sidebar subscribes to model-config updates on mount; the subscription
// itself is irrelevant here, only the meeting rows matter.
mock.module('@tauri-apps/api/event', () => ({
  listen: async () => () => {},
  emit: async () => {},
}));
mock.module('next/navigation', () => ({
  usePathname: () => '/',
  useRouter: () => ({ push: () => {}, replace: () => {} }),
}));
mock.module('../../src/contexts/RecordingStateContext', () => ({
  useRecordingState: () => ({ isRecording: false }),
}));
mock.module('../../src/contexts/ImportDialogContext', () => ({
  useImportDialog: () => ({ openImportDialog: () => {} }),
}));
mock.module('../../src/components/ui/dialog', () => ({
  Dialog: Container,
  DialogContent: Container,
  DialogFooter: Container,
  DialogTitle: Container,
  DialogTrigger: Container,
}));
mock.module('../../src/components/ui/tooltip', () => ({
  Tooltip: Container,
  TooltipContent: Container,
  TooltipProvider: Container,
  TooltipTrigger: Container,
}));
mock.module('../../src/components/Logo', () => ({ default: () => null }));
mock.module('sonner', () => ({ toast: { success: () => {}, error: () => {}, warning: () => {}, info: () => {} } }));

const { SidebarProvider } = await import('../../src/components/Sidebar/SidebarProvider');
const { default: Sidebar } = await import('../../src/components/Sidebar');

let renderer: ReactTestRenderer | undefined;
const originalDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
const originalWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  invoke.mockClear();
  if (originalDocument) Object.defineProperty(globalThis, 'document', originalDocument);
  else Reflect.deleteProperty(globalThis, 'document');
  if (originalWindow) Object.defineProperty(globalThis, 'window', originalWindow);
  else Reflect.deleteProperty(globalThis, 'window');
});

/** Local moment, so the expected text does not depend on the machine's zone. */
const recordedAt = new Date(2026, 10, 18, 14, 32).toISOString();

async function show() {
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: Object.assign(new EventTarget(), { visibilityState: 'visible' }),
  });
  Object.defineProperty(globalThis, 'window', { configurable: true, value: globalThis });
  await act(async () => {
    renderer = create(<SidebarProvider><Sidebar /></SidebarProvider>);
  });
  // The provider expands the Meeting Notes folder and fetches its rows.
  await act(async () => {});
  await act(async () => {});
  // The sidebar starts collapsed, where the spec keeps the rows unrendered.
  // Expand it the way a user does, through the floating toggle.
  const toggle = renderer!.root.findAll(
    (node) => node.type === 'button' && typeof node.props.onClick === 'function'
      && !node.props['aria-label'] && !node.props.disabled,
  )[0];
  await act(async () => toggle.props.onClick());
  return renderer!;
}

function textOf(node: any): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(textOf).join('');
  if (node?.children) return node.children.map(textOf).join('');
  return '';
}

function viewTextOf(instance: ReactTestRenderer): string {
  return textOf(instance.toJSON());
}

test('a listed recording shows its date and length under the title', async () => {
  listRows = [{
    id: 'meeting-1',
    title: 'Planning call',
    recorded_at: recordedAt,
    duration_seconds: 4_975.5,
  }];

  const view = await show();

  const text = viewTextOf(view);
  expect(text).toContain('Planning call');
  expect(text).toContain('18/11/2026, 14:32 · 1 h 23 min');
});

test('a meeting with neither value shows no second line', async () => {
  listRows = [{ id: 'meeting-2', title: 'Old call' }];

  const view = await show();

  expect(viewTextOf(view)).toContain('Old call');
  expect(viewTextOf(view)).not.toContain('·');
});
