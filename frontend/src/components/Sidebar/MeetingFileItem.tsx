'use client';

import React from 'react';
import { File, Pencil, Plus, Trash2 } from 'lucide-react';

import { formatMeetingDateTime, formatMeetingDuration } from '@/lib/meeting-datetime';

export interface MeetingFileItemProps {
  title: string;
  /** RFC3339 moment of the meeting itself; `null`/absent while unknown. */
  recordedAt?: string | null;
  /** Meeting length in seconds; `null`/absent while unknown. */
  durationSeconds?: number | null;
  /** Transcript snippet when the row matched a search; rendered below the date. */
  matchContext?: string | null;
  /** `false` for non-meeting rows (the "+ New Call" placeholder), which get a plus icon. */
  isMeeting?: boolean;
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
 */
export function MeetingFileItem({
  title,
  recordedAt = null,
  durationSeconds = null,
  matchContext = null,
  isMeeting = true,
  onEdit,
  onDelete,
}: MeetingFileItemProps) {
  const date = formatMeetingDateTime(recordedAt);
  const duration = formatMeetingDuration(durationSeconds);
  const meta = [date, duration].filter((part): part is string => part !== null);

  return (
    <div className="flex flex-col w-full">
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
                aria-label="Edit meeting title"
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
                aria-label="Delete meeting"
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
          <span className="font-medium text-yellow-600">Match:</span> {matchContext}
        </div>
      )}
    </div>
  );
}

export default MeetingFileItem;
