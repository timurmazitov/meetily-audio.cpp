import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import i18n from '../../src/i18n';

// The component is imported after the mocks below so the real module binds to
// the faked tooltip wrappers and Tauri boundary.

// In the app the popup opens through the global tooltip provider, whose Radix
// portal never renders under react-test-renderer. Following the sidebar list
// suite, the tooltip wrappers are replaced with containers so the popup
// content is inspectable as visible text. The speaker count arrives through
// the row's loadSpeakerCount seam, so this suite scripts the loader directly
// and never touches the Tauri boundary.
const originals = await Promise.all(
  ['../../src/components/ui/tooltip'].map(
    async path => [path, { ...(await import(path)) }] as const,
  ),
);
afterAll(async () => {
  // Return to English while the mocks are still in place: restoring the
  // originals first would flip the component's live ESM bindings back to the
  // real Radix tooltip before the language event settles.
  await act(async () => { await i18n.changeLanguage('en'); });
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

const Container = ({ children, className }: { children?: React.ReactNode; className?: string }) => (
  <div className={className}>{children}</div>
);
mock.module('../../src/components/ui/tooltip', () => ({
  Tooltip: Container,
  // The marker names the popup subtree for the assertions below.
  TooltipContent: ({ children, className }: any) => (
    <div data-popup="meeting" className={className}>{children}</div>
  ),
  TooltipProvider: Container,
  TooltipTrigger: Container,
}));

/**
 * Scripted answer of the speaker-count loader the row gets as its seam:
 * a number, `null` for unknown, or an Error to reject the request.
 */
let speakerAnswer: number | null | Error = null;
/** Meeting ids the loader was asked about, in order. */
let loaderAsks: string[] = [];
const loadSpeakerCount = (meetingId: string) => {
  loaderAsks.push(meetingId);
  return speakerAnswer instanceof Error
    ? Promise.reject(speakerAnswer)
    : Promise.resolve(speakerAnswer);
};

const { MeetingFileItem } = await import('../../src/components/Sidebar/MeetingFileItem');

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  speakerAnswer = null;
  loaderAsks = [];
  await act(async () => { await i18n.changeLanguage('en'); });
});

/** Local moment, so the rendered text does not depend on the test machine's zone. */
const recordedAt = new Date(2026, 10, 18, 14, 32).toISOString();

async function render(props: Partial<React.ComponentProps<typeof MeetingFileItem>> = {}) {
  await act(async () => {
    renderer = create(
      <MeetingFileItem
        title="Planning call"
        recordedAt={null}
        durationSeconds={null}
        matchContext={null}
        loadSpeakerCount={loadSpeakerCount}
        onEdit={() => {}}
        onDelete={() => {}}
        {...props}
      />,
    );
  });
  return renderer!;
}

function textOf(node: any): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(textOf).join('');
  if (node?.children) return node.children.map(textOf).join('');
  return '';
}

function viewText(instance: ReactTestRenderer): string {
  return textOf(instance.toJSON());
}

function classNames(instance: ReactTestRenderer): string[] {
  return instance.root
    .findAll((node) => typeof node.type === 'string')
    .map((node) => String(node.props.className ?? ''));
}

test('the title is clamped to two lines instead of breaking anywhere', async () => {
  const view = await render({ title: 'A very long meeting title that would otherwise run forever' });

  // The row's own title span: the popup repeats the full title, so only the
  // clamped one (flex-1, next to the icon) is in view here.
  const title = view.root.find(
    (node) => node.type === 'span' && String(node.props.className).includes('flex-1')
      && textOf(node.props.children).includes('A very long meeting title'),
  );
  expect(title.props.className).toContain('line-clamp-2');
  expect(title.props.className).not.toContain('break-words');
});

test('the meeting moment and length share one quiet line under the title', async () => {
  const view = await render({ recordedAt, durationSeconds: 42 * 60 });

  const line = view.root.find(
    (node) => typeof node.type === 'string' && node.props.className === 'mt-1 ml-8 text-xs text-gray-500',
  );
  expect(textOf(line.props.children)).toBe('18/11/2026, 14:32 · 42 min');
});

test('a known part is shown without a dangling separator', async () => {
  const datedOnly = await render({ recordedAt });
  expect(viewText(datedOnly)).toContain('18/11/2026, 14:32');
  expect(viewText(datedOnly)).not.toContain('·');

  const durationOnly = await render({ durationSeconds: 42 * 60 });
  expect(viewText(durationOnly)).toContain('42 min');
  expect(viewText(durationOnly)).not.toContain('·');
});

test('neither part means no second line at all', async () => {
  const view = await render();

  expect(viewText(view)).not.toContain('·');
  expect(classNames(view).some((className) => className.includes('text-xs text-gray-500'))).toBe(false);
});

