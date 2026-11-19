import React from 'react';
import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The transcript view renders Radix tooltips; tests only need the text, so
// the provider/shell components are replaced with plain containers.
const Container = ({ children }: any) => <div>{children}</div>;
const originalTooltip = { ...(await import('../../src/components/ui/tooltip')) };
mock.module('../../src/components/ui/tooltip', () => ({
  Tooltip: Container,
  TooltipTrigger: Container,
  TooltipContent: Container,
  TooltipProvider: Container,
}));
afterAll(() => mock.module('../../src/components/ui/tooltip', () => originalTooltip));

// framer-motion needs a DOM to detect SVG elements; these tests only assert
// text, so its primitives are replaced with plain containers. The mock is not
// restored afterwards: bun installs every file's mocks before running any test,
// so an `afterAll` restore here would replace the real module in the other
// suites mid-process (framer-motion then throws on the missing `SVGElement`).
const motionContainer = (props: any) => <div>{props.children}</div>;
mock.module('framer-motion', () => ({
  motion: new Proxy({}, { get: () => motionContainer }),
  AnimatePresence: Container,
}));

// The speaker dropdown is a Radix portal that needs a DOM. The wrapper is
// replaced with plain elements: the trigger/content become containers and
// each menu item becomes an <li> whose onClick is the real handler.
const originalDropdown = { ...(await import('../../src/components/ui/dropdown-menu')) };
const DropdownContainer = ({ children }: any) => <div>{children}</div>;
const DropdownItem = ({ children, onSelect, onClick }: any) => (
  <li onSelect={onSelect} onClick={onClick}>{children}</li>
);
mock.module('../../src/components/ui/dropdown-menu', () => ({
  DropdownMenu: DropdownContainer,
  DropdownMenuTrigger: DropdownContainer,
  DropdownMenuContent: DropdownContainer,
  DropdownMenuItem: DropdownItem,
  DropdownMenuLabel: DropdownContainer,
  DropdownMenuSeparator: () => <hr />,
  DropdownMenuGroup: DropdownContainer,
  DropdownMenuCheckboxItem: DropdownItem,
  DropdownMenuRadioItem: DropdownItem,
  DropdownMenuRadioGroup: DropdownContainer,
}));
afterAll(() => mock.module('../../src/components/ui/dropdown-menu', () => originalDropdown));

// The recording status bar reads the recording state context; only the live
// (non-interactive) case renders it, so a static idle state is enough.
const originalRecordingState = { ...(await import('../../src/contexts/RecordingStateContext')) };
mock.module('../../src/contexts/RecordingStateContext', () => ({
  ...originalRecordingState,
  useRecordingState: () => ({ isRecording: true, isPaused: false, activeDuration: null }),
}));
afterAll(() =>
  mock.module('../../src/contexts/RecordingStateContext', () => originalRecordingState),
);

// Row edits go through Tauri; tests record the payload and control failures.
// The real namespace is spread in so exports other suites need
// (`transformCallback`, `convertFileSrc`, …) keep resolving.
const originalCore = { ...(await import('@tauri-apps/api/core')) };
let saveSpeaker: (args?: Record<string, unknown>) => Promise<unknown>;
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_set_transcript_speaker') return saveSpeaker(args);
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));

// Errors are surfaced with a toast; capture them. Like `framer-motion`, this
// mock stays installed for the process: restoring the real module would strip
// the `sonner` mocks other suites registered before their tests ran.
const successNotify = mock(() => {});
const errorNotify = mock(() => {});
const infoNotify = mock(() => {});
const warningNotify = mock(() => {});
mock.module('sonner', () => ({
  toast: {
    success: successNotify,
    error: errorNotify,
    info: infoNotify,
    warning: warningNotify,
  },
}));

beforeEach(() => {
  // Another suite may have restored the real module while this file ran, so
  // re-apply the fake backend before every test. The `window` shim covers the
  // case where the real module is already bound: its `invoke` delegates to
  // `window.__TAURI_INTERNALS__.invoke`.
  mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: { __TAURI_INTERNALS__: { invoke } },
  });
  saveSpeaker = async () => ({ message: 'Transcript speaker saved successfully' });
  invoke.mockClear();
  successNotify.mockClear();
  errorNotify.mockClear();
  infoNotify.mockClear();
  warningNotify.mockClear();
});

