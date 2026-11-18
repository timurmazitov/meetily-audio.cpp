import React from 'react';
import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// Both panels sit inside Radix portals (tooltips, popovers, dialogs, dropdowns)
// and framer-motion wrappers that need a real DOM. These tests only assert the
// header text, so the wrappers become plain containers. Restored in `afterAll`
// where bun's shared module mocks allow it.
const Container = ({ children }: any) => <div>{children}</div>;

const originals = {
  tooltip: { ...(await import('../../src/components/ui/tooltip')) },
  popover: { ...(await import('../../src/components/ui/popover')) },
  dialog: { ...(await import('../../src/components/ui/dialog')) },
  dropdownMenu: { ...(await import('../../src/components/ui/dropdown-menu')) },
  config: { ...(await import('../../src/contexts/ConfigContext')) },
};

mock.module('../../src/components/ui/tooltip', () => ({
  Tooltip: Container,
  TooltipTrigger: Container,
  TooltipContent: Container,
  TooltipProvider: Container,
}));
mock.module('../../src/components/ui/popover', () => ({
  Popover: Container,
  PopoverTrigger: Container,
  PopoverContent: Container,
  PopoverAnchor: Container,
}));
mock.module('../../src/components/ui/dialog', () => ({
  Dialog: Container,
  DialogContent: Container,
  DialogTrigger: Container,
  DialogTitle: Container,
  DialogDescription: Container,
  DialogHeader: Container,
  DialogFooter: Container,
  DialogClose: Container,
  DialogPortal: Container,
}));
mock.module('../../src/components/ui/dropdown-menu', () => ({
  DropdownMenu: Container,
  DropdownMenuTrigger: Container,
  DropdownMenuContent: Container,
  DropdownMenuItem: Container,
  DropdownMenuLabel: Container,
  DropdownMenuSeparator: () => <hr />,
  DropdownMenuGroup: Container,
  DropdownMenuCheckboxItem: Container,
  DropdownMenuRadioItem: Container,
  DropdownMenuRadioGroup: Container,
}));

afterAll(() => {
  mock.module('../../src/components/ui/tooltip', () => originals.tooltip);
  mock.module('../../src/components/ui/popover', () => originals.popover);
  mock.module('../../src/components/ui/dialog', () => originals.dialog);
  mock.module('../../src/components/ui/dropdown-menu', () => originals.dropdownMenu);
  mock.module('../../src/contexts/ConfigContext', () => originals.config);
});

// The retranscribe dialog inside the transcript button group reads the config
// context on mount; only the config edge is stubbed, the rest stays real.
mock.module('../../src/contexts/ConfigContext', () => ({
  ...originals.config,
  useConfig: () => ({
    selectedLanguage: 'auto',
    transcriptModelConfig: { provider: 'whisper', model: 'large-v3' },
  }),
}));

// framer-motion cannot detect SVG elements without a DOM. Like the existing
// component suites, this mock stays installed for the process: restoring it in
// `afterAll` would break the other suites that loaded their components already.
const motionContainer = (props: any) => <div>{props.children}</div>;
mock.module('framer-motion', () => ({
  motion: new Proxy({}, { get: () => motionContainer }),
  AnimatePresence: Container,
}));

// The panels talk to Tauri only for unrelated side channels (summary language,
// speakers). The fake backend keeps those quiet and never touches the date.
const originalCore = { ...(await import('@tauri-apps/api/core')) };
const invoke = mock(async (command: string): Promise<unknown> => {
  if (command === 'api_get_meeting_summary_language') return null;
  if (command === 'api_get_meeting_speakers') return [];
  throw new Error(`Unexpected command: ${command}`);
});
const coreMock = () => ({ ...originalCore, invoke });
mock.module('@tauri-apps/api/core', coreMock);

const notify = mock(() => {});
mock.module('sonner', () => ({
  toast: { info: notify, error: notify, success: notify, warning: notify },
}));

const stored = new Map<string, string>();

