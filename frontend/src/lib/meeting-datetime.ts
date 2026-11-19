/**
 * Rendering a meeting's own moment and length for the interface.
 *
 * These are display formatters, so they differ from `resolveMeetingDate` in one
 * important way: a missing value renders as nothing. `resolveMeetingDate`
 * substitutes the current moment, which is correct for names and exports that
 * need *a* date, but for the list it would be a lie: an unknown meeting date
 * would look exactly like a meeting that just happened.
 *
 * The two conversion helpers below are the edit dialog's bridge between the
 * stored moment and the `datetime-local` field, and they follow the same rule:
 * an unknown date is the empty field, never "now".
 */

import { parseDateOnly } from '@/lib/meeting-date';

/**
 * The value a `datetime-local` input expects: `2026-11-18T14:32` in local time.
 *
 * A date-only value has no time of day, so it becomes midnight; nothing else is
 * invented. No usable value renders an empty string, which is how the input
 * shows "no date" while the meeting date stays unknown (ADR-0005).
 */
export function toDateTimeInputValue(
  value: string | Date | number | null | undefined,
): string {
  const moment = parseMoment(value);
  if (!moment) return '';

  const day = `${moment.getFullYear()}-${pad(moment.getMonth() + 1)}-${pad(moment.getDate())}`;
  return `${day}T${pad(moment.getHours())}:${pad(moment.getMinutes())}`;
}

/**
 * The RFC3339 instant behind a `datetime-local` value, or `null` when the field
 * is empty or does not name a real local moment.
 *
 * Seconds are not editable, so they are always zero: the meeting date is
 * edited to the minute.
 */
export function fromDateTimeInputValue(value: string): string | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(value.trim());
  if (!match) return null;

  const [, year, month, day, hours, minutes] = match;
  const local = new Date(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hours),
    Number(minutes),
  );

  // A rolled-over date (`2026-02-31T10:00`) is not a real moment, and an
  // out-of-range time (`25:00`) is rejected instead of being normalised.
  if (
    Number.isNaN(local.getTime())
    || local.getFullYear() !== Number(year)
    || local.getMonth() !== Number(month) - 1
    || local.getDate() !== Number(day)
    || local.getHours() !== Number(hours)
    || local.getMinutes() !== Number(minutes)
  ) {
    return null;
  }

  return local.toISOString();
}

/** The moment a stored value names, or `null` when it names none. */
function parseMoment(
  value: string | Date | number | null | undefined,
): Date | null {
  if (value === null || value === undefined) return null;

  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? null : value;
  }

  if (typeof value === 'number') {
    const moment = new Date(value);
    return Number.isNaN(moment.getTime()) ? null : moment;
  }

  const trimmed = value.trim();
  if (!trimmed) return null;

  const dateOnly = parseDateOnly(trimmed);
  if (dateOnly) return dateOnly;

  const moment = new Date(trimmed);
  return Number.isNaN(moment.getTime()) ? null : moment;
}

/**
 * `18/11/2026, 14:32` in local time, with zero-padded 24-hour parts.
 *
 * A date-only value has no time of day, so it renders the date alone rather
 * than a midnight that was never recorded. No usable value renders `null`.
 */
export function formatMeetingDateTime(
  value: string | Date | number | null | undefined,
): string | null {
  if (typeof value === 'string') {
    const dateOnly = parseDateOnly(value.trim());
    if (dateOnly) return formatDate(dateOnly);
  }

  const moment = parseMoment(value);
  return moment ? formatInstant(moment) : null;
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
