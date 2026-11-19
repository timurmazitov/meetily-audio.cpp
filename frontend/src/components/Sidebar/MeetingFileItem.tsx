'use client';

import React from 'react';
import { useTranslation } from 'react-i18next';
import { File, Pencil, Plus, Trash2 } from 'lucide-react';

import type { AppLanguageCode } from '@/i18n';
import { formatMeetingDateTime, formatMeetingDuration } from '@/lib/meeting-datetime';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

export interface MeetingFileItemProps {
  title: string;
  /** Meeting id; the hover popup loads its speaker count from the backend. */
  meetingId?: string | null;
  /** RFC3339 moment of the meeting itself; `null`/absent while unknown. */
  recordedAt?: string | null;
  /** Meeting length in seconds; `null`/absent while unknown. */
  durationSeconds?: number | null;
  /** Transcript snippet when the row matched a search; rendered below the date. */
  matchContext?: string | null;
  /** `false` for non-meeting rows (the "+ New Call" placeholder), which get a plus icon. */
  isMeeting?: boolean;
  /**
   * Speaker count loader, the popup's seam to the diarization API. The row
   * asks it on the first hover only; resolving once per meeting and caching
   * for the sidebar session is the caller's job (the sidebar provider does
   * it). A rejected or `null` result means unknown — a meeting without
   * diarization or a failed request — and omits the popup's speakers line.
   */
  loadSpeakerCount?: (meetingId: string) => Promise<number | null>;
  onEdit?: () => void;
  onDelete?: () => void;
}

/**
 * The body of one meeting row in the sidebar.
 *
 * The title is clamped to two lines so a long name cannot turn the list into a
 * ragged column. Under it sits a quiet line with the meeting's own date and
 * length, skipping whatever is unknown — with both unknown there is no line at
 * all, since an empty line would only push the list apart.
 *
 * Hovering the whole row opens a popup that completes the row: the full title
 * without the clamp, the same date and length, and the speaker count. The
 * popup only ever adds lines for values it actually knows — a title is always
 * there, and the rest appear as they arrive — and it stays transparent to the
 * mouse so the row's own hover actions remain clickable.
 */
export function MeetingFileItem({
  title,
  meetingId = null,
  recordedAt = null,
  durationSeconds = null,
  matchContext = null,
  isMeeting = true,
  loadSpeakerCount,
  onEdit,
  onDelete,
}: MeetingFileItemProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.language as AppLanguageCode;
  const date = formatMeetingDateTime(recordedAt, language);
  const duration = formatMeetingDuration(durationSeconds, language);
  const meta = [date, duration].filter((part): part is string => part !== null);

  // The speaker count is loaded on the first hover only, and the caller's
  // loader caches it for the sidebar session, so neither repeated hovers nor
  // list scrolling ask the backend again.
  const speakerRequestStarted = React.useRef(false);
  const [speakerCount, setSpeakerCount] = React.useState<number | null>(null);
  const handleMouseEnter = () => {
    if (!isMeeting || !meetingId || !loadSpeakerCount || speakerRequestStarted.current) return;
    speakerRequestStarted.current = true;
    void loadSpeakerCount(meetingId)
      .then((count) => {
        if (typeof count === 'number' && count > 0) setSpeakerCount(count);
      })
      .catch(() => {
        // Unknown count: the popup line simply never appears.
      });
  };

  const row = (
    <div className="flex flex-col w-full" onMouseEnter={isMeeting ? handleMouseEnter : undefined}>
      <div className="flex items-center w-full">
        {isMeeting ? (
          <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-gray-100">
            <File className="w-3.5 h-3.5 text-gray-600" />
          </div>
        ) : (
          <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-blue-100">
            <Plus className="w-3.5 h-3.5 text-blue-600" />
          </div>
        )}
        <span className="flex-1 line-clamp-2">{title}</span>
        {isMeeting && (
          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
            {onEdit && (
              <button
                onClick={(event) => {
                  event.stopPropagation();
                  onEdit();
                }}
                className="hover:text-blue-600 p-1 rounded-md hover:bg-blue-50 flex-shrink-0"
                aria-label={t('sidebar.editMeetingAria')}
              >
                <Pencil className="w-4 h-4" />
              </button>
            )}
            {onDelete && (
              <button
                onClick={(event) => {
                  event.stopPropagation();
                  onDelete();
                }}
                className="hover:text-red-600 p-1 rounded-md hover:bg-red-50 flex-shrink-0"
                aria-label={t('sidebar.deleteMeetingAria')}
              >
                <Trash2 className="w-4 h-4" />
              </button>
            )}
          </div>
        )}
      </div>

      {meta.length > 0 && (
        <div className="mt-1 ml-8 text-xs text-gray-500">{meta.join(' · ')}</div>
      )}

      {matchContext && (
        <div className="mt-1 ml-8 text-xs text-gray-500 bg-yellow-50 p-1.5 rounded border border-yellow-100 line-clamp-2">
          <span className="font-medium text-yellow-600">{t('sidebar.matchPrefix')}</span> {matchContext}
        </div>
      )}
    </div>
  );

  if (!isMeeting) return row;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{row}</TooltipTrigger>
      <TooltipContent
        side="right"
        // The popup never takes the mouse: it must not stand between the
        // cursor and the Edit/Delete buttons the row reveals on hover.
        className="max-w-xs pointer-events-none"
      >
        <div className="flex flex-col gap-0.5 text-left">
          {/* The full title, without the row's two-line clamp. */}
          <span className="font-medium break-words">{title}</span>
          {date && <span>{date}</span>}
          {duration && <span>{duration}</span>}
          {speakerCount !== null && speakerCount > 0 && (
            <span>{t('sidebar.popup.speakers', { count: speakerCount })}</span>
          )}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export default MeetingFileItem;
