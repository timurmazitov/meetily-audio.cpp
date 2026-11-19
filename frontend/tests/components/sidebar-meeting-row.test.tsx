import React from 'react';
import { afterEach, expect, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import { MeetingFileItem } from '../../src/components/Sidebar/MeetingFileItem';

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
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

  const title = view.root.find(
    (node) => node.type === 'span' && textOf(node.props.children).includes('A very long meeting title'),
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
