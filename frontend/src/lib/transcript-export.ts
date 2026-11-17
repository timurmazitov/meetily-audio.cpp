import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

/**
 * Make a string safe to use as a file name across platforms.
 */
export function sanitizeFileName(name: string): string {
  const sanitized = name.replace(/[\\/:*?"<>|]/g, '_').trim();
  return sanitized.length > 0 ? sanitized : 'transcript';
}

/**
 * Save transcript text to a file chosen in the native save dialog.
 * The file content is exactly the text passed in (same as the Copy button
 * produces). After a successful save, the full file path is copied to the
 * clipboard.
 *
 * Returns the saved file path, or null when the user cancels the dialog.
 */
export async function exportTranscriptToFile(
  content: string,
  defaultFileName: string,
): Promise<string | null> {
  let baseName = sanitizeFileName(defaultFileName);
  if (!baseName.toLowerCase().endsWith('.txt')) {
    baseName = `${baseName}.txt`;
  }

  try {
    const path = await invoke<string | null>('export_transcript_file_command', {
      content,
      defaultFileName: baseName,
    });

    if (!path) {
      return null; // User cancelled the dialog
    }

    await navigator.clipboard.writeText(path);
    toast.success('Transcript exported', { description: path });
    return path;
  } catch (error) {
    console.error('Failed to export transcript:', error);
    toast.error('Failed to export transcript');
    return null;
  }
}
