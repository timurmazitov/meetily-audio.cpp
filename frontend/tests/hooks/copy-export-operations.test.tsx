import { afterAll, afterEach, beforeEach, describe, expect, mock, setSystemTime, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { RefObject } from 'react';
import type { BlockNoteSummaryViewRef } from '../../src/components/AISummary/BlockNoteSummaryView';
import type { MeetingSummary, Transcript } from '../../src/types';

// Bun shares module mocks between test files; restore application modules after this suite.
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
// `window` is absent in the default bun test environment; remember that so later
// suites that rely on its absence (react-dom/server) are not affected by our shim.
const globalWindow = globalThis as any;
const hadWindow = 'window' in globalWindow;
const originalWindow = globalWindow.window;
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
  if (hadWindow) globalWindow.window = originalWindow;
  else Reflect.deleteProperty(globalWindow, 'window');
});

const successToast = mock(() => {});
const errorToast = mock(() => {});

const transcript: Transcript = { id: 'transcript-1', text: 'We shipped it', timestamp: '00:00' };
let transcriptsPage: { transcripts: Transcript[]; total_count: number; has_more: boolean };
let savedPath: string | null;
let saveError: Error | null;
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meeting_transcripts') return transcriptsPage;
  if (command === 'export_transcript_file_command') {
    if (saveError) throw saveError;
    return savedPath;
  }
  throw new Error(`Unexpected command: ${command} (${JSON.stringify(args)})`);
});

/**
 * Re-apply the test doubles for one test.
 *
 * Bun shares module mocks between test files, and another suite's `afterAll`
 * restore can run after this file was loaded, so a mock registered only at
 * module load may be gone by the first test. The `window` shim additionally
 * covers the case where `export-file.ts` is already bound to the real `invoke`:
 * the real `invoke` delegates to `window.__TAURI_INTERNALS__.invoke`.
 */
function installTestDoubles() {
  mock.module('sonner', () => ({
    toast: { success: successToast, error: errorToast, info: mock(() => {}), warning: mock(() => {}) },
  }));
  mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
  (globalThis as any).window ??= {};
  (globalThis as any).window.__TAURI_INTERNALS__ = { invoke };
}
installTestDoubles();

const { useCopyOperations } = await import('../../src/hooks/meeting-details/useCopyOperations');

const meeting = { id: 'meeting-1', title: 'Release planning', created_at: '2026-11-18' };

const clipboardWrites: string[] = [];
const writeText = mock(async (text: string) => {
  clipboardWrites.push(text);
});
const originalClipboard = Object.getOwnPropertyDescriptor(globalThis.navigator, 'clipboard');

let operations: ReturnType<typeof useCopyOperations>;
function Harness(props: Parameters<typeof useCopyOperations>[0]) {
  operations = useCopyOperations(props);
  return null;
}

let renderer: ReactTestRenderer | undefined;
let editorMarkdown: string | null;
let aiSummary: MeetingSummary | null;

async function show() {
  const ref = {
    current: {
      getMarkdown: async () => editorMarkdown ?? '',
      saveSummary: async () => {},
      isDirty: false,
    },
  } as RefObject<BlockNoteSummaryViewRef>;

  await act(async () => {
    renderer = create(
      <Harness
        meeting={meeting}
        transcripts={[]}
        meetingTitle={meeting.title}
        aiSummary={aiSummary}
        blockNoteSummaryRef={ref}
      />,
    );
  });
}

beforeEach(() => {
  installTestDoubles();
  Object.defineProperty(globalThis.navigator, 'clipboard', {
    configurable: true,
    value: { writeText },
  });
  setSystemTime(new Date(2026, 10, 18, 10, 30));
  transcriptsPage = { transcripts: [transcript], total_count: 1, has_more: false };
  savedPath = '/home/user/exports/20261118-Release_planning-summary.md';
  saveError = null;
  editorMarkdown = '## Decisions\n\n- Ship on Friday';
  aiSummary = { markdown: '## Saved summary' } as MeetingSummary;
  clipboardWrites.length = 0;
  invoke.mockClear();
  writeText.mockClear();
  successToast.mockClear();
  errorToast.mockClear();
});

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  setSystemTime();
  if (originalClipboard) Object.defineProperty(globalThis.navigator, 'clipboard', originalClipboard);
  else Reflect.deleteProperty(globalThis.navigator, 'clipboard');
});

function exportCalls() {
  return invoke.mock.calls.filter(([command]) => command === 'export_transcript_file_command');
}

