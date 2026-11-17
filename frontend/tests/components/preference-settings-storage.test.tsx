import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Inter-file mock isolation in a shared `bun test` process is best-effort
// (see docs/RUNNING_LINUX.md). Pattern follows paginated-transcripts.test.tsx:
// register the fakes at load time, import the unit under test immediately after
// (it binds this file's invoke object then), and re-register in beforeEach.
const originals = await Promise.all(['@tauri-apps/api/core', '../../src/contexts/ConfigContext', 'sonner'].map(async path =>
  [path, { ...await import(path) }] as const));
afterAll(() => { for (const [path, exports] of originals) mock.module(path, () => exports); });

const invoke = mock(async (cmd: string, args?: Record<string, unknown>) => {
  if (cmd === 'select_recording_folder') return { path: '/data/recordings', free_bytes: 8 * 1024 ** 3 };
  if (cmd === 'get_recording_preferences') return { save_folder: '/old/recordings' };
  if (cmd === 'set_recording_preferences') return null;
  if (cmd === 'get_database_directory') return '/data';
  if (cmd === 'whisper_get_models_directory') return '/data/models';
  return null;
});

const refreshStorageLocations = mock(async () => {});

// Full toast surface via Proxy: a partial mock would break other suites that
// call toast.info, because module mocks are process-global in bun.
const toastShim = new Proxy({}, { get: () => mock(() => {}) }) as Record<string, unknown>;

const registerMocks = () => {
  mock.module('@tauri-apps/api/core', () => ({ ...originals[0][1], invoke }));
  mock.module('../../src/contexts/ConfigContext', () => ({
    useConfig: () => ({
      notificationSettings: {
        notification_preferences: { show_recording_started: true, show_recording_stopped: true },
      },
      storageLocations: { database: '/data', models: '/data/models', recordings: '/old/recordings' },
      isLoadingPreferences: false,
      loadPreferences: async () => {},
      refreshStorageLocations,
      updateNotificationSettings: async () => {},
    }),
  }));
  mock.module('sonner', () => ({ toast: toastShim }));
};
registerMocks();
const { PreferenceSettings } = await import('../../src/components/PreferenceSettings');
beforeEach(registerMocks);

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  // Remove the global window shim so other suites can install their own.
  Reflect.deleteProperty(globalThis, 'window');
  invoke.mockClear();
  refreshStorageLocations.mockClear();
});

const buttonLabel = (node: any): string => {
  const parts: string[] = [];
  const walk = (n: any) => {
    if (typeof n === 'string') parts.push(n);
    else if (Array.isArray(n)) n.forEach(walk);
    else if (n?.children) walk(n.children);
  };
  walk(node.children);
  return parts.join('');
};

const findButton = (renderer: ReactTestRenderer, label: string) => {
  let found: any = null;
  renderer.root.findAll(node => node.type === 'button').forEach(node => {
    if (buttonLabel(node).includes(label)) found = node;
  });
  return found;
};

// The real `@tauri-apps/api/core` invoke delegates to `window.__TAURI_INTERNALS__.invoke`.
// Other suites in the same `bun test` process may restore the real bridge after our
// mock.module registration (see docs/RUNNING_LINUX.md), so ALSO point the internals
// at this file's fake — this is the pattern from onboarding-download-step.test.tsx.
function installTauriBridge() {
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    writable: true,
    value: { __TAURI_INTERNALS__: { invoke } },
  });
}

test('shows the saved recordings folder and offers Choose and Open actions', async () => {
  installTauriBridge();
  await act(async () => { renderer = create(<PreferenceSettings />); });
  const json = JSON.stringify(renderer!.toJSON());
  expect(json).toContain('/old/recordings');
  expect(findButton(renderer!, 'Choose...')).not.toBeNull();
  expect(findButton(renderer!, 'Open Folder')).not.toBeNull();
});

test('choosing a folder saves it as the new save_folder and refreshes locations', async () => {
  installTauriBridge();
  await act(async () => { renderer = create(<PreferenceSettings />); });

  await act(async () => { findButton(renderer!, 'Choose...')!.props.onClick(); });
  // let the async handler settle
  await act(async () => {});

  const setCall = invoke.mock.calls.find(([cmd]) => cmd === 'set_recording_preferences');
  expect(setCall).toBeDefined();
  expect((setCall![1] as { preferences: { save_folder: string } }).preferences.save_folder).toBe('/data/recordings');
  expect(refreshStorageLocations).toHaveBeenCalled();
});
