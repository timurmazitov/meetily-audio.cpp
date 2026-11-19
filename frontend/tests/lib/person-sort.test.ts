import { describe, expect, test } from 'bun:test';

import { comparePersonNames, foldPersonName, personNameKey } from '../../src/lib/person-sort';

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

  test('a letter outside the Russian alphabet still lands in the Cyrillic group, after it', () => {
    // `Ї` is Cyrillic but not Russian, so it has no place in the familiar
    // alphabet. It still belongs to group 0, and its position is fixed by the
    // rule rather than by whatever collation data the host happens to carry.
    expect(sorted(['яна', 'Їжа', 'Анна'])).toEqual(['Анна', 'яна', 'Їжа']);
    expect(sorted(['Їжа', 'Анна', 'яна'])).toEqual(['Анна', 'яна', 'Їжа']);
  });

  test('every letter of the Russian alphabet keeps its familiar slot', () => {
    // The alphabet itself is the source of truth here: Ё after Е, Й after И,
    // and the whole list in the order a Russian speaker recites it.
    const alphabet = 'абвгдеёжзийклмнопрстуфхцчшщъыьэюя'.split('');
    const shuffled = [...alphabet].reverse().map(letter => letter.toUpperCase() + 'ина');

    expect(sorted(shuffled).map(name => name.charAt(0).toLowerCase())).toEqual(alphabet);
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

describe('foldPersonName', () => {
  test('is the one case fold for names: lower-cased, nothing else', () => {
    expect(foldPersonName('Анна')).toBe('анна');
    expect(foldPersonName('ALEX')).toBe('alex');
    expect(foldPersonName('  Anna')).toBe('  anna');
  });
});

describe('personNameKey', () => {
  test('trims, folds, and treats blank as absent', () => {
    expect(personNameKey('  Anna ')).toBe('anna');
    expect(personNameKey('ЁЖИК')).toBe('ёжик');
    expect(personNameKey('   ')).toBeNull();
    expect(personNameKey('')).toBeNull();
    expect(personNameKey(null)).toBeNull();
    expect(personNameKey(undefined)).toBeNull();
  });

  test('names that differ only by case and spacing share one key', () => {
    expect(personNameKey(' Alex ')).toBe(personNameKey('alex'));
  });
});
