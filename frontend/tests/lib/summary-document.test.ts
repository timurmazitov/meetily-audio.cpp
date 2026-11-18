import { afterEach, describe, expect, setSystemTime, test } from 'bun:test';
import {
  buildSummaryDocument,
  convertLegacySummaryToMarkdown,
} from '../../src/lib/summary-document';
import { buildExportFileName } from '../../src/lib/export-file';

// Local dates: `toLocaleDateString('en-US', …)` renders them identically in any
// timezone, so the expected literals below are stable.
const meetingDate = new Date(2026, 0, 15, 14, 5);
const copiedAt = new Date(2026, 10, 18, 9, 3);
const summaryMarkdown = '# Overview\n\nShipped the release.';

describe('summary document', () => {
  test('builds the copied document byte-for-byte', () => {
    expect(buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate,
      label: 'Copied',
      timestamp: copiedAt,
    })).toBe(
      '# Meeting Summary: Release planning\n\n'
      + '**Meeting ID:** meeting-42\n'
      + '**Date:** January 15, 2026 at 02:05 PM\n'
      + '**Copied on:** November 18, 2026 at 09:03 AM\n'
      + '\n---\n\n'
      + summaryMarkdown,
    );
  });

  test('the file variant differs from the clipboard variant only in the label line', () => {
    const base = {
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate,
      timestamp: copiedAt,
    };
    const copied = buildSummaryDocument({ ...base, label: 'Copied' });
    const exported = buildSummaryDocument({ ...base, label: 'Exported' });

    const stripLabelLine = (document: string) => document
      .split('\n')
      .filter(line => !/^\*\*(Copied|Exported) on:\*\* /.test(line))
      .join('\n');

    expect(stripLabelLine(copied)).toBe(stripLabelLine(exported));
    expect(copied).not.toBe(exported);
    expect(exported).toContain('**Exported on:** November 18, 2026 at 09:03 AM\n');
    expect(exported).not.toContain('**Copied on:**');
  });

  test('writes an ISO meeting date as the meeting date in the metadata block', () => {
    expect(buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate: '2026-01-15T14:05:00',
      label: 'Copied',
      timestamp: copiedAt,
    })).toContain('**Date:** January 15, 2026 at 02:05 PM\n');
  });

  test('reads a date-only meeting date as that local calendar day', () => {
    // A meeting date may be stored as a bare `YYYY-MM-DD`. Parsing it as UTC
    // midnight would make the metadata block name the previous day west of
    // Greenwich while the file name uses the local day.
    expect(buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate: '2026-11-18',
      label: 'Exported',
      timestamp: copiedAt,
    })).toContain('**Date:** November 18, 2026');
  });
});

describe('the document and the export file name name the same day', () => {
  afterEach(() => {
    setSystemTime();
  });

  /** The `**Date:**` line of a summary document. */
  const dateLine = (document: string) => document
    .split('\n')
    .find(line => line.startsWith('**Date:**'));

  test('an instant meeting date dates both the document and the file name', () => {
    const meetingDate = '2026-10-02T09:15:00';

    // The row was created on 18 November; the meeting happened on 2 October.
    // Both surfaces read one value, so they cannot name different days.
    const document = buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate,
      label: 'Exported',
      timestamp: copiedAt,
    });
    const fileName = buildExportFileName({
      date: meetingDate,
      title: 'Release planning',
      kind: 'summary',
    });

    expect(dateLine(document)).toBe('**Date:** October 2, 2026 at 09:15 AM');
    expect(fileName).toBe('20261002-Release_planning-summary.md');
  });

  test('a date-only meeting date dates both the document and the file name', () => {
    const meetingDate = '2026-10-02';

    const document = buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      meetingDate,
      label: 'Exported',
      timestamp: copiedAt,
    });
    const fileName = buildExportFileName({
      date: meetingDate,
      title: 'Release planning',
      kind: 'summary',
    });

    // A date-only value has no time of day; the metadata block's accepted
    // format still prints the midnight it resolves to. The date must not change.
    expect(dateLine(document)).toBe('**Date:** October 2, 2026 at 12:00 AM');
    expect(fileName).toBe('20261002-Release_planning-summary.md');
  });

  test('a meeting without a date falls back to the same current moment in both', () => {
    setSystemTime(new Date(2026, 10, 18, 10, 30));

    const document = buildSummaryDocument({
      markdown: summaryMarkdown,
      meetingTitle: 'Release planning',
      meetingId: 'meeting-42',
      label: 'Exported',
      timestamp: copiedAt,
    });
    const fileName = buildExportFileName({
      date: undefined,
      title: 'Release planning',
      kind: 'summary',
    });

    expect(dateLine(document)).toBe('**Date:** November 18, 2026 at 10:30 AM');
    expect(fileName).toBe('20261118-Release_planning-summary.md');
  });
});

describe('legacy summary conversion', () => {
  test('turns section blocks into markdown headings and bullets', () => {
    expect(convertLegacySummaryToMarkdown({
      MeetingName: 'Release planning',
      _section_order: ['Decisions'],
      Decisions: { title: 'Decisions', blocks: [{ content: 'Ship it' }, { content: 'Wait a week' }] },
      Risks: { title: 'Risks', blocks: [{ content: 'None' }] },
      notASection: 'plain string',
    })).toBe('## Decisions\n\n- Ship it\n- Wait a week\n\n## Risks\n\n- None');
  });

  test('returns an empty string when there is no legacy section content', () => {
    expect(convertLegacySummaryToMarkdown({ MeetingName: 'Release planning' })).toBe('');
    expect(convertLegacySummaryToMarkdown({})).toBe('');
  });
});
