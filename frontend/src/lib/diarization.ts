/**
 * Post-transcription diarization of a saved meeting.
 *
 * The recording pipeline saves the meeting as soon as the last transcript
 * segment lands; diarization is a separate pass over the saved recording
 * (GLOSSARY: «диаризация» — postprocessing after transcription). The pass is
 * independent of the transcription provider: native code decides whether it
 * can run (installed Nemotron model, available audio.cpp runtime, no active
 * recording) and reports the outcome as a status. This module holds the seam
 * and the status→UI rules; it never decides availability itself.
 *
 * Kept free of Tauri imports at module scope so plain unit tests can inject a
 * fake: production uses `TauriDiarizationClient` via `diarizationClient`.
 * Tauri's `invoke` is imported dynamically inside the production client so a
 * test that never touches it does not need the native bridge.
 */

import type { AudioCppModelInfo } from './audiocpp';

/**
 * Why a diarization pass did or did not happen, as native code reports it.
 *
 * `ran` with zero labeled segments is a successful pass over a recording with
 * no distinguishable voices, not a failure.
 */
export type DiarizationStatus = 'ran' | 'no_model' | 'no_runtime' | 'deferred' | 'failed';

/** Result of a diarization pass, as returned by `audiocpp_diarize_meeting`. */
export interface DiarizationResult {
  meeting_id: string;
  /** Why the pass did or did not run. */
  status: DiarizationStatus;
  /** Number of transcript segments that received a speaker label. */
  labeled_segments: number;
  total_segments: number;
  /** Historical availability flag, derived natively from `status === 'ran'`. */
  speakers_available: boolean;
  /** Diagnostic detail for `failed`; never user-facing prose. */
  error?: string | null;
}

/** The native command the diarization seam calls. */
export const AUDIOCPP_DIARIZE_COMMAND = 'audiocpp_diarize_meeting';

/** The seam between the UI and the native diarization command. */
export interface DiarizationClient {
  /** Diarize a saved meeting, resolving with the native result. */
  diarizeMeeting(meetingId: string): Promise<DiarizationResult>;
}

/** Native implementation over the Tauri command. */
export class TauriDiarizationClient implements DiarizationClient {
  private readonly invokeImpl?: (
    command: string,
    args: Record<string, unknown>,
  ) => Promise<DiarizationResult>;

  /**
   * `invokeImpl` is the test seam: production leaves it unset and the Tauri
   * bridge is imported lazily, so unit tests can assert the exact command
   * without module-level Tauri mocks leaking across test files.
   */
  constructor(
    invokeImpl?: (command: string, args: Record<string, unknown>) => Promise<DiarizationResult>,
  ) {
    this.invokeImpl = invokeImpl;
  }

  async diarizeMeeting(meetingId: string): Promise<DiarizationResult> {
    const invoke =
      this.invokeImpl ?? (await import('@tauri-apps/api/core')).invoke;
    return invoke(AUDIOCPP_DIARIZE_COMMAND, { meetingId });
  }
}

/**
 * Mirrored native message texts.
 *
 * `frontend/src-tauri/src/audio/transcription/diarization_service.rs` is the
 * source of truth; the frontend duplicates the strings it needs for direct
 * answers to an explicit run. `tests/lib/live-diarization.test.ts` compares
 * them verbatim, so the two cannot drift apart silently.
 */

/** Native `DIARIZATION_UNAVAILABLE_MESSAGE`: no diarization model installed. */
export const DIARIZATION_NO_MODEL_MESSAGE =
  'Speaker labels were skipped. Download the Nemotron-3-Diarization model in Settings to see who said what.';

/** Native `DIARIZATION_RUNTIME_UNAVAILABLE_MESSAGE`: model present, library missing. */
export const DIARIZATION_NO_RUNTIME_MESSAGE =
  'Speaker labels were skipped. The audio.cpp engine is unavailable on this system.';

/** Native `DIARIZATION_FAILED_MESSAGE`: the pass was attempted and broke. */
export const DIARIZATION_FAILED_MESSAGE =
  'Speaker labels were skipped because diarization failed.';

/**
 * No native counterpart: a deferred pass is silent in automatic paths, so
 * native code has no message for it. The panel explains it when the user asks
 * for a run during a recording.
 */
export const DIARIZATION_DEFERRED_MESSAGE =
  'Speaker labels were postponed because a recording is in progress. Run diarization again once the recording stops.';

/**
 * No native counterpart: `ran` with zero labels is a successful pass, so native
 * code has no refusal text. The panel still explains why the list stayed empty
 * after the user pressed the button.
 */
export const DIARIZATION_NO_SPEAKERS_FOUND_MESSAGE =
  'Diarization finished but found no distinct speakers in this recording.';

/** Hint shown in the speakers panel when the meeting has no labels yet. */
export const DIARIZATION_UNAVAILABLE_HINT =
  'Download the Nemotron-3-Diarization model in Settings to see who said what.';

/** Hint shown when a recount of the labels removed every speaker name. */
export const SPEAKER_NAMES_RESET_HINT =
  'Speaker names were reset because the voices were recomputed.';

