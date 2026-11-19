import i18n, { DEFAULT_LANGUAGE, type AppLanguageCode } from '@/i18n';

/**
 * Locale-aware formatting for the non-React helpers.
 *
 * Components use the `t` from `useTranslation` directly. These helpers are
 * called from modules that render no JSX (`meeting-datetime`, `summary-document`),
 * so they resolve a fixed translator for the active language instead of
 * depending on hook state. Re-rendering on a language switch is the caller's
 * job: a component that formats during render re-renders when `i18n.language`
 * changes, because `useTranslation` subscribes to it.
 */

/** A translator bound to one interface language. */
export function translator(language: AppLanguageCode = DEFAULT_LANGUAGE) {
  return i18n.getFixedT(language);
}

/** The BCP-47 tag `Intl` should use for an interface language. */
export function intlLocale(language: AppLanguageCode = DEFAULT_LANGUAGE): string {
  return language === 'ru' ? 'ru-RU' : 'en-US';
}

/** `1,2` in Russian, `1.2` in English. */
export function formatDecimal(
  value: number,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
  fractionDigits = 1,
): string {
  return new Intl.NumberFormat(intlLocale(language), {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(value);
}

/**
 * A download size, in the units the model catalogs already use (megabytes or
 * gigabytes as a bare number): `2554` MB → `2,5 ГБ` / `2.5 GB`.
 *
 * One decimal as soon as the number is small enough for it to matter, none for
 * the three-digit values the catalogs mostly carry.
 */
export function formatMegabytes(
  megabytes: number,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
): string {
  const t = translator(language);
  if (megabytes >= 1000) {
    return t('units.gigabytesShort', { value: formatDecimal(megabytes / 1000, language) });
  }

  const digits = megabytes < 10 ? 1 : 0;
  return t('units.megabytesShort', { value: formatDecimal(megabytes, language, digits) });
}

/** A byte count as GB (used by the update dialog). */
export function formatGigabytes(
  bytes: number,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
): string {
  return translator(language)('units.gigabytesShort', {
    value: formatDecimal(bytes / (1024 * 1024 * 1024), language),
  });
}

/** A calendar date in the active language's convention. */
export function formatLocalDate(
  value: string | number | Date,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
): string {
  return new Date(value).toLocaleDateString(intlLocale(language));
}

/** A full date and time in the active language's convention. */
export function formatLocalDateTime(
  value: string | number | Date,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
): string {
  return new Date(value).toLocaleString(intlLocale(language));
}

/** A clock time in the active language's convention. */
export function formatLocalTime(
  value: string | number | Date,
  language: AppLanguageCode = DEFAULT_LANGUAGE,
): string {
  return new Date(value).toLocaleTimeString(intlLocale(language));
}
