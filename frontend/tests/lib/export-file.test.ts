import { afterEach, describe, expect, setSystemTime, test } from 'bun:test';

import { buildExportFileName, ensureFileExtension } from '../../src/lib/export-file';

const localDate = (year: number, month: number, day: number, hour = 12) =>
  new Date(year, month - 1, day, hour);

// Strip the date prefix, suffix and extension so a test can assert on the
// normalized meeting title alone.
function titlePart(fileName: string, suffix: string, extension: string): string {
  const withoutDate = fileName.replace(/^\d{8}-/, '');
  return withoutDate.replace(new RegExp(`-${suffix}\\.${extension}$`), '');
}

describe('buildExportFileName', () => {
  afterEach(() => {
    setSystemTime();
  });

  test('builds YYYYMMDD-title-transcript.txt from the meeting date', () => {
    expect(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Release planning',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20261118-Release_planning-transcript.txt');
  });

  test('supports the summary suffix and extension', () => {
    expect(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Планёрка по релизу',
      suffix: 'summary',
      extension: 'md',
    })).toBe('20261118-Планёрка_по_релизу-summary.md');
  });

  test('takes the date from the local calendar day, not from UTC', () => {
    expect(buildExportFileName({
      date: localDate(2026, 1, 2, 23),
      title: 'Late meeting',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20260102-Late_meeting-transcript.txt');
  });

  test('accepts a created_at timestamp string', () => {
    const createdAt = localDate(2026, 9, 1, 9).toISOString();
    expect(buildExportFileName({
      date: createdAt,
      title: 'Kickoff',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20260901-Kickoff-transcript.txt');
  });

  test('accepts a date-only created_at value as that local day', () => {
    expect(buildExportFileName({
      date: '2026-09-01',
      title: 'Kickoff',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20260901-Kickoff-transcript.txt');
  });

  test('falls back to the current moment for a missing or invalid date', () => {
    setSystemTime(localDate(2026, 11, 18, 10));

    for (const date of [null, undefined, '', 'not-a-date']) {
      expect(buildExportFileName({
        date,
        title: 'Release planning',
        suffix: 'transcript',
        extension: 'txt',
      })).toBe('20261118-Release_planning-transcript.txt');
    }
  });

  test('keeps Cyrillic and letter case readable', () => {
    expect(titlePart(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Планёрка Продукта',
      suffix: 'transcript',
      extension: 'txt',
    }), 'transcript', 'txt')).toBe('Планёрка_Продукта');
  });

  test('turns spaces into single underscores', () => {
    expect(titlePart(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: '  Release    planning   meeting  ',
      suffix: 'transcript',
      extension: 'txt',
    }), 'transcript', 'txt')).toBe('Release_planning_meeting');
  });

  test('replaces characters that file systems reject', () => {
    const fileName = buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'a/b\\c:d*e?f"g<h>i|j',
      suffix: 'transcript',
      extension: 'txt',
    });

    expect(titlePart(fileName, 'transcript', 'txt')).toBe('a_b_c_d_e_f_g_h_i_j');
    expect(fileName).not.toMatch(/[\\/:*?"<>|]/);
  });

  test('replaces newlines and tabs without leaving empty stretches', () => {
    expect(titlePart(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Line one\nLine\ttwo\r\n\r\nLine three',
      suffix: 'transcript',
      extension: 'txt',
    }), 'transcript', 'txt')).toBe('Line_one_Line_two_Line_three');
  });

  test('collapses repeated and edge underscores', () => {
    expect(titlePart(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: '___Release // planning ___',
      suffix: 'transcript',
      extension: 'txt',
    }), 'transcript', 'txt')).toBe('Release_planning');
  });

  test('truncates a long Cyrillic title to 60 characters', () => {
    expect(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Очень длинное название встречи про планирование релиза номер один',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe(
      '20261118-Очень_длинное_название_встречи_про_планирование_релиза_номер-transcript.txt',
    );
  });

  test('truncates by character boundary, keeping surrogate pairs intact', () => {
    const fileName = buildExportFileName({
      date: localDate(2026, 11, 18),
      title: `a${'😀'.repeat(70)}`,
      suffix: 'transcript',
      extension: 'txt',
    });

    expect(titlePart(fileName, 'transcript', 'txt')).toBe(`a${'😀'.repeat(59)}`);
    expect(fileName.isWellFormed()).toBe(true);
  });

  test('does not end a truncated title with an underscore', () => {
    expect(titlePart(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'слово '.repeat(12),
      suffix: 'transcript',
      extension: 'txt',
    }), 'transcript', 'txt')).toBe(
      'слово_слово_слово_слово_слово_слово_слово_слово_слово_слово',
    );
  });

  test('falls back to meeting for an empty or blank title', () => {
    for (const title of ['', '   ', '\n\t', null, undefined]) {
      expect(buildExportFileName({
        date: localDate(2026, 11, 18),
        title,
        suffix: 'transcript',
        extension: 'txt',
      })).toBe('20261118-meeting-transcript.txt');
    }
  });

  test('falls back to meeting for a title made only of separators', () => {
    expect(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: ' /// ',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20261118-meeting-transcript.txt');
  });

  test('falls back to meeting for the new-call placeholder title', () => {
    expect(buildExportFileName({
      date: localDate(2026, 11, 18),
      title: '+ New Call',
      suffix: 'transcript',
      extension: 'txt',
    })).toBe('20261118-meeting-transcript.txt');
  });

  test('never produces characters that file systems reject', () => {
    const fileName = buildExportFileName({
      date: localDate(2026, 11, 18),
      title: 'Bad / name \\ with : everything * ? " < > | here',
      suffix: 'transcript',
      extension: 'txt',
    });

    expect(fileName).not.toMatch(/[\\/:*?"<>|]/);
    expect(fileName).not.toMatch(/[\n\r\t]/);
    expect(fileName).toMatch(/^\d{8}-[^-]+-transcript\.txt$/u);
  });
});

describe('ensureFileExtension', () => {
  test('appends the extension when the name has none', () => {
    expect(ensureFileExtension('20261118-Release_planning-transcript', 'txt'))
      .toBe('20261118-Release_planning-transcript.txt');
  });

  test('accepts an extension written with a leading dot', () => {
    expect(ensureFileExtension('20261118-Release_planning-summary', '.md'))
      .toBe('20261118-Release_planning-summary.md');
  });

  test('keeps an extension the name already has', () => {
    expect(ensureFileExtension('20261118-Release_planning-transcript.txt', 'txt'))
      .toBe('20261118-Release_planning-transcript.txt');
  });

  test('does not replace an extension the user typed', () => {
    expect(ensureFileExtension('20261118-Release_planning-summary.markdown', 'md'))
      .toBe('20261118-Release_planning-summary.markdown');
  });

  test('keeps an existing extension written in capitals', () => {
    expect(ensureFileExtension('20261118-Release_planning-summary.MD', 'md'))
      .toBe('20261118-Release_planning-summary.MD');
  });

  test('is a no-op for an empty extension', () => {
    expect(ensureFileExtension('20261118-Release_planning-transcript', ''))
      .toBe('20261118-Release_planning-transcript');
  });
});
