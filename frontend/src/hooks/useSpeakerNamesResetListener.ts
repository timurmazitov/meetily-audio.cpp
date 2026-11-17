import { useEffect, useRef } from 'react';
import { toast } from 'sonner';

import {
  SPEAKER_NAMES_RESET_EVENT,
  SPEAKER_NAMES_RESET_HINT,
  type SpeakerNamesResetPayload,
} from '@/lib/diarization';

/**
 * What the hook needs from the Tauri event bridge. The seam is injectable for
 * the same reason `TauriDiarizationClient` takes `invokeImpl`: a unit test can
 * drive the listener without a module-level Tauri mock leaking into other
 * suites.
 */
export type SpeakerNamesResetListen = (
  event: string,
  handler: (event: { payload: SpeakerNamesResetPayload }) => void,
) => Promise<() => void>;

/** Production bridge: Tauri's `listen`, imported lazily so merely importing
 * this module never touches the native dependency. */
const tauriListen: SpeakerNamesResetListen = async (event, handler) => {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<SpeakerNamesResetPayload>(event, handler);
};

/**
 * Tells the user that a recount of the speaker labels removed every name of
 * the open meeting.
 *
 * Mirrors the `diarization-unavailable` listener: a recount is a background
 * pass, so the page explains the outcome instead of failing. The transcript is
 * refetched so the names disappear from the UI right away.
 *
 * `onReset` is called after a matching event; the caller decides how to reload
 * (the meeting page passes the pagination hook's `refetch`). It is kept in a
 * ref so a new callback identity does not re-subscribe the listener.
 */
export function useSpeakerNamesResetListener(
  meetingId: string | null | undefined,
  onReset: () => void,
  listenImpl: SpeakerNamesResetListen = tauriListen,
): void {
  const onResetRef = useRef(onReset);
  useEffect(() => {
    onResetRef.current = onReset;
  }, [onReset]);

  useEffect(() => {
    if (!meetingId) return;

    let unlisten: (() => void) | undefined;
    let cleanedUp = false;

    const setup = async () => {
      const stop = await listenImpl(SPEAKER_NAMES_RESET_EVENT, (event) => {
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

    void setup().catch((error) => {
      // The listener is a hint: without a Tauri bridge (unit tests, a plain
      // browser preview) the page must keep working.
      console.warn('Failed to listen for speaker name resets:', error);
    });

    return () => {
      cleanedUp = true;
      unlisten?.();
    };
  }, [meetingId, listenImpl]);
}
