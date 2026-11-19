import { afterEach, describe, expect, mock, test } from 'bun:test';

/**
 * The window/document title follows the active interface language (spec
 * decision 7, audit BUG-2). Tested at two seams: `applyWindowTitle` (the
 * side-effect helper, with the Tauri window API — a system boundary —
 * mocked) and `LanguageProvider` (the title re-follows the language on
 * switch).
 */

// bun test has no DOM; stub the pieces the helper and the provider touch.
(globalThis as Record<string, unknown>).document = {
  title: '',
  documentElement: {},
};

afterEach(() => {
  (globalThis.document as { title: string }).title = '';
  delete (globalThis as Record<string, unknown>).window;
  delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
  setTitle.mockClear();
});

const setTitle = mock(async (_title: string) => {});
mock.module('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTitle }),
}));

const { applyWindowTitle } = await import('../../src/lib/app-window-title');

describe('applyWindowTitle', () => {
  test('sets the document title', async () => {
    await applyWindowTitle('Samobranka');
    expect(document.title).toBe('Samobranka');
    // No Tauri in the browser build: the window API must stay untouched.
    expect(setTitle).not.toHaveBeenCalled();
  });

  test('under Tauri also sets the native window title', async () => {
    globalThis.window = { __TAURI_INTERNALS__: {} } as never;

    await applyWindowTitle('Samobranka');

    expect(setTitle).toHaveBeenCalledWith('Samobranka');
  });

  test('a failing window API fails gracefully instead of throwing', async () => {
    globalThis.window = { __TAURI_INTERNALS__: {} } as never;
    setTitle.mockRejectedValueOnce(new Error('window API unavailable'));
    const originalError = console.error;
    const errors: unknown[] = [];
    console.error = (...args: unknown[]) => {
      errors.push(args);
    };

    try {
      // Must resolve anyway: a broken native title cannot break the switch.
      await applyWindowTitle('Samobranka');
    } finally {
      console.error = originalError;
    }

    expect(document.title).toBe('Samobranka');
    expect(errors.length).toBe(1);
  });
});
