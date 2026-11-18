import { resolveMeetingDate } from '@/lib/meeting-date';

/** The action word in the metadata label line (`**Copied on:**` / `**Exported on:**`). */
export type SummaryDocumentLabel = 'Copied' | 'Exported';

export interface SummaryDocumentInput {
  /** Markdown body of the summary, appended verbatim after the separator. */
  markdown: string;
  /** Meeting title shown in the `# Meeting Summary:` heading. */
  meetingTitle: string;
  /** Meeting ID shown in the metadata block. */
  meetingId: string;
  /** Meeting moment; an ISO string or `Date` (as stored for `created_at`). */
  meetingDate: string | Date | number;
  /** Action word of the label line — the only difference between copy and export. */
  label: SummaryDocumentLabel;
  /** Moment written next to the label; defaults to now. */
  timestamp?: string | Date | number;
}

/** Keys that are summary metadata or BlockNote payload, never legacy sections. */
const NON_SECTION_KEYS = ['markdown', 'summary_json', '_section_order', 'MeetingName'];

/**
 * Format a moment the way the summary metadata block always has:
 * `January 15, 2026 at 02:05 PM` (English, local time).
 */
function formatMetadataDate(value: string | Date | number): string {
  return new Date(value).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/**
 * Format the meeting date in the metadata block, reading it through the same
 * helper as the export file name so the two can never name different days.
 */
function formatMeetingDate(value: string | Date | number): string {
  return formatMetadataDate(resolveMeetingDate(value));
}

/**
 * Build the complete summary document shared by Copy and Export.
 *
 * Layout: `# Meeting Summary: <title>`, blank line, the `**Meeting ID:**`,
 * `**Date:**` and `**<label> on:**` lines, blank line, `---`, blank line, then
 * the summary markdown verbatim. Copy and Export differ only in the label word,
 * so the clipboard and the file can never drift apart.
 */
export function buildSummaryDocument({
  markdown,
  meetingTitle,
  meetingId,
  meetingDate,
  label,
  timestamp = new Date(),
}: SummaryDocumentInput): string {
  const header = `# Meeting Summary: ${meetingTitle}\n\n`;
  const metadata = `**Meeting ID:** ${meetingId}\n`
    + `**Date:** ${formatMeetingDate(meetingDate)}\n`
    + `**${label} on:** ${formatMetadataDate(timestamp)}\n\n---\n\n`;

  return header + metadata + markdown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isLegacySection(value: unknown): value is { title: unknown; blocks: unknown[] } {
  return isRecord(value) && 'title' in value && Array.isArray(value.blocks);
}

/**
 * Convert a legacy summary (sections of `{ title, blocks }`) to markdown.
 *
 * Each section becomes a `## <title>` heading followed by one `- <content>`
 * bullet per block; sections are separated by a blank line. Entries that are
 * not sections are skipped, and a summary without legible content yields an
 * empty string — the same rule the copy path always applied.
 */
export function convertLegacySummaryToMarkdown(summary: unknown): string {
  if (!isRecord(summary)) {
    return '';
  }

  return Object.entries(summary)
    .filter(([key]) => !NON_SECTION_KEYS.includes(key))
    .map(([, section]) => {
      if (!isLegacySection(section)) {
        return '';
      }
      const sectionTitle = `## ${section.title}\n\n`;
      const sectionContent = section.blocks
        .map(block => `- ${isRecord(block) ? block.content : undefined}`)
        .join('\n');
      return sectionTitle + sectionContent;
    })
    .filter(section => section.trim())
    .join('\n\n');
}
