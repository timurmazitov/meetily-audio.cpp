import { afterAll, afterEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

// LanguageSelection reaches for the config context on mount; stub that single
// edge so the component's own provider/language behaviour is what is tested.
// Tauri is loaded for real, so no partial module mock leaks
// into other test files in the same bun process.
const originals = await Promise.all(
  ['../../src/contexts/ConfigContext'].map(
    async path => [path, { ...(await import(path)) }] as const,
  ),
);
afterAll(() => {
  for (const [path, exports] of originals) mock.module(path, () => exports);
});

mock.module('../../src/contexts/ConfigContext', () => ({
  ...(originals[0]?.[1] ?? {}),
  useConfig: () => ({ setSelectedLanguage: () => {} }),
}));

const { LanguageSelection } = await import('../../src/components/LanguageSelection');
const en = (await import('../../src/i18n/locales/en/translation.json')).default;
const TRANSCRIBECPP_TITLE = en.languageSelection.transcribeCppTitle;

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

function textOf(node: ReactTestRenderer) {
  return JSON.stringify(node.toJSON());
}

async function render(selectedLanguage = 'auto') {
  await act(async () => {
    renderer = create(
      <LanguageSelection
        selectedLanguage={selectedLanguage}
        onLanguageChange={() => {}}
        provider="transcribeCpp"
      />,
    );
  });
  return renderer!;
}

function optionValues(view: ReactTestRenderer): string[] {
  return view.root
    .findAllByType('option')
    .map(option => option.props.value as string);
}

test('transcribeCpp offers no automatic-translation option', async () => {
  const view = await render();

  // Translation was a Whisper-only capability; with Whisper gone the option
  // must not exist anywhere in the list.
  expect(optionValues(view)).not.toContain('auto-translate');
  expect(textOf(view)).not.toContain('Translate to English');
});

test('transcribeCpp keeps auto-detect and specific languages', async () => {
  const view = await render();

  const values = optionValues(view);
  expect(values).toContain('auto');
  expect(values).toContain('ru');
  expect(values).toContain('en');
});

test('transcribeCpp explains that transcription keeps the spoken language', async () => {
  const view = await render();

  expect(textOf(view)).toContain(TRANSCRIBECPP_TITLE);
  expect(textOf(view)).toContain('Transcribe.cpp');
});

test('a stale auto-translate preference falls back to auto-detect', async () => {
  // The option is gone; a preference saved before the removal must not keep
  // the UI claiming English output.
  const view = await render('auto-translate');

  expect(view.root.findByType('select').props.value).toBe('auto');
});

test('choosing a language reports it upward', async () => {
  const changes: string[] = [];
  await act(async () => {
    renderer = create(
      <LanguageSelection
        selectedLanguage="auto"
        onLanguageChange={language => changes.push(language)}
        provider="transcribeCpp"
      />,
    );
  });

  await act(async () => {
    renderer!.root.findByType('select').props.onChange({ target: { value: 'ru' } });
  });

  expect(changes).toEqual(['ru']);
});
