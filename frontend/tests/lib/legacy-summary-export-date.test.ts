import { describe, expect, test } from 'bun:test';

import { legacySummaryExportDate } from '../../src/components/AISummary';

/**
 * The legacy summary export builds a `## Date:` header from the meeting it is
 * given. It must prefer the meeting's own moment; `created_at` names when the
 * database row appeared (for a recording, after the call had ended — ADR-0005).
 */
describe('legacySummaryExportDate', () => {
  test('prefers the meeting moment over the row creation time', () => {
    // Both are given, and they land on different days on purpose.
    expect(legacySummaryExportDate({
      created_at: '2026-11-19T09:00:00+00:00',
      recorded_at: '2026-11-18T14:32:05+00:00',
    })).toBe(new Date(2026, 10, 18).toLocaleDateString());
  });

  test('falls back to created_at when the meeting moment is unknown', () => {
    // An export needs *a* date, so the audit field is the documented fallback.
    expect(legacySummaryExportDate({
      created_at: '2026-11-19T09:00:00+00:00',
      recorded_at: null,
    })).toBe(new Date(2026, 10, 19).toLocaleDateString());

    expect(legacySummaryExportDate({
      created_at: '2026-11-19T09:00:00+00:00',
    })).toBe(new Date(2026, 10, 19).toLocaleDateString());
  });

  test('with no date at all it still renders a date, as the export always did', () => {
    const rendered = legacySummaryExportDate(undefined);
    expect(rendered).toBe(new Date().toLocaleDateString());
  });
});
