import { describe, expect, test } from 'bun:test';

import {
  formatMeetingDateTime,
  formatMeetingDuration,
  fromDateTimeInputValue,
  toDateTimeInputValue,
} from '../../src/lib/meeting-datetime';

describe('formatMeetingDateTime', () => {
  test('renders a local date and a zero-padded 24-hour time', () => {
    // Local values: the expected literal is stable in any time zone.
    expect(formatMeetingDateTime(new Date(2026, 10, 18, 14, 32))).toBe('18/11/2026, 14:32');
    expect(formatMeetingDateTime(new Date(2026, 0, 1, 9, 5))).toBe('01/01/2026, 09:05');
    expect(formatMeetingDateTime(new Date(2026, 11, 31, 0, 0))).toBe('31/12/2026, 00:00');
    expect(formatMeetingDateTime(new Date(2026, 0, 2, 23, 59))).toBe('02/01/2026, 23:59');
  });

  test('accepts the RFC3339 string the list command sends', () => {
    const moment = new Date(2026, 10, 18, 14, 32);
    expect(formatMeetingDateTime(moment.toISOString())).toBe('18/11/2026, 14:32');
  });

  test('a date-only value renders the date without inventing a time', () => {
    expect(formatMeetingDateTime('2026-11-18')).toBe('18/11/2026');
    expect(formatMeetingDateTime('2026-01-02')).toBe('02/01/2026');
  });

  test('no usable value renders nothing at all', () => {
    expect(formatMeetingDateTime(null)).toBeNull();
    expect(formatMeetingDateTime(undefined)).toBeNull();
    expect(formatMeetingDateTime('')).toBeNull();
    expect(formatMeetingDateTime('   ')).toBeNull();
    expect(formatMeetingDateTime('not a date')).toBeNull();
    expect(formatMeetingDateTime(new Date('not a date'))).toBeNull();
  });
});

describe('formatMeetingDuration', () => {
  test('whole minutes are the common case', () => {
    expect(formatMeetingDuration(42 * 60)).toBe('42 min');
    expect(formatMeetingDuration(60)).toBe('1 min');
    expect(formatMeetingDuration(89)).toBe('1 min');
    expect(formatMeetingDuration(90)).toBe('2 min');
  });

  test('an hour or more gains an hours part and drops the minutes when they are zero', () => {
    expect(formatMeetingDuration(83 * 60)).toBe('1 h 23 min');
    expect(formatMeetingDuration(3600)).toBe('1 h');
    expect(formatMeetingDuration(2 * 3600 + 5 * 60)).toBe('2 h 5 min');
  });

  test('anything shorter than a minute is reported as such, never as zero', () => {
    expect(formatMeetingDuration(1)).toBe('< 1 min');
    expect(formatMeetingDuration(59)).toBe('< 1 min');
    expect(formatMeetingDuration(0.4)).toBe('< 1 min');
  });

  test('no usable value renders nothing at all', () => {
    expect(formatMeetingDuration(null)).toBeNull();
    expect(formatMeetingDuration(undefined)).toBeNull();
    // Zero is "unknown" for a meeting, never a real length.
    expect(formatMeetingDuration(0)).toBeNull();
    expect(formatMeetingDuration(-5)).toBeNull();
    expect(formatMeetingDuration(Number.NaN)).toBeNull();
    expect(formatMeetingDuration(Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe('toDateTimeInputValue', () => {
  test('renders the local wall clock the way datetime-local expects', () => {
    expect(toDateTimeInputValue(new Date(2026, 10, 18, 14, 32, 45))).toBe('2026-11-18T14:32');
    expect(toDateTimeInputValue(new Date(2026, 0, 1, 9, 5))).toBe('2026-01-01T09:05');
    expect(toDateTimeInputValue(new Date(2026, 11, 31, 0, 0))).toBe('2026-12-31T00:00');
  });

  test('accepts the RFC3339 string the database stores', () => {
    const moment = new Date(2026, 10, 18, 14, 32);
    expect(toDateTimeInputValue(moment.toISOString())).toBe('2026-11-18T14:32');
  });

  test('a date-only value becomes midnight, not a rejected field', () => {
    expect(toDateTimeInputValue('2026-11-18')).toBe('2026-11-18T00:00');
  });

  test('an unknown date leaves the field empty instead of inventing one', () => {
    expect(toDateTimeInputValue(null)).toBe('');
    expect(toDateTimeInputValue(undefined)).toBe('');
    expect(toDateTimeInputValue('')).toBe('');
    expect(toDateTimeInputValue('   ')).toBe('');
    expect(toDateTimeInputValue('not a date')).toBe('');
  });
});

describe('fromDateTimeInputValue', () => {
  test('round-trips a local moment through the input value', () => {
    const moment = new Date(2026, 10, 18, 14, 32);
    expect(fromDateTimeInputValue('2026-11-18T14:32')).toBe(moment.toISOString());
    expect(toDateTimeInputValue(fromDateTimeInputValue('2026-11-18T14:32'))).toBe('2026-11-18T14:32');
  });

  test('seconds are always zero: the date is edited to the minute', () => {
    const stored = fromDateTimeInputValue('2026-11-18T14:32')!;
    expect(new Date(stored).getSeconds()).toBe(0);
    expect(new Date(stored).getMilliseconds()).toBe(0);
  });

  test('an empty field means "no date", which is not an instant', () => {
    expect(fromDateTimeInputValue('')).toBeNull();
    expect(fromDateTimeInputValue('   ')).toBeNull();
  });

  test('a day that does not exist or an impossible time is rejected, not rolled over', () => {
    expect(fromDateTimeInputValue('2026-02-31T10:00')).toBeNull();
    expect(fromDateTimeInputValue('2026-13-01T10:00')).toBeNull();
    expect(fromDateTimeInputValue('2026-11-18T25:00')).toBeNull();
    expect(fromDateTimeInputValue('2026-11-18T10:61')).toBeNull();
  });

  test('a value that is not a datetime-local string is rejected', () => {
    expect(fromDateTimeInputValue('18/11/2026, 14:32')).toBeNull();
    expect(fromDateTimeInputValue('not a date')).toBeNull();
  });
});
