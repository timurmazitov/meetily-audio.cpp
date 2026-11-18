import { useEffect, useRef } from 'react';

import {
  TRANSCRIPT_UPDATED_EVENT,
  type TranscriptUpdatedPayload,
} from '@/lib/diarization';

/**
 * What the hook needs from the Tauri event bridge. The seam is injectable for
 * the same reason `TauriDiarizationClient` takes `invokeImpl`: a unit test can
 * drive the listener without a module-level Tauri mock leaking into other
 * suites.
 */
export type TranscriptUpdatedListen = (
  event: string,
  handler: (event: { payload: TranscriptUpdatedPayload }) => void,
) => Promise<() => void>;

/** Production bridge: Tauri's `listen`, imported lazily so merely importing
 * this module never touches the native dependency. */
const tauriListen: TranscriptUpdatedListen = async (event, handler) => {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<TranscriptUpdatedPayload>(event, handler);
};

/**
 * Refetches the open meeting when a background pass reports new speaker labels.
 *
 * The native event is the only refresh mechanism: the stop flow no longer
 * dispatches a window event, so a pass started by the recording, an import or a
 * retranscription refreshes the page the same way. The payload carries the
 * meeting id; an event that does not name a meeting cannot be attributed to the
 * open page and is ignored.
 *
 * Refreshing is silent on purpose: `failed` is announced by the
 * `diarization-unavailable` listener, and the configuration statuses owe the
 * user nothing. Kept in a ref so a new callback identity does not re-subscribe
 * the listener.
 */
export function useTranscriptUpdatedListener(
  meetingId: string | null | undefined,
  onUpdated: () => void,
  listenImpl: TranscriptUpdatedListen = tauriListen,
): void {
  const onUpdatedRef = useRef(onUpdated);
  useEffect(() => {
    onUpdatedRef.current = onUpdated;
  }, [onUpdated]);

  useEffect(() => {
    if (!meetingId) return;

    let unlisten: (() => void) | undefined;
    let cleanedUp = false;

    const setup = async () => {
      const stop = await listenImpl(TRANSCRIPT_UPDATED_EVENT, (event) => {
        if (event.payload.meeting_id !== meetingId) return;
        onUpdatedRef.current();
      });
      if (cleanedUp) {
        stop();
        return;
      }
      unlisten = stop;
    };

    void setup().catch((error) => {
      // The listener is an optimisation: without a Tauri bridge (unit tests, a
      // plain browser preview) the page must keep working.
      console.warn('Failed to listen for transcript updates:', error);
    });

    return () => {
      cleanedUp = true;
      unlisten?.();
    };
  }, [meetingId, listenImpl]);
}
