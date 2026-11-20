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
const WHISPER_ONLY_MESSAGE = en.languageSelection.translationWhisperOnly;
const AUDIOCPP_TITLE = en.languageSelection.audioCppTitle;
const TRANSCRIBECPP_TITLE = en.languageSelection.transcribeCppTitle;
const TRANSLATION_UNAVAILABLE = en.languageSelection.translationUnavailable;

let renderer: ReactTestRenderer | undefined;

afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

function textOf(node: ReactTestRenderer) {
  return JSON.stringify(node.toJSON());
}

async function render(provider: string, selectedLanguage = 'auto') {
  await act(async () => {
    renderer = create(
      <LanguageSelection
        selectedLanguage={selectedLanguage}
        onLanguageChange={() => {}}
        provider={provider as never}
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

test('audioCpp disables auto-translation and explains it is Whisper-only', async () => {
  const view = await render('audioCpp');

  expect(optionValues(view)).not.toContain('auto-translate');
  expect(textOf(view)).toContain(AUDIOCPP_TITLE);
  expect(textOf(view)).toContain(WHISPER_ONLY_MESSAGE);
});

test('Whisper keeps automatic translation available', async () => {
  const view = await render('localWhisper');

  expect(optionValues(view)).toContain('auto-translate');
  expect(textOf(view)).not.toContain(AUDIOCPP_TITLE);
});

test('transcribeCpp disables auto-translation like audioCpp and explains it is Whisper-only', async () => {
  const view = await render('transcribeCpp');

  expect(optionValues(view)).not.toContain('auto-translate');
  expect(textOf(view)).toContain(TRANSCRIBECPP_TITLE);
  expect(textOf(view)).toContain('Transcribe.cpp');
});

test('a stale auto-translate preference does not misreport transcribeCpp output as translated', async () => {
  // Whisper saved auto-translate, then the user switched to transcribe.cpp.
  const view = await render('transcribeCpp', 'auto-translate');

  expect(textOf(view)).toContain(TRANSLATION_UNAVAILABLE);
  // The select falls back to auto-detect, so the UI does not claim English.
  const select = view.root.findByType('select');
  expect(select.props.value).toBe('auto');
});

test('Parakeet keeps auto-detection only', async () => {
  const view = await render('parakeet');

  // Parakeet offers auto-detect and auto-translate only (no manual language
  // selection), exactly as it did before audioCpp was added.
  expect(optionValues(view)).toEqual(['auto', 'auto-translate']);
  expect(textOf(view)).not.toContain(AUDIOCPP_TITLE);
});

test('Parakeet still offers automatic translation', async () => {
  const view = await render('parakeet', 'auto-translate');

  expect(optionValues(view)).toContain('auto-translate');
  expect(textOf(view)).not.toContain(TRANSLATION_UNAVAILABLE);
  expect(view.root.findByType('select').props.value).toBe('auto-translate');
});

test('a stale auto-translate preference does not misreport audioCpp output as translated', async () => {
  // Whisper saved auto-translate, then the user switched to audioCpp.
  const view = await render('audioCpp', 'auto-translate');

  expect(textOf(view)).toContain(TRANSLATION_UNAVAILABLE);
  expect(textOf(view)).toContain(WHISPER_ONLY_MESSAGE);
  // The select falls back to auto-detect, so the UI does not claim English.
  const select = view.root.findByType('select');
  expect(select.props.value).toBe('auto');
});

test('choosing a language reports it upward', async () => {
  const changes: string[] = [];
  await act(async () => {
    renderer = create(
      <LanguageSelection
        selectedLanguage="auto"
        onLanguageChange={language => changes.push(language)}
        provider={'audioCpp' as never}
      />,
    );
  });

  await act(async () => {
    renderer!.root.findByType('select').props.onChange({ target: { value: 'ru' } });
  });

  expect(changes).toEqual(['ru']);
});