describe('summary export through the native dialog', () => {
  test('asks for a markdown file and writes the document with the Exported label', async () => {
    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(exportCalls()).toHaveLength(1);
    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(args.extension).toBe('md');
    expect(args.filterLabel).toBe('Markdown Files');
    expect(args.defaultFileName).toBe('20261118-Release_planning-summary.md');

    const content = args.content as string;
    expect(content).toStartWith('# Meeting Summary: Release planning\n\n');
    expect(content).toContain('**Meeting ID:** meeting-1');
    expect(content).toContain('**Date:** November 18, 2026');
    expect(content).toContain('**Exported on:**');
    expect(content).not.toContain('**Copied on:**');
    expect(content).toEndWith('---\n\n## Decisions\n\n- Ship on Friday');
  });

  test('the file text equals the copied text except for the label word', async () => {
    await show();
    await act(async () => {
      await operations.handleCopySummary();
    });
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(clipboardWrites).toHaveLength(2); // the copied document, then the saved path
    const copied = clipboardWrites[0];
    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(copied).toContain('**Copied on:**');
    expect(args.content).toBe(copied.replace('**Copied on:**', '**Exported on:**'));
  });

  test('exports the current editor state, including unsaved edits', async () => {
    editorMarkdown = '## Unsaveable edits';
    aiSummary = { markdown: '## Stale saved text' } as MeetingSummary;

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(args.content).toContain('## Unsaveable edits');
    expect(args.content).not.toContain('## Stale saved text');
    // Export must not auto-save the summary: only the transcript read and the dialog ran.
    expect(invoke.mock.calls.map(([command]) => command))
      .toEqual(['export_transcript_file_command']);
  });

  test('falls back to the saved markdown when the editor is unavailable', async () => {
    editorMarkdown = null; // getMarkdown() resolves to an empty string
    aiSummary = { markdown: '## Saved summary' } as MeetingSummary;

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(args.content).toContain('## Saved summary');
  });

  test('exports a legacy summary through the same resolution as Copy', async () => {
    editorMarkdown = null;
    aiSummary = {
      KeyPoints: { title: 'Key Points', blocks: [{ content: 'Ship it' }, { content: 'Tell users' }] },
    } as unknown as MeetingSummary;

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(args.content).toEndWith('---\n\n## Key Points\n\n- Ship it\n- Tell users');
  });

  test('puts the saved path on the clipboard and reports success with it', async () => {
    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(clipboardWrites).toEqual(['/home/user/exports/20261118-Release_planning-summary.md']);
    expect(successToast).toHaveBeenCalledWith('Summary exported', {
      description: '/home/user/exports/20261118-Release_planning-summary.md',
    });
    expect(errorToast).not.toHaveBeenCalled();
  });

  test('cancelling the dialog leaves the clipboard and the toasts alone', async () => {
    savedPath = null;

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(clipboardWrites).toEqual([]);
    expect(successToast).not.toHaveBeenCalled();
    expect(errorToast).not.toHaveBeenCalled();
  });

  test('a failing write reports an error and leaves the clipboard alone', async () => {
    saveError = new Error('permission denied');

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(clipboardWrites).toEqual([]);
    expect(successToast).not.toHaveBeenCalled();
    expect(errorToast).toHaveBeenCalledWith('Failed to export summary');
  });

  test('refuses to export when the summary has no visible content', async () => {
    editorMarkdown = null;
    aiSummary = { markdown: '   ' } as MeetingSummary;

    await show();
    await act(async () => {
      await operations.handleExportSummary();
    });

    expect(exportCalls()).toHaveLength(0);
    expect(errorToast).toHaveBeenCalledWith('No summary content available to export');
    expect(clipboardWrites).toEqual([]);
  });
});

describe('transcript export keeps its text file contract', () => {
  test('sends txt and Text Files to the shared command', async () => {
    savedPath = '/home/user/exports/20261118-Release_planning-transcript.txt';

    await show();
    await act(async () => {
      await operations.handleExportTranscript();
    });

    expect(exportCalls()).toHaveLength(1);
    const [, args] = exportCalls()[0] as [string, Record<string, unknown>];
    expect(args.extension).toBe('txt');
    expect(args.filterLabel).toBe('Text Files');
    expect(args.defaultFileName).toBe('20261118-Release_planning-transcript.txt');
    expect(clipboardWrites).toEqual(['/home/user/exports/20261118-Release_planning-transcript.txt']);
    expect(successToast).toHaveBeenCalledWith('Transcript exported', {
      description: '/home/user/exports/20261118-Release_planning-transcript.txt',
    });
  });
});
