/**
 * Language metadata for the supported interface locales.
 *
 * To add a language:
 * 1. Create `src/i18n/locales/{code}/translation.json`
 * 2. Register it in `RESOURCES` in `src/i18n/index.ts`
 * 3. Add an entry here with the English name, the native name and a priority
 *    (lower = higher in the picker)
 */
export interface LanguageMetadata {
  /** English name, used for the parenthesised hint in the picker. */
  name: string;
  /** Name in the language itself, what the picker shows first. */
  nativeName: string;
  /** Lower sorts first. */
  priority: number;
}

export const LANGUAGE_METADATA: Record<string, LanguageMetadata> = {
  en: { name: 'English', nativeName: 'English', priority: 1 },
  ru: { name: 'Russian', nativeName: 'Русский', priority: 2 },
};

/**
 * What the picker shows: the native name, plus the English name in parentheses
 * when the two differ. A user who switched by accident must still recognise
 * their language in a UI they cannot read.
 */
export function languageOptionLabel(language: {
  name: string;
  nativeName: string;
}): string {
  return language.name === language.nativeName
    ? language.nativeName
    : `${language.nativeName} (${language.name})`;
}
