import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The transcript view renders Radix tooltips; tests only need the text, so
// the provider/shell components are replaced with plain containers.
const Container = ({ children }: any) => <div>{children}</div>;
mock.module('../../src/components/ui/tooltip', () => ({
  Tooltip: Container,
  TooltipTrigger: Container,
  TooltipContent: Container,
  TooltipProvider: Container,
}));
const originalTooltip = { ...(await import('../../src/components/ui/tooltip')) };
afterAll(() => mock.module('../../src/components/ui/tooltip', () => originalTooltip));

// framer-motion needs a DOM to detect SVG elements; these tests only assert
// text, so its primitives are replaced with plain containers.
const motionContainer = (props: any) => <div>{props.children}</div>;
mock.module('framer-motion', () => ({
  motion: new Proxy({}, { get: () => motionContainer }),
  AnimatePresence: Container,
}));
const originalMotion = { ...(await import('framer-motion')) };
afterAll(() => mock.module('framer-motion', () => originalMotion));

const { VirtualizedTranscriptView } = await import(
  '../../src/components/VirtualizedTranscriptView'
);
type TranscriptSegmentData = import('../../src/types').TranscriptSegmentData;

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

async function render(segments: TranscriptSegmentData[]) {
  await act(async () => {
    renderer = create(
      <VirtualizedTranscriptView
        segments={segments}
        isRecording={false}
        showConfidence={false}
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
