import { useCallback, RefObject } from 'react';
import { MeetingSummary, Transcript } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { toast } from 'sonner';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { hasVisibleSummaryContent } from '@/lib/summary-content';
import {
  buildSummaryDocument,
  convertLegacySummaryToMarkdown,
  type SummaryDocumentLabel,
} from '@/lib/summary-document';
import { formatTranscriptText } from '@/lib/transcript-format';
import { resolveMeetingDate } from '@/lib/meeting-date';
import { buildExportFileName, exportFileWithDialog } from '@/lib/export-file';
import { useAppLanguage } from '@/contexts/LanguageContext';

interface UseCopyOperationsProps {
  meeting: any;
  transcripts: Transcript[];
  meetingTitle: string;
  aiSummary: MeetingSummary | null;
  blockNoteSummaryRef: RefObject<BlockNoteSummaryViewRef>;
}

export function useCopyOperations({
  meeting,
  transcripts,
  meetingTitle,
  aiSummary,
  blockNoteSummaryRef,
}: UseCopyOperationsProps) {
  const { language } = useAppLanguage();

  // The meeting title the user is looking at, falling back to the saved meeting
  // name. Both exports and both file names go through it, so Copy, Export and
  // the saved file always agree on what the meeting is called.
  const exportTitle: string = meetingTitle ?? meeting.title;

  // The meeting's own moment (GLOSSARY: «дата встречи»), read from the row's
  // `recorded_at`. The row's `created_at` is the audit field describing when the
  // row appeared in the database — for a recording that is after the meeting
  // ended — so it must never date a document. Both documents and both file names
  // read this one value, so the copied text, the saved file and the panel header
  // name the same day. A meeting without a date falls through to
  // `resolveMeetingDate`'s "now", the long-standing export fallback: an export
  // needs *a* date, and it is only reached for rows stocked before the date was
  // recorded at all.
  const meetingDate: string | undefined = meeting.recorded_at ?? undefined;

  // Helper function to fetch ALL transcripts for copying (not just paginated data)
  const fetchAllTranscripts = useCallback(async (meetingId: string): Promise<Transcript[]> => {
    try {
      console.log('📊 Fetching all transcripts for copying:', meetingId);

      // First, get total count by fetching first page
      const firstPage = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: 1,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      const totalCount = firstPage.total_count;
      console.log(`📊 Total transcripts in database: ${totalCount}`);

      if (totalCount === 0) {
        return [];
      }

      // Fetch all transcripts in one call
      const allData = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: totalCount,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      console.log(`✅ Fetched ${allData.transcripts.length} transcripts from database for copying`);
      return allData.transcripts;
    } catch (error) {
      console.error('❌ Error fetching all transcripts:', error);
      toast.error('Failed to fetch transcripts for copying');
      return [];
    }
  }, []);

  // Build the full transcript text (header + segments) for copy/export.
  // Shared so the Export button saves exactly what Copy puts on the clipboard.
  const buildTranscriptText = useCallback(async (): Promise<string | null> => {
    console.log('📊 Fetching all transcripts for copying...');
    const allTranscripts = await fetchAllTranscripts(meeting.id);

    if (!allTranscripts.length) {
      const error_msg = 'No transcripts available to copy';
      console.log(error_msg);
      toast.error(error_msg);
      return null;
    }

    console.log(`✅ Formatting ${allTranscripts.length} transcripts`);

    const header = `# Transcript of the Meeting: ${meeting.id} - ${exportTitle}\n\n`;
    const date = `## Date: ${resolveMeetingDate(meetingDate).toLocaleDateString()}\n\n`;

    return formatTranscriptText(allTranscripts, {
      header: header + date,
      fallbackTimestamp: t => t.timestamp,
      markdownLineBreaks: true,
    });
  }, [meeting.id, meetingDate, exportTitle, fetchAllTranscripts]);

  // Copy transcript to clipboard
  const handleCopyTranscript = useCallback(async () => {
    const text = await buildTranscriptText();
    if (!text) return;

    await navigator.clipboard.writeText(text);
    toast.success("Transcript copied to clipboard");
  }, [buildTranscriptText]);

  // Export transcript to a file via the native save dialog.
  // The file content is identical to what Copy puts on the clipboard.
  const handleExportTranscript = useCallback(async () => {
    const text = await buildTranscriptText();
    if (!text) return;

    await exportFileWithDialog({
      content: text,
      defaultFileName: buildExportFileName({
        date: meetingDate,
        title: exportTitle,
        kind: 'transcript',
      }),
      kind: 'transcript',
    });
  }, [buildTranscriptText, meetingDate, exportTitle]);

  // Resolve the summary markdown the user is looking at. The live editor wins
  // over the saved body on purpose: unsaved edits are what the user sees on
  // screen, so Copy and Export must not silently drop them. The saved
  // `aiSummary.markdown` is only a fallback for when the editor is unmounted,
  // and legacy sections are the last resort for old meetings.
  const resolveSummaryMarkdown = useCallback(async (): Promise<string | null> => {
    let summaryMarkdown = '';

    console.log('🔍 Resolving summary markdown...');

    if (blockNoteSummaryRef.current?.getMarkdown) {
      console.log('📝 Trying to get markdown from ref...');
      summaryMarkdown = await blockNoteSummaryRef.current.getMarkdown();
      console.log('📝 Got markdown from ref, length:', summaryMarkdown.length);
    }

    if (!summaryMarkdown && aiSummary && typeof aiSummary.markdown === 'string') {
      console.log('📝 Using markdown from aiSummary');
      summaryMarkdown = aiSummary.markdown;
      console.log('📝 Markdown from aiSummary, length:', summaryMarkdown.length);
    }

    if (!summaryMarkdown && aiSummary) {
      console.log('📝 Converting legacy format to markdown');
      summaryMarkdown = convertLegacySummaryToMarkdown(aiSummary);
      console.log('📝 Converted legacy format, length:', summaryMarkdown.length);
    }

    return summaryMarkdown.trim() ? summaryMarkdown : null;
  }, [aiSummary, blockNoteSummaryRef]);

  // Build the complete summary document for an action; Copy and Export differ
  // only in the label word, so the clipboard and the file stay identical.
  const buildSummaryText = useCallback(async (
    label: SummaryDocumentLabel,
  ): Promise<string | null> => {
    const verb = label === 'Copied' ? 'copy' : 'export';

    if (!hasVisibleSummaryContent(aiSummary)) {
      toast.error(`No summary content available to ${verb}`);
      return null;
    }

    try {
      const summaryMarkdown = await resolveSummaryMarkdown();
      if (!summaryMarkdown) {
        console.error(`❌ No summary content available to ${verb}`);
        toast.error(`No summary content available to ${verb}`);
        return null;
      }

      return buildSummaryDocument({
        markdown: summaryMarkdown,
        meetingTitle: exportTitle,
        meetingId: meeting.id,
        meetingDate,
        label,
        language,
      });
    } catch (error) {
      console.error(`❌ Failed to ${verb} summary:`, error);
      toast.error(`Failed to ${verb} summary`);
      return null;
    }
  }, [aiSummary, exportTitle, meeting.id, meetingDate, resolveSummaryMarkdown, language]);

  // Copy summary to clipboard
  const handleCopySummary = useCallback(async () => {
    const fullMarkdown = await buildSummaryText('Copied');
    if (!fullMarkdown) return;

    try {
      await navigator.clipboard.writeText(fullMarkdown);

      console.log('✅ Successfully copied to clipboard!');
      toast.success("Summary copied to clipboard");
    } catch (error) {
      console.error('❌ Failed to copy summary:', error);
      toast.error("Failed to copy summary");
    }
  }, [buildSummaryText]);

  // Export summary to a markdown file via the native save dialog.
  // The file content is identical to what Copy puts on the clipboard, apart
  // from the `Exported on:` label line, and it includes unsaved edits.
  const handleExportSummary = useCallback(async () => {
    const fullMarkdown = await buildSummaryText('Exported');
    if (!fullMarkdown) return;

    await exportFileWithDialog({
      content: fullMarkdown,
      defaultFileName: buildExportFileName({
        date: meetingDate,
        title: exportTitle,
        kind: 'summary',
      }),
      kind: 'summary',
    });
  }, [buildSummaryText, meetingDate, exportTitle]);

  return {
    handleCopyTranscript,
    handleExportTranscript,
    handleCopySummary,
    handleExportSummary,
  };
}
