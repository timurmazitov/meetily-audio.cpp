import React from 'react';
import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The Speakers panel lives inside a Radix Sheet / Popover / Command stack. None
// of those render without a DOM, so the app wrappers are replaced with plain
// containers. The Sheet keeps its real `open`/`onOpenChange` contract; the last
// handler is captured so a test can trigger Escape / outside-click dismissal.
const Container = ({ children }: any) => <div>{children}</div>;

let sheetOnOpenChange: ((open: boolean) => void) | null = null;
mock.module('../../src/components/ui/sheet', () => ({
  Sheet: ({ open, onOpenChange, children }: any) => {
    sheetOnOpenChange = onOpenChange;
    return open ? <div>{children}</div> : null;
  },
  SheetContent: Container,
  SheetHeader: Container,
  SheetFooter: Container,
  SheetTitle: Container,
  SheetDescription: Container,
  SheetClose: Container,
  SheetTrigger: Container,
  SheetPortal: Container,
  SheetOverlay: Container,
}));
const originalSheet = { ...(await import('../../src/components/ui/sheet')) };
afterAll(() => {
  mock.module('../../src/components/ui/sheet', () => originalSheet);
});

// Popover content stays mounted so the picker list is observable; the component
// under test still owns all the ordering / filtering logic.
mock.module('../../src/components/ui/popover', () => ({
  Popover: Container,
  PopoverTrigger: Container,
  PopoverContent: Container,
  PopoverAnchor: Container,
}));
const originalPopover = { ...(await import('../../src/components/ui/popover')) };
afterAll(() => {
  mock.module('../../src/components/ui/popover', () => originalPopover);
});

mock.module('../../src/components/ui/command', () => ({
  Command: Container,
  CommandInput: ({ value, onValueChange, placeholder }: any) => (
    <input
      value={value}
      placeholder={placeholder}
      onChange={(event) => onValueChange?.(event.target.value)}
    />
  ),
  CommandList: Container,
  CommandEmpty: Container,
  CommandGroup: Container,
  CommandItem: ({ children, onSelect }: any) => (
    <button type="button" onClick={() => onSelect?.()}>
      {children}
    </button>
  ),
  CommandSeparator: Container,
  CommandShortcut: Container,
}));
const originalCommand = { ...(await import('../../src/components/ui/command')) };
afterAll(() => {
  mock.module('../../src/components/ui/command', () => originalCommand);
});

mock.module('../../src/contexts/ConfigContext', () => ({
  useConfig: () => ({
    selectedLanguage: 'auto',
    transcriptModelConfig: { provider: 'whisper', model: 'large-v3' },
  }),
}));
const originalConfig = { ...(await import('../../src/contexts/ConfigContext')) };
afterAll(() => {
  mock.module('../../src/contexts/ConfigContext', () => originalConfig);
});

const notify = mock(() => {});
mock.module('sonner', () => ({
  toast: { info: notify, error: notify, success: notify, warning: notify },
}));
const originalToast = { ...(await import('sonner')) };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});

type MeetingSpeaker = import('../../src/types').MeetingSpeaker;
type SpeakerNameEntry = import('../../src/types').SpeakerNameEntry;

let speakers: MeetingSpeaker[] = [];
let directory: SpeakerNameEntry[] = [];
let assignError: string | null = null;

const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meeting_speakers') return speakers;
  if (command === 'api_get_speaker_names') {
    const query = String(args?.query ?? '').toLowerCase();
    return directory.filter((entry) => entry.name.toLowerCase().includes(query));
  }
  if (command === 'api_assign_meeting_speakers') {
    if (assignError) throw assignError;
    return speakers;
  }
  throw new Error(`Unexpected command: ${command}`);
});
// Keep the real exports (`transformCallback`, …) that other modules pull in;
// only `invoke` is replaced.
const originalCore = { ...(await import('@tauri-apps/api/core')) };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});
const coreMock = () => ({ ...originalCore, invoke });

mock.module('@tauri-apps/api/core', coreMock);

const { TranscriptButtonGroup } = await import(
  '../../src/components/MeetingDetails/TranscriptButtonGroup'
);

let renderer: ReactTestRenderer | undefined;

beforeEach(() => {
  mock.module('@tauri-apps/api/core', coreMock);
  (globalThis as any).window = {
    __TAURI_INTERNALS__: { invoke },
    dispatchEvent: () => true,
  };
  invoke.mockClear();
  notify.mockClear();
  onRefetchTranscripts.mockClear();
  sheetOnOpenChange = null;
  assignError = null;
  speakers = [];
  directory = [];
});

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

const onRefetchTranscripts = mock(async () => {});

async function render() {
  await act(async () => {
    renderer = create(
      <TranscriptButtonGroup
        transcriptCount={0}
        onCopyTranscript={() => {}}
        onExportTranscript={() => {}}
        onOpenMeetingFolder={async () => {}}
        meetingId="meeting-a"
        meetingFolderPath="/tmp/meeting-a"
        onRefetchTranscripts={onRefetchTranscripts}
      />
    );
  });
  return renderer!;
}

