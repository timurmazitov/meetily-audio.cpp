import { useCallback, RefObject } from 'react';
import { MeetingSummary, Transcript } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { toast } from 'sonner';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { hasVisibleSummaryContent } from '@/lib/summary-content';
import { buildSummaryDocument, convertLegacySummaryToMarkdown } from '@/lib/summary-document';
import { formatTranscriptText } from '@/lib/transcript-format';
import { buildExportFileName, exportFileWithDialog } from '@/lib/export-file';

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

    const header = `# Transcript of the Meeting: ${meeting.id} - ${meetingTitle ?? meeting.title}\n\n`;
    const date = `## Date: ${new Date(meeting.created_at).toLocaleDateString()}\n\n`;

    return formatTranscriptText(allTranscripts, {
      header: header + date,
      fallbackTimestamp: t => t.timestamp,
      markdownLineBreaks: true,
    });
  }, [meeting, meetingTitle, fetchAllTranscripts]);

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
        date: meeting.created_at,
        title: meetingTitle ?? meeting.title,
        suffix: 'transcript',
        extension: 'txt',
      }),
      extension: 'txt',
      filterLabel: 'Text Files',
      successMessage: 'Transcript exported',
      errorMessage: 'Failed to export transcript',
    });
  }, [buildTranscriptText, meeting, meetingTitle]);

  // Copy summary to clipboard
  const handleCopySummary = useCallback(async () => {
    if (!hasVisibleSummaryContent(aiSummary)) {
      toast.error('No summary content available to copy');
      return;
    }
    try {
      let summaryMarkdown = '';

      console.log('🔍 Copy Summary - Starting...');

      // Try to get markdown from BlockNote editor first
      if (blockNoteSummaryRef.current?.getMarkdown) {
        console.log('📝 Trying to get markdown from ref...');
        summaryMarkdown = await blockNoteSummaryRef.current.getMarkdown();
        console.log('📝 Got markdown from ref, length:', summaryMarkdown.length);
      }

      // Fallback: Check if aiSummary has markdown property
      if (!summaryMarkdown && aiSummary && typeof aiSummary.markdown === 'string') {
        console.log('📝 Using markdown from aiSummary');
        summaryMarkdown = aiSummary.markdown;
        console.log('📝 Markdown from aiSummary, length:', summaryMarkdown.length);
      }

      // Fallback: Check for legacy format
      if (!summaryMarkdown && aiSummary) {
        console.log('📝 Converting legacy format to markdown');
        summaryMarkdown = convertLegacySummaryToMarkdown(aiSummary);
        console.log('📝 Converted legacy format, length:', summaryMarkdown.length);
      }

      // If still no summary content, show message
      if (!summaryMarkdown.trim()) {
        console.error('❌ No summary content available to copy');
        toast.error('No summary content available to copy');
        return;
      }

      const fullMarkdown = buildSummaryDocument({
        markdown: summaryMarkdown,
        meetingTitle,
        meetingId: meeting.id,
        meetingDate: meeting.created_at,
        label: 'Copied',
      });
      await navigator.clipboard.writeText(fullMarkdown);

      console.log('✅ Successfully copied to clipboard!');
      toast.success("Summary copied to clipboard");
    } catch (error) {
      console.error('❌ Failed to copy summary:', error);
      toast.error("Failed to copy summary");
    }
  }, [aiSummary, meetingTitle, meeting, blockNoteSummaryRef]);

  return {
    handleCopyTranscript,
    handleExportTranscript,
    handleCopySummary,
  };
}