test('the search match stays below the date line', async () => {
  const view = await render({
    recordedAt,
    durationSeconds: 42 * 60,
    matchContext: '...we agreed on the release date...',
  });

  const column = view.root.find(
    (node) => typeof node.type === 'string' && node.props.className === 'flex flex-col w-full',
  );
  const rendered = (column.children as any[])
    .filter((child) => typeof child !== 'string')
    .map((child) => textOf(child));
  const dateLine = rendered.findIndex((text) => text.includes('18/11/2026, 14:32'));
  const matchLine = rendered.findIndex((text) => text.includes('Match:'));

  expect(dateLine).toBeGreaterThanOrEqual(0);
  expect(matchLine).toBeGreaterThan(dateLine);
});

test('the icon and the hover actions are still part of the row', async () => {
  const view = await render({ recordedAt });

  const labels = view.root
    .findAll((node) => typeof node.type === 'string' && node.type === 'button')
    .map((node) => node.props['aria-label']);
  expect(labels).toEqual(['Edit meeting details', 'Delete meeting']);
  expect(labels).toHaveLength(2);
});

// — The hover popup ---------------------------------------------------------

/** The popup subtree, marked by the mocked TooltipContent, or undefined. */
function popupOf(view: ReactTestRenderer) {
  return view.root.findAll((node) => node.props['data-popup'] === 'meeting')[0];
}

test('the popup carries the full title, the date and time, and the length', async () => {
  const longTitle = 'Quarterly roadmap alignment with the infrastructure and design teams';
  const view = await render({ meetingId: 'meeting-1', title: longTitle, recordedAt, durationSeconds: 42 * 60 });

  const popup = textOf(popupOf(view).children);
  // The full title, not the two-line-clamped one.
  expect(popup).toContain(longTitle);
  // The very same date and length strings the row itself shows.
  expect(popup).toContain('18/11/2026, 14:32');
  expect(popup).toContain('42 min');
});

test('the popup shows the speaker count once the loader reports speakers', async () => {
  speakerAnswer = 3;
  const view = await render({ meetingId: 'meeting-1', recordedAt });

  // The row must offer a hover entry point for the lazy fetch.
  const hoverTargets = view.root.findAll((node) => typeof node.props.onMouseEnter === 'function');
  expect(hoverTargets.length).toBeGreaterThanOrEqual(1);
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });

  const popup = textOf(popupOf(view).children);
  expect(popup).toContain('Speakers: 3');

  // Repeated hovers of the same row ask the loader no further.
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });
  expect(loaderAsks).toEqual(['meeting-1']);
});

async function unmountCurrent() {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
}

test('zero speakers, unknown speakers and a failed request all omit the line', async () => {
  speakerAnswer = 0;
  const empty = await render({ meetingId: 'meeting-1', recordedAt });
  let hoverTargets = empty.root.findAll((node) => typeof node.props.onMouseEnter === 'function');
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });
  let popup = textOf(popupOf(empty).children);
  expect(popup).not.toContain('Speakers');

  await unmountCurrent();

  speakerAnswer = null;
  const unknown = await render({ meetingId: 'meeting-1b', recordedAt });
  hoverTargets = unknown.root.findAll((node) => typeof node.props.onMouseEnter === 'function');
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });
  popup = textOf(popupOf(unknown).children);
  expect(popup).not.toContain('Speakers');

  await unmountCurrent();

  speakerAnswer = new Error('diarization backend is down');
  const failed = await render({ meetingId: 'meeting-2', recordedAt });
  hoverTargets = failed.root.findAll((node) => typeof node.props.onMouseEnter === 'function');
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });
  popup = textOf(popupOf(failed).children);
  expect(popup).not.toContain('Speakers');
});

test('a meeting the popup knows nothing about shows neither dashes nor zeros', async () => {
  const view = await render({ meetingId: 'meeting-3', title: 'Old call' });

  const popup = textOf(popupOf(view).children);
  expect(popup).toContain('Old call');
  expect(popup).not.toContain('—');
  expect(popup).not.toContain('Speakers');
});

test('the popup covers short titles too, not only clamped ones', async () => {
  const view = await render({ meetingId: 'meeting-4', title: 'Sync' });

  const popup = textOf(popupOf(view).children);
  expect(popup).toContain('Sync');
});

test('the popup does not intercept the mouse over the row', async () => {
  const view = await render({ meetingId: 'meeting-5', title: 'Sync' });

  const popup = popupOf(view);
  expect(String(popup.props.className)).toContain('pointer-events-none');
});

test('the popup follows the interface language', async () => {
  speakerAnswer = 2;
  await act(async () => { await i18n.changeLanguage('ru'); });
  const view = await render({ meetingId: 'meeting-ru', recordedAt, durationSeconds: 4975.5 });

  const hoverTargets = view.root.findAll((node) => typeof node.props.onMouseEnter === 'function');
  await act(async () => { hoverTargets[0].props.onMouseEnter(); });

  const popup = textOf(popupOf(view).children);
  expect(popup).toContain('18.11.2026, 14:32');
  expect(popup).toContain('1 ч 23 мин');
  expect(popup).toContain('Спикеров: 2');
});
