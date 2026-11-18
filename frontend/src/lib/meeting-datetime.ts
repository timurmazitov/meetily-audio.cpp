/**
 * Rendering a meeting's own moment and length for the interface.
 *
 * These are display formatters, so they differ from `resolveMeetingDate` in one
 * important way: a missing value renders as nothing. `resolveMeetingDate`
 * substitutes the current moment, which is correct for names and exports that
 * need *a* date, but for the list it would be a lie: an unknown meeting date
 * would look exactly like a meeting that just happened.
 */

/** A date-only value: year, month and day with no time or zone. */
const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * `18/11/2026, 14:32` in local time, with zero-padded 24-hour parts.
 *
 * A date-only value has no time of day, so it renders the date alone rather
 * than a midnight that was never recorded. No usable value renders `null`.
 */
export function formatMeetingDateTime(
  value: string | Date | number | null | undefined,
): string | null {
  if (value === null || value === undefined) return null;

  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return null;

    const dateOnly = parseDateOnly(trimmed);
    if (dateOnly) return formatDate(dateOnly);

    return formatInstant(new Date(trimmed));
  }

  return formatInstant(value instanceof Date ? value : new Date(value));
}

/**
 * `42 min`, `1 h 23 min`, `< 1 min`.
 *
 * A missing, zero or non-positive length has no display form: `0 min` would
 * read as a real measurement of a meeting that never happened.
 */
export function formatMeetingDuration(seconds: number | null | undefined): string | null {
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds <= 0) {
    return null;
  }

  if (seconds < 60) return '< 1 min';

  const totalMinutes = Math.round(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours === 0) return `${minutes} min`;
  if (minutes === 0) return `${hours} h`;
  return `${hours} h ${minutes} min`;
}

function formatInstant(moment: Date): string | null {
  if (Number.isNaN(moment.getTime())) return null;
  return `${formatDate(moment)}, ${pad(moment.getHours())}:${pad(moment.getMinutes())}`;
}

function formatDate(moment: Date): string {
  return `${pad(moment.getDate())}/${pad(moment.getMonth() + 1)}/${moment.getFullYear()}`;
}

function pad(value: number): string {
  return String(value).padStart(2, '0');
}

/** Parsed local day, or null when the value is not a real calendar day. */
function parseDateOnly(value: string): Date | null {
  const match = DATE_ONLY.exec(value);
  if (!match) return null;

  const [, year, month, day] = match;
  const local = new Date(Number(year), Number(month) - 1, Number(day));

  // `new Date(2026, 1, 31)` rolls over to March 3, so a mismatching round trip
  // means the input named a day that does not exist.
  if (
    local.getFullYear() !== Number(year)
    || local.getMonth() !== Number(month) - 1
    || local.getDate() !== Number(day)
  ) {
    return null;
  }

  return local;
}