const { VirtualizedTranscriptView } = await import(
  '../../src/components/VirtualizedTranscriptView'
);
type TranscriptSegmentData = import('../../src/types').TranscriptSegmentData;
type MeetingSpeaker = import('../../src/types').MeetingSpeaker;
type ViewProps = import('../../src/components/VirtualizedTranscriptView').VirtualizedTranscriptViewProps;

const speakers: MeetingSpeaker[] = [
  { label: 'speaker_0', speaker_name: 'Alex', segment_count: 3, first_start_time: 0 },
  { label: 'speaker_1', speaker_name: null, segment_count: 1, first_start_time: 12 },
];

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  Reflect.deleteProperty(globalThis, 'window');
});

async function render(
  segments: TranscriptSegmentData[],
  props: Partial<ViewProps> = {},
) {
  await act(async () => {
    renderer = create(
      <VirtualizedTranscriptView
        segments={segments}
        isRecording={false}
        showConfidence={false}
        {...props}
      />,
      {
        createNodeMock: () => ({
          addEventListener: () => {},
          removeEventListener: () => {},
          scrollTop: 0,
          scrollHeight: 0,
          clientHeight: 0,
          getBoundingClientRect: () => ({
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            width: 0,
            height: 0,
          }),
          observe: () => {},
          disconnect: () => {},
        }),
      },
    );
  });
  return renderer!;
}

function text(instance: ReactTestRenderer): string {
  return JSON.stringify(instance.toJSON());
}

function nodeText(node: any): string {
  if (node === null || node === undefined || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(nodeText).join('');
  if (node.props) return nodeText(node.props.children);
  if (node.children) return nodeText(node.children);
  return '';
}

/** Menu items are rendered as <li> by the dropdown mock. */
function menuItems(instance: ReactTestRenderer) {
  return instance.root.findAll(n => n.type === 'li');
}
function menuLabels(instance: ReactTestRenderer): string[] {
  return menuItems(instance).map(item => nodeText(item.props.children).trim());
}
/** The clickable speaker chip (a <button>); plain chips are not buttons. */
function chip(instance: ReactTestRenderer) {
  return instance.root.findAll(n => n.type === 'button')[0];
}
async function chooseMenu(instance: ReactTestRenderer, label: string) {
  const item = menuItems(instance).find(i => nodeText(i.props.children).includes(label));
  expect(item).toBeDefined();
  await act(async () => {
    await (item!.props.onSelect ?? item!.props.onClick)();
  });
}

// --- Existing behaviour: plain chips -------------------------------------

test('renders a speaker chip for a diarized segment', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Hello there', speaker: 'speaker_0' },
  ]);

  expect(text(view)).toContain('Speaker 1');
  expect(text(view)).toContain('Hello there');
});

test('speaker numbers follow order of appearance, 1-based', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'First', speaker: 'speaker_0' },
    { id: 't2', timestamp: 1, text: 'Second', speaker: 'speaker_1' },
  ]);

  expect(text(view)).toContain('Speaker 1');
  expect(text(view)).toContain('Speaker 2');
});

test('renders no speaker chip when the segment was not diarized', async () => {
  const view = await render([{ id: 't1', timestamp: 0, text: 'No labels here' }]);

  expect(text(view)).not.toContain('Speaker ');
  expect(text(view)).toContain('No labels here');
});

test('renders no speaker chip for a legacy audio-source value', async () => {
  // The reused `speaker` column documented 'mic'/'system' before
  // diarization; those are audio sources, not speakers, and must not
  // surface as "mic" chips on old meetings.
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Legacy row', speaker: 'mic' },
    { id: 't2', timestamp: 1, text: 'Legacy row two', speaker: 'system' },
  ]);

  expect(text(view)).not.toContain('Speaker ');
  expect(text(view)).not.toContain('mic');
  expect(text(view)).not.toContain('system');
  expect(text(view)).toContain('Legacy row');
});

test('renders no chip for an unrecognized free-form speaker value', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Custom label', speaker: 'Guest A' },
  ]);

  expect(text(view)).not.toContain('Guest A');
  expect(text(view)).not.toContain('Speaker ');
});