function nodeText(node: any): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(nodeText).join('');
  if (node?.children) return node.children.map(nodeText).join('');
  return '';
}

function text(instance: ReactTestRenderer): string {
  return nodeText(instance.toJSON());
}

/** Concatenated text of a mounted test instance subtree. */
function instanceText(node: any): string {
  return (node.children ?? [])
    .map((child: any) => (typeof child === 'string' ? child : instanceText(child)))
    .join('');
}

/** Clicks the first button whose subtree text contains `label`. Accepts the
 * renderer itself or any nested test instance (a row). */
async function clickIn(node: any, label: string) {
  const scoped = typeof node.findAll === 'function' ? node : node.root;
  const button = scoped.find(
    (n: any) => n.type === 'button' && instanceText(n).includes(label)
  );
  await act(async () => button.props.onClick());
}

async function clickButton(instance: any, label: string) {
  await clickIn(instance, label);
}

async function openPanel(instance: ReactTestRenderer) {
  const button = instance.root.find(
    (n) => n.type === 'button' && n.props.title === 'Speakers'
  );
  await act(async () => button.props.onClick());
}

function speakerRow(instance: ReactTestRenderer, label: string) {
  return instance.root.find(
    (n) => n.props?.['data-testid'] === `speaker-row-${label}`
  );
}

async function typeName(instance: ReactTestRenderer, label: string, value: string) {
  const input = speakerRow(instance, label).find((n: any) => n.type === 'input');
  await act(async () => input.props.onChange({ target: { value } }));
}

async function waitForDirectory() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 260));
  });
}

function assignCalls() {
  return invoke.mock.calls.filter(([command]) => command === 'api_assign_meeting_speakers');
}

test('rows show the Speaker N placeholder, segment count and current name', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 12,
      first_start_time: 0,
      top_speaker_names: ['Alex'],
    },
    {
      label: 'speaker_1',
      speaker_name: 'Maria',
      segment_count: 1,
      first_start_time: 30,
      top_speaker_names: ['Alex'],
    },
  ];

  const instance = await render();
  await openPanel(instance);
  const rendered = text(instance);

  expect(rendered).toContain('Speaker 1');
  expect(rendered).toContain('Speaker 2');
  expect(rendered).toContain('12 segments');
  expect(rendered).toContain('1 segment');
  expect(rendered).toContain('Maria');
});

test('picking an existing directory name then Done sends the assignment', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 3,
      first_start_time: 0,
      top_speaker_names: [],
    },
  ];
  directory = [{ id: 'speaker-alex', name: 'Alex', meeting_count: 4 }];

  const instance = await render();
  await openPanel(instance);

  await typeName(instance, 'speaker_0', 'al');
  await waitForDirectory();
  await clickButton(speakerRow(instance, 'speaker_0'), 'Alex');
  await clickButton(instance, 'Done');

  expect(assignCalls()).toEqual([
    [
      'api_assign_meeting_speakers',
      { meetingId: 'meeting-a', assignments: [{ label: 'speaker_0', name: 'Alex' }] },
    ],
  ]);
  expect(onRefetchTranscripts).toHaveBeenCalled();
});

test('Done only sends the rows whose draft changed', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: 'Alex',
      segment_count: 3,
      first_start_time: 0,
      top_speaker_names: ['Alex'],
    },
    {
      label: 'speaker_1',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 40,
      top_speaker_names: ['Maria'],
    },
  ];

  const instance = await render();
  await openPanel(instance);
  await clickButton(speakerRow(instance, 'speaker_1'), 'Maria');
  await clickButton(instance, 'Done');

  expect(assignCalls()).toEqual([
    [
      'api_assign_meeting_speakers',
      { meetingId: 'meeting-a', assignments: [{ label: 'speaker_1', name: 'Maria' }] },
    ],
  ]);
});

test('typing a new name offers Create and sends that name', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
  ];
  directory = [];

  const instance = await render();
  await openPanel(instance);

  await typeName(instance, 'speaker_0', 'Мария');
  await waitForDirectory();
  expect(text(instance)).toContain('Create „Мария“');

  await clickButton(speakerRow(instance, 'speaker_0'), 'Create „Мария“');
  await clickButton(instance, 'Done');

  expect(assignCalls()).toEqual([
    [
      'api_assign_meeting_speakers',
      { meetingId: 'meeting-a', assignments: [{ label: 'speaker_0', name: 'Мария' }] },
    ],
  ]);
});

test('an exact case-insensitive match does not offer Create', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
  ];
  directory = [{ id: 'speaker-alex', name: 'Alex', meeting_count: 4 }];

  const instance = await render();
  await openPanel(instance);

  await typeName(instance, 'speaker_0', '  aLeX ');
  await waitForDirectory();

  const row = speakerRow(instance, 'speaker_0');
  expect(instanceText(row)).toContain('Alex');
  expect(text(instance)).not.toContain('Create „');
});

