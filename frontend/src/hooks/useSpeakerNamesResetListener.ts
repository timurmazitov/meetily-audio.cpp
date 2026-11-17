import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

import { SPEAKER_NAMES_RESET_HINT, type SpeakerNamesResetPayload } from '@/lib/diarization';

/**
 * Tells the user that a recount of the speaker labels removed every name of
 * the open meeting.
 *
 * Mirrors the `diarization-unavailable` listener: a recount is a background
 * pass, so the page explains the outcome instead of failing. The transcript is
 * refetched so the names disappear from the UI right away.
 *
 * `onReset` is called after a matching event; the caller decides how to
 * reload (the meeting page passes the pagination hook's `refetch`). It is kept
 * in a ref so a new callback identity does not re-subscribe the listener.
 */
export function useSpeakerNamesResetListener(
  meetingId: string | null | undefined,
  onReset: () => void,
): void {
  const onResetRef = useRef(onReset);
  onResetRef.current = onReset;

  useEffect(() => {
    if (!meetingId) return;

    let unlisten: (() => void) | undefined;
    let cleanedUp = false;

    const setup = async () => {
      const stop = await listen<SpeakerNamesResetPayload>('speaker-names-reset', (event) => {
        // The import path has no meeting yet, so a missing id matches any page.
        if (event.payload.meeting_id && event.payload.meeting_id !== meetingId) return;
        toast.info('Speaker names were reset', {
          description: event.payload.message ?? SPEAKER_NAMES_RESET_HINT,
        });
        onResetRef.current();
      });
      if (cleanedUp) {
        stop();
        return;
      }
      unlisten = stop;
    };

    void setup();

    return () => {
      cleanedUp = true;
      unlisten?.();
    };
  }, [meetingId]);
}