test('legacy audio-source values stay inert even when the meeting has speakers', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Legacy row', speaker: 'mic' }],
    { meetingSpeakers: speakers, onSpeakerChange: async () => {} },
  );

  expect(chip(view)).toBeUndefined();
  expect(menuLabels(view)).toEqual([]);
});

// --- Chip shows the assigned name ----------------------------------------

test('chip shows the assigned speaker name instead of the label number', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' },
  ]);

  expect(text(view)).toContain('Alex');
  expect(text(view)).not.toContain('Speaker 1');
});

test('chip falls back to Speaker N when the label has no name', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_1', speaker_name: null as unknown as undefined },
  ]);

  expect(text(view)).toContain('Speaker 2');
});

// --- Per-row dropdown ----------------------------------------------------

test('the row dropdown lists the meeting speakers and No speaker', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: speakers, onSpeakerChange: async () => {} },
  );

  expect(menuLabels(view)).toEqual(['Alex', 'Speaker 2', 'No speaker']);
});

test('a segment without a label also opens the dropdown and shows No speaker', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Background noise' }],
    { meetingSpeakers: speakers, onSpeakerChange: async () => {} },
  );

  expect(nodeText(chip(view).props.children)).toContain('No speaker');
  expect(menuLabels(view)).toEqual(['Alex', 'Speaker 2', 'No speaker']);
});

test('choosing a speaker saves the label and updates the row locally', async () => {
  const changed = mock(async (_id: string, _label: string | null) => {});
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0' }],
    { meetingSpeakers: speakers, onSpeakerChange: changed },
  );

  await chooseMenu(view, 'Speaker 2');

  expect(invoke.mock.calls).toContainEqual([
    'api_set_transcript_speaker',
    { transcriptId: 't1', label: 'speaker_1' },
  ]);
  expect(nodeText(chip(view).props.children)).toContain('Speaker 2');
  expect(changed.mock.calls).toContainEqual(['t1', 'speaker_1']);
});

test('choosing No speaker saves a null label', async () => {
  const changed = mock(async (_id: string, _label: string | null) => {});
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: speakers, onSpeakerChange: changed },
  );

  await chooseMenu(view, 'No speaker');

  expect(invoke.mock.calls).toContainEqual([
    'api_set_transcript_speaker',
    { transcriptId: 't1', label: null },
  ]);
  expect(nodeText(chip(view).props.children)).toContain('No speaker');
  expect(changed.mock.calls).toContainEqual(['t1', null]);
});

test('a failed save leaves the row unchanged and toasts an error', async () => {
  saveSpeaker = async () => {
    throw new Error('database is locked');
  };
  const changed = mock(async (_id: string, _label: string | null) => {});
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0' }],
    { meetingSpeakers: speakers, onSpeakerChange: changed },
  );

  await chooseMenu(view, 'Speaker 2');

  expect(nodeText(chip(view).props.children)).toContain('Speaker 1');
  expect(changed).not.toHaveBeenCalled();
  expect(errorNotify).toHaveBeenCalled();
});

// --- Dropdown order ------------------------------------------------------
//
// The dropdown lists the meeting's own speakers only, but in alphabetical
// order by the label the user sees: Cyrillic before Latin, case-insensitive
// (the shared `comparePersonNames` rule). Unnamed speakers follow, ordered by
// their label number rather than by where the page happened to put them, and
// No speaker stays the last item.

const namedSpeakers: MeetingSpeaker[] = [
  { label: 'speaker_0', speaker_name: 'Zoe', segment_count: 2, first_start_time: 0 },
  { label: 'speaker_1', speaker_name: 'анна', segment_count: 2, first_start_time: 5 },
  { label: 'speaker_2', speaker_name: 'Alex', segment_count: 2, first_start_time: 9 },
];

// First-appearance order of the loaded page: the unnamed speaker_1 was seen
// before the unnamed speaker_2, and both before the named one.
const pageOrderedSpeakers: MeetingSpeaker[] = [
  { label: 'speaker_2', speaker_name: null, segment_count: 1, first_start_time: 0 },
  { label: 'speaker_1', speaker_name: null, segment_count: 1, first_start_time: 3 },
  { label: 'speaker_0', speaker_name: 'Alex', segment_count: 1, first_start_time: 7 },
];

