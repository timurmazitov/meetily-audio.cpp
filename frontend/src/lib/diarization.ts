/**
 * Post-transcription diarization for live recordings.
 *
 * The recording pipeline saves the meeting as soon as the last transcript
 * segment lands; diarization is a separate pass over the saved recording
 * (GLOSSARY: «диаризация» — postprocessing after transcription). Only the
 * audioCpp provider has a diarization engine, so this is a no-op for Whisper
 * and Parakeet.
 *
 * Kept free of Tauri imports at module scope so plain unit tests can inject a
 * fake: production uses `TauriDiarizationClient` via `diarizationClient`.
 * Tauri's `invoke` is imported dynamically inside the production client so a
 * test that never touches it does not need the native bridge.
 */

import { AUDIO_CPP_PROVIDER } from './transcription-provider';

/** Result of a diarization pass, as returned by `audiocpp_diarize_meeting`. */
export interface DiarizationResult {
  meeting_id: string;
  /** Number of transcript segments that received a speaker label. */
  labeled_segments: number;
  total_segments: number;
  /** False when no diarization model is installed or the pass failed. */
  speakers_available: boolean;
  error?: string | null;
}

/** The native command the diarization seam calls. */
export const AUDIOCPP_DIARIZE_COMMAND = 'audiocpp_diarize_meeting';

/** The seam between the stop flow and the native diarization command. */
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

/** Whether the provider that recorded this meeting has a diarization pass. */
export function providerSupportsDiarization(provider: string | undefined): boolean {
  return provider === AUDIO_CPP_PROVIDER;
}

/** Hint shown when the diarization pass could not label the transcript. */
export const DIARIZATION_UNAVAILABLE_HINT =
  'Download the Nemotron-3-Diarization model in Settings to see who said what.';

/** Hint shown when a recount of the labels removed every speaker name. */
export const SPEAKER_NAMES_RESET_HINT =
  'Speaker names were reset because the voices were recomputed.';

/** The native event that announces a full reset of the meeting's names. */
export const SPEAKER_NAMES_RESET_EVENT = 'speaker-names-reset';

/** Payload of the `speaker-names-reset` event. */
export interface SpeakerNamesResetPayload {
  /** Absent when the event comes from an import, which has no saved meeting yet. */
  meeting_id?: string;
  message?: string;
}

/**
 * Diarize a just-saved live recording when (and only when) the configured
 * provider is audioCpp.
 *
 * Non-fatal by design: the meeting is already saved and a missing model or a
 * failed pass only leaves the speaker labels out. Any error resolves to
 * `null` so the caller never has to guard the stop flow against it.
 */
export async function diarizeIfAudioCpp(
  client: DiarizationClient,
  provider: string | undefined,
  meetingId: string,
): Promise<DiarizationResult | null> {
  if (!providerSupportsDiarization(provider)) {
    return null;
  }
  try {
    return await client.diarizeMeeting(meetingId);
  } catch {
    return null;
  }
}
