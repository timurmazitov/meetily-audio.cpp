import React from 'react';
import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// The component talks to the Tauri backend through `invoke`; tests replace it
// with a scripted fake and restore every mocked module afterwards because Bun
// shares module mocks between test files.
const originalToast = { ...(await import('sonner')) };
const originalCore = { ...(await import('@tauri-apps/api/core')) };
afterAll(() => {
  mock.module('sonner', () => originalToast);
  mock.module('@tauri-apps/api/core', () => originalCore);
});

const DEFAULT_PROMPT = 'Default prompt with {section_instructions} and {template}';
const notify = mock(() => {});
mock.module('sonner', () => ({
  toast: { info: notify, error: notify, success: notify, warning: notify },
}));

let stored = '';
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_summary_system_prompt') {
    return stored.trim() ? stored : DEFAULT_PROMPT;
  }
  if (command === 'api_save_summary_system_prompt') {
    stored = (args?.prompt as string) ?? '';
    return { status: 'success' };
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ invoke }));

const { SummarySystemPromptSettings } = await import(
  '../../src/components/SummarySystemPromptSettings'
);

let renderer: ReactTestRenderer | undefined;
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  stored = '';
  invoke.mockClear();
});

async function render() {
  await act(async () => {
    renderer = create(<SummarySystemPromptSettings />);
  });
  return renderer!;
}

function text(instance: ReactTestRenderer): string {
  return JSON.stringify(instance.toJSON());
}

function nodeText(node: any): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(nodeText).join('');
  if (node?.children) return node.children.map(nodeText).join('');
  return '';
}

async function clickButton(instance: ReactTestRenderer, label: string) {
  const button = instance.root.find(
    (node) => node.type === 'button' && nodeText(node.props.children).includes(label),
  );
  await act(async () => button.props.onClick());
}

async function typeIntoPrompt(instance: ReactTestRenderer, value: string) {
  const textarea = instance.root.find((node) => node.type === 'textarea');
  await act(async () => textarea.props.onChange({ target: { value } }));
}

test('loads the current base system prompt', async () => {
  stored = 'My custom base prompt {template}';
  const view = await render();

  expect(invoke).toHaveBeenCalledWith('api_get_summary_system_prompt');
  const textarea = view.root.find((node) => node.type === 'textarea');
  expect(textarea.props.value).toBe('My custom base prompt {template}');
});

test('shows the placeholders hint', async () => {
  const view = await render();

  expect(text(view)).toContain('{section_instructions}');
  expect(text(view)).toContain('{template}');
});

test('saves the edited prompt', async () => {
  const view = await render();
  await typeIntoPrompt(view, 'Edited prompt {section_instructions}');
  await clickButton(view, 'Save prompt');

  const saveCall = invoke.mock.calls.find(
    ([command]) => command === 'api_save_summary_system_prompt',
  );
  expect(saveCall?.[1]).toEqual({ prompt: 'Edited prompt {section_instructions}' });
});

test('reset stores an empty override and restores the default text', async () => {
  stored = 'Custom prompt';
  const view = await render();

  await clickButton(view, 'Reset to default');

  const saveCall = invoke.mock.calls.find(
    ([command]) => command === 'api_save_summary_system_prompt',
  );
  expect(saveCall?.[1]).toEqual({ prompt: '' });

  const textarea = view.root.find((node) => node.type === 'textarea');
  expect(textarea.props.value).toBe(DEFAULT_PROMPT);
});