test('named speakers are listed alphabetically by their visible label', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Zoe' }],
    { meetingSpeakers: namedSpeakers, onSpeakerChange: async () => {} },
  );

  // Cyrillic first, then Latin, case-insensitively: the same rule as the
  // Speakers panel's name list.
  expect(menuLabels(view)).toEqual(['анна', 'Alex', 'Zoe', 'No speaker']);
});

test('unnamed speakers come after named ones, ordered by label number', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: pageOrderedSpeakers, onSpeakerChange: async () => {} },
  );

  // speaker_1 is "Speaker 2" and speaker_2 is "Speaker 3", whatever order the
  // page listed them in.
  expect(menuLabels(view)).toEqual(['Alex', 'Speaker 2', 'Speaker 3', 'No speaker']);
});

test('the dropdown order does not depend on the order of the loaded page', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    {
      meetingSpeakers: [...pageOrderedSpeakers].reverse(),
      onSpeakerChange: async () => {},
    },
  );

  expect(menuLabels(view)).toEqual(['Alex', 'Speaker 2', 'Speaker 3', 'No speaker']);
});

test('No speaker stays the last item in both directions of the page order', async () => {
  const forward = await render(
    [{ id: 't1', timestamp: 0, text: 'One', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: namedSpeakers, onSpeakerChange: async () => {} },
  );
  expect(menuLabels(forward).at(-1)).toBe('No speaker');

  await act(async () => forward.unmount());
  renderer = undefined;

  const backward = await render(
    [{ id: 't1', timestamp: 0, text: 'Two', speaker: 'speaker_0', speaker_name: 'Alex' }],
    {
      meetingSpeakers: [...namedSpeakers].reverse(),
      onSpeakerChange: async () => {},
    },
  );
  expect(menuLabels(backward).at(-1)).toBe('No speaker');
});

test('the dropdown still lists only this meeting\'s speakers plus No speaker', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: namedSpeakers, onSpeakerChange: async () => {} },
  );

  // Exactly the meeting's three speakers and the trailing "No speaker": no
  // directory people who did not appear in this meeting.
  expect(menuLabels(view)).toEqual(['анна', 'Alex', 'Zoe', 'No speaker']);
  expect(menuLabels(view)).toHaveLength(namedSpeakers.length + 1);
});

test('sorting the dropdown leaves the meeting speaker order untouched', async () => {
  const inputOrder = pageOrderedSpeakers.map(speaker => speaker.label);

  await render(
    [{ id: 't1', timestamp: 0, text: 'Hello', speaker: 'speaker_0', speaker_name: 'Alex' }],
    { meetingSpeakers: pageOrderedSpeakers, onSpeakerChange: async () => {} },
  );

  // The panel keeps first-appearance order; the chip sorts a copy.
  expect(pageOrderedSpeakers.map(speaker => speaker.label)).toEqual(inputOrder);
});

// --- Live panel stays non-interactive ------------------------------------

test('a live recording has no clickable chip and no dropdown', async () => {
  const view = await render(
    [{ id: 't1', timestamp: 0, text: 'Live text', speaker: 'speaker_0' }],
    { isRecording: true, meetingSpeakers: speakers, onSpeakerChange: async () => {} },
  );

  expect(chip(view)).toBeUndefined();
  expect(menuLabels(view)).toEqual([]);
  expect(text(view)).toContain('Speaker 1');
});

test('without meeting speakers the chip stays a plain label', async () => {
  const view = await render([
    { id: 't1', timestamp: 0, text: 'Meeting text', speaker: 'speaker_0' },
  ]);

  expect(chip(view)).toBeUndefined();
  expect(menuLabels(view)).toEqual([]);
  expect(text(view)).toContain('Speaker 1');
});

test('a meeting without speakers adds no interactivity and no chip', async () => {
  const view = await render([{ id: 't1', timestamp: 0, text: 'No labels' }], {
    meetingSpeakers: [],
    onSpeakerChange: async () => {},
  });

  expect(chip(view)).toBeUndefined();
  expect(menuLabels(view)).toEqual([]);
});