test('No name is sent as null', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: 'Alex',
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: ['Alex'],
    },
  ];
  directory = [{ id: 'speaker-alex', name: 'Alex', meeting_count: 4 }];

  const instance = await render();
  await openPanel(instance);

  await clickButton(speakerRow(instance, 'speaker_0'), 'No name');
  await clickButton(instance, 'Done');

  expect(assignCalls()).toEqual([
    [
      'api_assign_meeting_speakers',
      { meetingId: 'meeting-a', assignments: [{ label: 'speaker_0', name: null }] },
    ],
  ]);
});

test('dismissing the sheet (Escape / outside click) discards the draft', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
  ];

  const instance = await render();
  await openPanel(instance);

  await typeName(instance, 'speaker_0', 'Ghost');
  await waitForDirectory();
  await clickButton(speakerRow(instance, 'speaker_0'), 'Create „Ghost“');

  await act(async () => sheetOnOpenChange?.(false));

  expect(assignCalls()).toHaveLength(0);
  expect(nodeText(instance.toJSON())).not.toContain('Ghost');
});

test('Cancel discards the draft without calling the backend', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
  ];

  const instance = await render();
  await openPanel(instance);

  await typeName(instance, 'speaker_0', 'Ghost');
  await waitForDirectory();
  await clickButton(speakerRow(instance, 'speaker_0'), 'Create „Ghost“');
  await clickButton(instance, 'Cancel');

  expect(assignCalls()).toHaveLength(0);

  // Reopening reloads the panel from the backend, so the discarded draft is gone.
  await openPanel(instance);
  expect(text(instance)).not.toContain('Ghost');
  expect(text(instance)).toContain('No name');
});

test('empty panel shows the diarization hint', async () => {
  speakers = [];

  const instance = await render();
  await openPanel(instance);

  const { DIARIZATION_UNAVAILABLE_HINT } = await import('../../src/lib/diarization');
  expect(text(instance)).toContain(DIARIZATION_UNAVAILABLE_HINT);
});

test('a conflict lands on the edited row, not on the speaker number from the message', async () => {
  // The backend says "already assigned to Speaker 1", but the row the user
  // edited in this batch is the second one. The message must land there.
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: 'Alex',
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: ['Alex', 'Zoe'],
    },
    {
      label: 'speaker_1',
      speaker_name: null,
      segment_count: 1,
      first_start_time: 20,
      top_speaker_names: ['Alex', 'Zoe'],
    },
  ];
  directory = [
    { id: 'speaker-alex', name: 'Alex', meeting_count: 4 },
    { id: 'speaker-zoe', name: 'Zoe', meeting_count: 1 },
  ];
  assignError = 'this person is already assigned to Speaker 1';

  const instance = await render();
  await openPanel(instance);

  // Only the second row is edited, typed in a different case than the saved name.
  await typeName(instance, 'speaker_1', '  aLeX ');
  await waitForDirectory();
  await clickButton(speakerRow(instance, 'speaker_1'), 'Alex');
  await clickButton(instance, 'Done');

  const edited = instanceText(speakerRow(instance, 'speaker_1'));
  expect(edited).toContain('this person is already assigned to Speaker 1');
  expect(instanceText(speakerRow(instance, 'speaker_0'))).not.toContain(
    'already assigned',
  );
});

test('a conflict between two newly typed names lands on the edited row', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
    {
      label: 'speaker_1',
      speaker_name: null,
      segment_count: 1,
      first_start_time: 20,
      top_speaker_names: [],
    },
  ];
  directory = [];
  assignError = 'this person is already assigned to Speaker 1';

  const instance = await render();
  await openPanel(instance);

  for (const [label, name] of [
    ['speaker_0', 'Maria'],
    ['speaker_1', ' maria '],
  ] as const) {
    await typeName(instance, label, name);
    await waitForDirectory();
    await clickButton(speakerRow(instance, label), `Create „${name.trim()}“`);
  }
  await clickButton(instance, 'Done');

  const rendered = text(instance);
  expect(rendered).toContain('this person is already assigned to Speaker 1');
  expect(instanceText(speakerRow(instance, 'speaker_1'))).toContain('already assigned');
});

test('a backend conflict keeps the panel open and shows the message on the row', async () => {
  speakers = [
    {
      label: 'speaker_0',
      speaker_name: null,
      segment_count: 2,
      first_start_time: 0,
      top_speaker_names: [],
    },
    {
      label: 'speaker_1',
      speaker_name: null,
      segment_count: 1,
      first_start_time: 20,
      top_speaker_names: [],
    },
  ];
  directory = [{ id: 'speaker-alex', name: 'Alex', meeting_count: 4 }];
  assignError = 'this person is already assigned to Speaker 1';

  const instance = await render();
  await openPanel(instance);

  for (const label of ['speaker_0', 'speaker_1']) {
    await typeName(instance, label, 'al');
    await waitForDirectory();
    await clickButton(speakerRow(instance, label), 'Alex');
  }

  await clickButton(instance, 'Done');

  const rendered = text(instance);
  expect(rendered).toContain('this person is already assigned to Speaker 1');
  expect(rendered).toContain('Done');
  expect(onRefetchTranscripts).not.toHaveBeenCalled();
});
