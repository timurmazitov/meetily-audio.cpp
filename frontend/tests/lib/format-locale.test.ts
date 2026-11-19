import { describe, expect, test } from 'bun:test';

import {
  formatMegabytes,
  formatDecimal,
  intlLocale,
} from '../../src/lib/format-locale';

/**
 * Sizes and decimals follow the interface language: Russian writes a decimal
 * comma and Russian unit abbreviations, English the reverse.
 */

describe('formatDecimal', () => {
  test('Russian uses a decimal comma, English a point', () => {
    expect(formatDecimal(1.2, 'ru')).toBe('1,2');
    expect(formatDecimal(1.2, 'en')).toBe('1.2');
  });

  test('thousands are grouped with a space in Russian, a comma in English', () => {
    // `Intl` uses a (non-breaking) space for Russian's group separator.
    expect(formatDecimal(1000, 'ru', 0)).toMatch(/^1\s000$/);
    expect(formatDecimal(1000, 'en', 0)).toBe('1,000');
  });
});

describe('formatMegabytes', () => {
  test('large sizes flip to gigabytes with one decimal', () => {
    expect(formatMegabytes(2554, 'ru')).toBe('2,6 ГБ');
    expect(formatMegabytes(2554, 'en')).toBe('2.6 GB');
  });

  test('hundreds of megabytes keep no decimals', () => {
    expect(formatMegabytes(742, 'ru')).toBe('742 МБ');
    expect(formatMegabytes(742, 'en')).toBe('742 MB');
  });

  test('a small size keeps one decimal instead of collapsing to zero', () => {
    expect(formatMegabytes(0.4, 'ru')).toBe('0,4 МБ');
    expect(formatMegabytes(0.4, 'en')).toBe('0.4 MB');
  });
});

describe('intlLocale', () => {
  test('maps each interface language to its regional tag', () => {
    expect(intlLocale('ru')).toBe('ru-RU');
    expect(intlLocale('en')).toBe('en-US');
  });
});