/** The native event that announces a full reset of the meeting's names. */
export const SPEAKER_NAMES_RESET_EVENT = 'speaker-names-reset';

/** Payload of the `speaker-names-reset` event. */
export interface SpeakerNamesResetPayload {
  /**
   * The meeting whose names changed. Every save path recounts on a saved
   * meeting, so the id is always present; optional for older payloads.
   */
  meeting_id?: string;
  message?: string;
}

/**
 * The native event announcing that a saved meeting's transcript gained speaker
 * labels. The meeting page subscribes to it instead of the old window event, so
 * a background pass can refresh an open meeting on its own.
 */
export const TRANSCRIPT_UPDATED_EVENT = 'transcript-updated';

/** Payload of the `transcript-updated` event. */
export interface TranscriptUpdatedPayload {
  /** The meeting whose labels changed; absent payloads match no page. */
  meeting_id?: string;
  labeled_segments?: number;
  total_segments?: number;
}

/**
 * Diarize a saved meeting, whatever provider transcribed it.
 *
 * Non-fatal by design: the meeting is already saved and a missing model, a
 * missing runtime or a failed pass only leaves the speaker labels out. A
 * rejected native call resolves to `null` so the caller never has to guard the
 * stop flow against it.
 */
export async function diarizeSavedMeeting(
  client: DiarizationClient,
  meetingId: string,
): Promise<DiarizationResult | null> {
  try {
    return await client.diarizeMeeting(meetingId);
  } catch {
    return null;
  }
}

/**
 * Whether an automatic pass should interrupt the user with a toast.
 *
 * Only `failed` does: the three configuration statuses were not requested by
 * the user, so `no_model`, `no_runtime` and `deferred` stay silent.
 */
export function shouldToastDiarizationStatus(status: DiarizationStatus): boolean {
  return status === 'failed';
}

/** What an automatic pass needs from the toast library (only `error`). */
export interface DiarizationToast {
  error(message: string, options?: { description?: string }): void;
}

/**
 * The automatic pass after a live recording, as one testable step.
 *
 * Returns a promise instead of being fire-and-forget itself so a test can
 * await the outcome; the stop flow drops it with `void`, which is what keeps
 * saving and navigation off the pass's critical path. Never rejects: a native
 * failure leaves the saved meeting unlabeled and stays silent, because the
 * meeting and its transcript are already safe.
 */
export async function startLiveDiarization(
  client: DiarizationClient,
  meetingId: string,
  toast: DiarizationToast,
): Promise<void> {
  const result = await diarizeSavedMeeting(client, meetingId);
  // `null` means the native command itself failed: the meeting stays saved
  // without labels, and there is no status to explain.
  if (result === null) return;
  if (!shouldToastDiarizationStatus(result.status)) return;
  toast.error('Diarization failed', { description: DIARIZATION_FAILED_MESSAGE });
}

/** Payload of the native `diarization-unavailable` event. */
export interface DiarizationUnavailablePayload {
  /** The meeting the pass ran for; absent on events from older callers. */
  meeting_id?: string;
  /** Why the pass did not produce labels. */
  status?: DiarizationStatus;
  /** Diagnostic detail; never user-facing prose. */
  reason?: string | null;
  /** User-facing explanation, owned by native code. */
  message?: string;
}

/**
 * Whether an incoming `diarization-unavailable` event is worth a toast.
 *
 * Native code emits this event only for `failed`, but the listeners check the
 * payload status instead of assuming that: if the native contract is ever
 * widened, automatic passes stay silent for configuration reasons and only a
 * real failure speaks up. Kept as one helper so import and retranscription
 * cannot drift from the automatic-pass rule.
 */
export function shouldToastDiarizationUnavailable(
  status: DiarizationStatus | undefined,
): boolean {
  return status !== undefined && shouldToastDiarizationStatus(status);
}

/**
 * Explanation of a status for an explicit run, or `null` after a pass.
 *
 * Unlike the automatic path, the panel shows the reason for every refusal: it
 * answers a button the user pressed.
 */
export function diarizationStatusMessage(status: DiarizationStatus): string | null {
  switch (status) {
    case 'ran':
      return null;
    case 'no_model':
      return DIARIZATION_NO_MODEL_MESSAGE;
    case 'no_runtime':
      return DIARIZATION_NO_RUNTIME_MESSAGE;
    case 'deferred':
      return DIARIZATION_DEFERRED_MESSAGE;
    case 'failed':
      return DIARIZATION_FAILED_MESSAGE;
  }
}

/**
 * Whether a catalogued model can actually diarize: the `diar` task with an
 * installed artifact. Mirrors the native selection rule, so the manual run
 * button is disabled exactly when the pass would answer `no_model`.
 */
export function hasInstalledDiarizationModel(
  models: Array<Pick<AudioCppModelInfo, 'task' | 'status'>>,
): boolean {
  return models.some((model) => model.task === 'diar' && model.status === 'Available');
}
