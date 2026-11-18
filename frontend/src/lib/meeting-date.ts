/**
 * Reading a meeting's date, shared by the export file name, the summary
 * metadata block and the transcript header.
 *
 * The file name and the document must always name the same day, so all three go
 * through `resolveMeetingDate`. The value read is the meeting's own moment
 * (GLOSSARY: «дата встречи», `recorded_at`), never the row's `created_at`: that
 * is an audit field naming when the row appeared in the database — for a
 * recording, after the meeting had already ended. A bare `YYYY-MM-DD` value is a
 * calendar day, not an instant: it is read as that *local* day so the time zone
 * cannot shift it. Anything else is a normal `Date`-parsable instant.
 */

/** A date-only value: year, month and day with no time or zone. */
const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * A bare `YYYY-MM-DD` value read as that *local* calendar day, or `null` when
 * the value is not a date-only string or names a day that does not exist.
 *
 * This is the one home of the date-only rule: `meeting-datetime.ts` uses it to
 * decide that a value has no time of day to render.
 */
export function parseDateOnly(value: string): Date | null {
  const match = DATE_ONLY.exec(value);
  if (!match) return null;

  const [, year, month, day] = match;
  const local = new Date(Number(year), Number(month) - 1, Number(day));

  // `new Date(2026, 1, 31)` rolls over to March 3 and `new Date(2026, 12, 1)`
  // to January 1 of the next year, so a mismatching round trip means the input
  // named a day that does not exist.
  if (
    local.getFullYear() !== Number(year)
    || local.getMonth() !== Number(month) - 1
    || local.getDate() !== Number(day)
  ) {
    return null;
  }

  return local;
}

/**
 * Parse a meeting moment, falling back to the current moment when it is
 * missing, unparsable, or names a calendar day that does not exist.
 */
export function resolveMeetingDate(value?: string | Date | number | null): Date {
  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? new Date() : value;
  }

  if (typeof value === 'number') {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
  }

  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (trimmed) {
      return parseDateString(trimmed) ?? new Date();
    }
  }

  return new Date();
}

/** Parsed value, or null when it is not a real calendar day. */
function parseDateString(value: string): Date | null {
  if (DATE_ONLY.test(value)) {
    // A date-only value is never handed to `new Date`, which would roll an
    // impossible day over into the next month instead of rejecting it.
    return parseDateOnly(value);
  }

  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}
