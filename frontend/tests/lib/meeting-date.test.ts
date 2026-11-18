import { describe, expect, test } from 'bun:test';

import { parseDateOnly } from '../../src/lib/meeting-date';

// Local values: the expected literals are stable in any time zone.
describe('parseDateOnly', () => {
  test('a real calendar day becomes that local midnight', () => {
    const parsed = parseDateOnly('2026-11-18');
    expect(parsed).not.toBeNull();
    expect(parsed!.getFullYear()).toBe(2026);
    expect(parsed!.getMonth()).toBe(10);
    expect(parsed!.getDate()).toBe(18);
    expect(parsed!.getHours()).toBe(0);
    expect(parsed!.getMinutes()).toBe(0);
  });

  test('a day that does not exist is rejected instead of rolling over', () => {
    // `new Date(2026, 1, 31)` rolls over to March 3; February has no 31st.
    expect(parseDateOnly('2026-02-31')).toBeNull();
    expect(parseDateOnly('2026-13-01')).toBeNull();
    expect(parseDateOnly('2026-04-31')).toBeNull();
  });

  test('anything that is not a bare YYYY-MM-DD value is not a date-only value', () => {
    expect(parseDateOnly('18.11.2026')).toBeNull();
    expect(parseDateOnly('2026-11-18T14:32:05Z')).toBeNull();
    expect(parseDateOnly('2026-11-18 14:32')).toBeNull();
    expect(parseDateOnly('')).toBeNull();
  });
});
