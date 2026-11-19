import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import type { AppLanguageCode } from '@/i18n';
import { intlLocale } from '@/lib/format-locale';

/**
 * Names of the *content* languages — the languages you can transcribe or
 * summarize in — as opposed to the interface language.
 *
 * There are a hundred of them in the Whisper list, so the names come from
 * `Intl.DisplayNames` instead of a hand-written catalog: it knows every tag in
 * the lists and formats it per language for free. English keeps the labels the
 * data files already carry, so nothing changes for the language the app was
 * written in; anything else is localized by the platform.
 */

/** `zh-tw` → `zh-TW`, which is what `Intl` expects. */
function toBcp47(code: string): string {
  return code
    .split('-')
    .map((part, index) => (index === 0 ? part.toLowerCase() : part.toUpperCase()))
    .join('-');
}

const REGIONAL_OVERRIDES: Record<string, Partial<Record<AppLanguageCode, string>>> = {
  // `Intl` reads `zh-TW` as "Chinese (Taiwan)" rather than naming the script.
  'zh-tw': { ru: 'Китайский (традиционный)' },
};

function capitalize(value: string): string {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

/**
 * The name of `code` in `language`, falling back to the label the list carries
 * (English) when the platform cannot name it.
 */
export function contentLanguageName(
  code: string,
  language: AppLanguageCode,
  fallback: string,
): string {
  if (language === 'en') return fallback;

  const override = REGIONAL_OVERRIDES[code]?.[language];
  if (override) return override;

  try {
    const names = new Intl.DisplayNames([intlLocale(language)], { type: 'language' });
    const name = names.of(toBcp47(code));
    return name ? capitalize(name) : fallback;
  } catch {
    return fallback;
  }
}

/** React flavour: re-renders when the interface language changes. */
export function useContentLanguageName() {
  const { i18n } = useTranslation();
  const language = i18n.language as AppLanguageCode;

  return useCallback(
    (code: string, fallback: string) => contentLanguageName(code, language, fallback),
    [language],
  );
}
