import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

/** Longest meeting-title fragment kept in an export file name. */
const MAX_TITLE_LENGTH = 60;

/** Title used when a meeting has no usable name. */
const FALLBACK_TITLE = 'meeting';

/** Placeholder title of a not-yet-saved recording; never used as a file name. */
const NEW_CALL_TITLE = '+ New Call';

/** Characters that are illegal in file names on at least one supported OS. */
const FORBIDDEN_FILE_NAME_CHARS = /[\\/:*?"<>|\u0000-\u001f\u007f]/g;

/** A trailing `.<word>` segment that already looks like a file extension. */
const EXISTING_EXTENSION = /\.[A-Za-z0-9]{1,10}$/;

const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;

export interface ExportFileNameOptions {
  /** Local meeting date: a created_at string, a Date, or nothing. */
  date?: string | Date | null;
  /** Meeting title as shown to the user. */
  title?: string | null;
  /** Kind of export, e.g. `transcript` or `summary`. */
  suffix: string;
  /** File extension without the leading dot, e.g. `txt`. */
  extension: string;
}

export interface ExportFileOptions {
  /** Text written to the file, byte for byte. */
  content: string;
  /** Name offered by the save dialog. */
  defaultFileName: string;
  /** Extension appended when the name has none, e.g. `txt`. */
  extension: string;
  /** Save-dialog filter caption, e.g. `Text Files`. */
  filterLabel: string;
  /** Toast shown after a successful save. */
  successMessage: string;
  /** Toast shown when the dialog or the write fails. */
  errorMessage?: string;
}

/**
 * Build the default file name for an export:
 * `<YYYYMMDD>-<normalized meeting title>-<suffix>.<extension>`.
 *
 * The date is the local calendar day of the meeting; a missing or unparsable
 * date falls back to the current moment. The title keeps its script and case
 * but is made safe for file systems, and an empty title becomes `meeting`.
 */
export function buildExportFileName({
  date,
  title,
  suffix,
  extension,
}: ExportFileNameOptions): string {
  const day = resolveLocalDate(date);
  const baseName = `${formatDateStamp(day)}-${normalizeTitle(title)}-${suffix}`;
  return ensureFileExtension(baseName, extension);
}

/**
 * Append `extension` to a default file name that has none. A file name that
 * already ends with an extension is returned unchanged, so a name typed by the
 * user (for example `.markdown`) is never rewritten.
 */
export function ensureFileExtension(fileName: string, extension: string): string {
  const normalizedExtension = extension.trim().replace(/^\.+/, '');
  if (!normalizedExtension) {
    return fileName;
  }
  if (EXISTING_EXTENSION.test(fileName)) {
    return fileName;
  }
  return `${fileName}.${normalizedExtension}`;
}

/**
 * Save text to a file chosen in the native save dialog. The file content is
 * exactly the text passed in. After a successful save the full path is copied
 * to the clipboard and a toast names the file.
 *
 * Returns the saved path, or null when the user cancels the dialog.
 */
export async function exportFileWithDialog({
  content,
  defaultFileName,
  extension,
  filterLabel,
  successMessage,
  errorMessage = 'Failed to export file',
}: ExportFileOptions): Promise<string | null> {
  const fileName = ensureFileExtension(defaultFileName, extension);

  try {
    const path = await invoke<string | null>('export_transcript_file_command', {
      content,
      defaultFileName: fileName,
      extension,
      filterLabel,
    });

    if (!path) {
      return null; // User cancelled the dialog
    }

    await navigator.clipboard.writeText(path);
    toast.success(successMessage, { description: path });
    return path;
  } catch (error) {
    console.error(`${errorMessage}:`, error);
    toast.error(errorMessage);
    return null;
  }
}

/** Meeting title normalized for a file name, never empty. */
function normalizeTitle(title?: string | null): string {
  const trimmed = (title ?? '').trim();
  if (!trimmed || trimmed === NEW_CALL_TITLE) {
    return FALLBACK_TITLE;
  }

  const normalized = trimmed
    .replace(FORBIDDEN_FILE_NAME_CHARS, '_')
    .replace(/\s+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '');

  // Truncate by code point so surrogate pairs are never split.
  const truncated = Array.from(normalized)
    .slice(0, MAX_TITLE_LENGTH)
    .join('')
    .replace(/_+$/g, '');

  return truncated || FALLBACK_TITLE;
}

/** Local calendar day for a created_at value, or now when it is unusable. */
function resolveLocalDate(date?: string | Date | null): Date {
  if (date instanceof Date && !Number.isNaN(date.getTime())) {
    return date;
  }

  if (typeof date === 'string') {
    const value = date.trim();
    // A date-only created_at names a calendar day, not an instant: read its
    // parts directly so the local time zone cannot shift the day.
    const dateOnly = DATE_ONLY.exec(value);
    if (dateOnly) {
      const [, year, month, day] = dateOnly;
      const local = new Date(Number(year), Number(month) - 1, Number(day));
      if (!Number.isNaN(local.getTime())) {
        return local;
      }
    } else if (value) {
      const parsed = new Date(value);
      if (!Number.isNaN(parsed.getTime())) {
        return parsed;
      }
    }
  }

  return new Date();
}

/** `YYYYMMDD` for the given moment in the local time zone. */
function formatDateStamp(date: Date): string {
  const year = String(date.getFullYear()).padStart(4, '0');
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}${month}${day}`;
}
