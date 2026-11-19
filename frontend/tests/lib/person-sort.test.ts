import { describe, expect, test } from 'bun:test';

import { comparePersonNames } from '../../src/lib/person-sort';

// The expectations are the rule from the spec, not a second copy of the
// implementation: Cyrillic А-Я first, then Latin A-Z, then everything else,
// case-insensitively, with the original spelling as the deterministic
// tie-breaker.
const sorted = (names: string[]) => [...names].sort(comparePersonNames);

describe('comparePersonNames', () => {
  test('Cyrillic names come before Latin ones', () => {
    expect(sorted(['Zoe', 'Мария'])).toEqual(['Мария', 'Zoe']);
  });

  test('names inside one group stay alphabetical, by the familiar alphabet', () => {
    // `Ё` belongs after `Е` in the Russian alphabet, not at the end of the
    // code-point range where `U+0451` sits.
    expect(sorted(['яна', 'Анна', 'ёжик', 'Борис', 'Егор', 'elif', 'Aziz'])).toEqual([
      'Анна',
      'Борис',
      'Егор',
      'ёжик',
      'яна',
      'Aziz',
      'elif',
    ]);
  });

  test('case does not split the order: names differing only by case stand together', () => {
    const names = ['анна', 'Анна'];
    const result = sorted(names);

    expect(result).toHaveLength(2);
    expect(result.map(name => name.toLowerCase())).toEqual(['анна', 'анна']);
    // The tie-breaker is the original spelling, so the order is fixed.
    expect(result).toEqual(['Анна', 'анна']);
  });

  test('digits and other alphabets come after both letter groups', () => {
    expect(sorted(['Мария', '2B', 'anna', '_x', 'Иван', 'Zoe'])).toEqual([
      'Иван',
      'Мария',
      'anna',
      'Zoe',
      '2B',
      '_x',
    ]);
  });

  test('a mixed list sorts the same whatever the input order', () => {
    const names = ['Мария', 'Zoe', 'анна', 'Alex', 'Иван', '2B'];
    const expected = sorted(names);

    expect(sorted([...names].reverse())).toEqual(expected);
    expect(sorted([...names].sort())).toEqual(expected);
  });

  test('comparison is antisymmetric and self-comparison is stable', () => {
    expect(comparePersonNames('Анна', 'Анна')).toBe(0);
    expect(comparePersonNames('Анна', 'Zoe')).toBeLessThan(0);
    expect(comparePersonNames('Zoe', 'Анна')).toBeGreaterThan(0);
  });
});