beforeEach(() => {
  mock.module('@tauri-apps/api/core', coreMock);
  // Radix (tabs) schedules a mount animation on the bare global.
  Object.defineProperty(globalThis, 'requestAnimationFrame', {
    configurable: true,
    value: (callback: () => void) => { callback(); return 0; },
  });
  Object.defineProperty(globalThis, 'cancelAnimationFrame', {
    configurable: true,
    value: () => {},
  });
  // The summary panel reads its saved language preference through `window`;
  // the language picker registers a `storage` listener.
  (globalThis as any).window = {
    __TAURI_INTERNALS: { invoke },
    addEventListener: () => {},
    removeEventListener: () => {},
    setTimeout: () => 0,
    clearTimeout: () => {},
    // The split view asks the media query whether both panes are visible.
    matchMedia: () => ({
      matches: true,
      addEventListener: () => {},
      removeEventListener: () => {},
    }),
    localStorage: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => { stored.set(key, value); },
      removeItem: (key: string) => { stored.delete(key); },
    },
  };
  invoke.mockClear();
});

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  stored.clear();
  Reflect.deleteProperty(globalThis, 'window');
  Reflect.deleteProperty(globalThis, 'requestAnimationFrame');
  Reflect.deleteProperty(globalThis, 'cancelAnimationFrame');
});

const { SummaryPanel } = await import('../../src/components/MeetingDetails/SummaryPanel');
const { TranscriptPanel } = await import('../../src/components/MeetingDetails/TranscriptPanel');

/** Local moment, so the rendered text does not depend on the test machine's zone. */
const meetingDate = new Date(2026, 10, 18, 14, 32).toISOString();
const expectedDate = '18/11/2026, 14:32';

type SummaryProps = React.ComponentProps<typeof SummaryPanel>;
type TranscriptProps = React.ComponentProps<typeof TranscriptPanel>;

function summaryProps(overrides: Partial<SummaryProps> = {}): SummaryProps {
  return {
    meeting: { id: 'meeting-a', title: 'Planning call', created_at: '2026-11-18T21:00:00+07:00' },
    meetingTitle: 'Planning call',
    isSummaryDirty: false,
    summaryRef: { current: null } as any,
    isSaving: false,
    onSaveAll: async () => {},
    onCopySummary: async () => {},
    onExportSummary: async () => {},
    aiSummary: null,
    summaryStatus: 'idle',
    transcripts: [],
    modelConfig: { provider: 'ollama' as any, model: 'llama', whisperModel: 'large-v3' },
    setModelConfig: () => {},
    onSaveModelConfig: async () => {},
    onGenerateSummary: async () => {},
    onStopGeneration: () => {},
    customPrompt: '',
    onSaveSummary: async () => {},
    onSummaryChange: () => {},
    onDirtyChange: () => {},
    summaryError: null,
    onRegenerateSummary: async () => {},
    getSummaryStatusMessage: () => '',
    availableTemplates: [],
    selectedTemplate: '',
    onTemplateSelect: () => {},
    ...overrides,
  };
}

function transcriptProps(overrides: Partial<TranscriptProps> = {}): TranscriptProps {
  return {
    transcripts: [],
    customPrompt: '',
    onPromptChange: () => {},
    onCopyTranscript: () => {},
    onExportTranscript: () => {},
    onOpenMeetingFolder: async () => {},
    isRecording: false,
    meetingId: 'meeting-a',
    meetingFolderPath: '/tmp/meeting-a',
    ...overrides,
  };
}

async function render(element: React.ReactElement) {
  await act(async () => {
    renderer = create(element);
  });
  return renderer!;
}

function textOf(node: any): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(textOf).join('');
  if (node?.children) return node.children.map(textOf).join('');
  return '';
}

function viewText(instance: ReactTestRenderer): string {
  return textOf(instance.toJSON());
}

/** The header's action rows: the date line is a sibling of the centered buttons. */
function headerRows(instance: ReactTestRenderer, containerClass: string) {
  const root = instance.root.find(
    (node) => typeof node.type === 'string' && node.props.className === containerClass,
  );
  const header = root
    .findAll((node) => typeof node.type === 'string' && node.props.className === 'p-4 border-b border-gray-200')
    .at(0);
  expect(header).toBeDefined();
  return (header!.children as any[]).filter((child) => typeof child !== 'string');
}

/** The right-aligned date rows currently rendered in a panel header. */
function dateRows(instance: ReactTestRenderer, containerClass: string) {
  return headerRows(instance, containerClass).filter((row) =>
    String(row?.props?.className ?? '').includes('justify-end'),
  );
}

const SUMMARY_ROOT = 'flex-1 min-w-0 flex flex-col bg-white overflow-hidden h-full w-full @container';
const TRANSCRIPT_ROOT = 'flex h-full min-w-0 w-full bg-white flex-col relative @container';

// --- Slice 1: the summary panel shows the meeting date ---------------------

test('the summary panel header shows the meeting date and time on the right', async () => {
  const view = await render(<SummaryPanel {...summaryProps({ meetingDate })} />);

  expect(viewText(view)).toContain(expectedDate);

  const rows = headerRows(view, SUMMARY_ROOT);
  const dateRow = rows.find((row) => textOf(row).includes(expectedDate));
  expect(String(dateRow.props.className)).toContain('flex items-center justify-end');
  // The date is its own row, so it can never pull the centered buttons.
  expect(String(dateRow.props.className)).not.toContain('justify-center');
});

test('the header date comes from the meeting moment, not the row creation time', async () => {
  // The row was created at 21:00 local, the meeting itself happened at 14:32.
  // Showing `created_at` would date a one-hour call by its end (ADR-0005).
  const view = await render(<SummaryPanel {...summaryProps({ meetingDate })} />);

  expect(viewText(view)).not.toContain('18/11/2026, 21:00');
});

// --- Slice 2: the transcript panel shows the same date ---------------------

test('the transcript panel header shows the same meeting date and time', async () => {
  const view = await render(<TranscriptPanel {...transcriptProps({ meetingDate })} />);

  expect(viewText(view)).toContain(expectedDate);

  const rows = headerRows(view, TRANSCRIPT_ROOT);
  const dateRow = rows.find((row) => textOf(row).includes(expectedDate));
  expect(String(dateRow.props.className)).toContain('flex items-center justify-end');
  expect(String(dateRow.props.className)).not.toContain('justify-center');
});

// --- Slice 3: an unknown date draws no row at all -------------------------

test('a summary panel meeting without a date keeps its old single-row header', async () => {
  const unknown = await render(<SummaryPanel {...summaryProps({ meetingDate: null })} />);
  expect(dateRows(unknown, SUMMARY_ROOT)).toHaveLength(0);

  if (renderer) await act(async () => renderer!.unmount());
  const unusable = await render(<SummaryPanel {...summaryProps({ meetingDate: 'not-a-date' })} />);
  // Neither a placeholder nor the current moment: the formatter returns null.
  expect(dateRows(unusable, SUMMARY_ROOT)).toHaveLength(0);
  expect(viewText(unusable)).not.toContain(expectedDate);
});

test('a transcript panel meeting without a date keeps its old single-row header', async () => {
  const unknown = await render(<TranscriptPanel {...transcriptProps({ meetingDate: null })} />);
  expect(dateRows(unknown, TRANSCRIPT_ROOT)).toHaveLength(0);

  if (renderer) await act(async () => renderer!.unmount());
  const unusable = await render(<TranscriptPanel {...transcriptProps({ meetingDate: 'not-a-date' })} />);
  expect(dateRows(unusable, TRANSCRIPT_ROOT)).toHaveLength(0);
  expect(viewText(unusable)).not.toContain(expectedDate);
});

// --- Slice 4: switching tabs keeps the date -------------------------------

test('both panels carry the date at once, so switching tabs cannot lose it', async () => {
  // The split view mounts both tabs (`forceMount`) and only hides one, so a
  // single tree with both panels is exactly what the user sees on desktop.
  const { MeetingDetailsSplitView } = await import(
    '../../src/components/MeetingDetails/MeetingDetailsSplitView'
  );

  const view = await render(
    <MeetingDetailsSplitView
      activeTab="transcript"
      onTabChange={() => {}}
      transcript={<TranscriptPanel {...transcriptProps({ meetingDate })} />}
      summary={<SummaryPanel {...summaryProps({ meetingDate })} />}
    />,
  );

  const dates = view.root.findAll(
    (node) => typeof node.type === 'string' && node.props.className === 'text-xs text-gray-500 tabular-nums whitespace-nowrap',
  );
  expect(dates).toHaveLength(2);
  expect(dates.map((node) => textOf(node.props.children))).toEqual([expectedDate, expectedDate]);
  expect(dateRows(view, SUMMARY_ROOT)).toHaveLength(1);
  expect(dateRows(view, TRANSCRIPT_ROOT)).toHaveLength(1);
});
