/**
 * Provider-independent diarization seam.
 *
 * The decision "can this meeting be diarized" lives in native code (installed
 * Nemotron model + audio.cpp runtime + no active recording). The frontend has
 * no provider gate: the same command is called for Whisper, Parakeet and
 * audioCpp, and the returned status drives the UI.
 *
 * The Tauri bridge is not mocked at module scope here: mocking a Tauri module
 * leaks into other test files in the same bun process. The client seam is
 * injected instead, which is exactly what the stop flow uses.
 */

import { describe, expect, mock, test } from 'bun:test';
import {
  AUDIOCPP_DIARIZE_COMMAND,
  DIARIZATION_DEFERRED_MESSAGE,
  DIARIZATION_FAILED_MESSAGE,
  DIARIZATION_NO_MODEL_MESSAGE,
  DIARIZATION_NO_RUNTIME_MESSAGE,
  DIARIZATION_UNAVAILABLE_HINT,
  SPEAKER_NAMES_RESET_HINT,
  TRANSCRIPT_UPDATED_EVENT,
  TauriDiarizationClient,
  diarizationStatusMessage,
  diarizeSavedMeeting,
  hasInstalledDiarizationModel,
  shouldToastDiarizationStatus,
  shouldToastDiarizationUnavailable,
  type DiarizationClient,
  type DiarizationResult,
  type DiarizationStatus,
} from '../../src/lib/diarization';

function client(result: DiarizationResult | Error) {
  const diarizeMeeting = mock(async (_meetingId: string) => {
    if (result instanceof Error) throw result;
    return result;
  });
  const instance: DiarizationClient = { diarizeMeeting };
  return { instance, diarizeMeeting };
}

function resultFor(status: DiarizationStatus): DiarizationResult {
  return {
    meeting_id: 'meeting-42',
    status,
    labeled_segments: status === 'ran' ? 3 : 0,
    total_segments: 3,
    speakers_available: status === 'ran',
    error: status === 'failed' ? 'engine exploded' : null,
  };
}

describe('diarizeSavedMeeting', () => {
  test('calls the same command with the same arguments for every transcription provider', async () => {
    // The seam takes no provider at all; the loop documents that the three
    // providers a meeting can originate from now behave identically.
    for (const _provider of ['localWhisper', 'parakeet', 'audioCpp']) {
      const calls: Array<[string, Record<string, unknown>]> = [];
      const client = new TauriDiarizationClient(async (command, args) => {
        calls.push([command, args]);
        return resultFor('ran');
      });

      const result = await diarizeSavedMeeting(client, 'meeting-42');

      expect(AUDIOCPP_DIARIZE_COMMAND).toBe('audiocpp_diarize_meeting');
      expect(calls).toEqual([[AUDIOCPP_DIARIZE_COMMAND, { meetingId: 'meeting-42' }]]);
      expect(result).toEqual(resultFor('ran'));
    }
  });

  test('returns the native status untouched so the caller can switch on it', async () => {
    for (const status of [
      'ran',
      'no_model',
      'no_runtime',
      'deferred',
      'failed',
    ] as DiarizationStatus[]) {
      const { instance } = client(resultFor(status));

      const result = await diarizeSavedMeeting(instance, 'meeting-42');

      expect(result?.status).toBe(status);
    }
  });

  test('stays non-fatal when the native command rejects', async () => {
    const { instance, diarizeMeeting } = client(new Error('native library missing'));

    expect(await diarizeSavedMeeting(instance, 'meeting-1')).toBeNull();
    expect(diarizeMeeting).toHaveBeenCalledTimes(1);
  });
});

describe('shouldToastDiarizationStatus', () => {
  // Automatic passes (after a recording, import or retranscription) only speak
  // up when something broke: the three configuration statuses were not
  // requested by the user.
  test('toasts only a failed pass', () => {
    const expected: Array<[DiarizationStatus, boolean]> = [
      ['ran', false],
      ['no_model', false],
      ['no_runtime', false],
      ['deferred', false],
      ['failed', true],
    ];

    for (const [status, shouldToast] of expected) {
      expect(shouldToastDiarizationStatus(status)).toBe(shouldToast);
    }
  });
});

describe('shouldToastDiarizationUnavailable', () => {
  // The `diarization-unavailable` native event now fires only for `failed`,
  // but the listener checks the payload status before speaking: if native code
  // ever widens the event, the UI must stay quiet for configuration reasons
  // instead of re-learning the old "toast for everything" behaviour.
  test('stays silent for every status except failed', () => {
    const expected: Array<[DiarizationStatus | undefined, boolean]> = [
      ['ran', false],
      ['no_model', false],
      ['no_runtime', false],
      ['deferred', false],
      ['failed', true],
      [undefined, false],
    ];

    for (const [status, shouldToast] of expected) {
      expect(shouldToastDiarizationUnavailable(status)).toBe(shouldToast);
    }
  });

  test('agrees with the automatic-pass rule', () => {
    for (const status of [
      'ran',
      'no_model',
      'no_runtime',
      'deferred',
      'failed',
    ] as DiarizationStatus[]) {
      expect(shouldToastDiarizationUnavailable(status)).toBe(
        shouldToastDiarizationStatus(status),
      );
    }
  });
});

describe('diarizationStatusMessage', () => {
  test('explains every non-success status and stays silent after a pass', () => {
    const expected: Array<[DiarizationStatus, string | null]> = [
      ['ran', null],
      ['no_model', DIARIZATION_NO_MODEL_MESSAGE],
      ['no_runtime', DIARIZATION_NO_RUNTIME_MESSAGE],
      ['deferred', DIARIZATION_DEFERRED_MESSAGE],
      ['failed', DIARIZATION_FAILED_MESSAGE],
    ];

    for (const [status, message] of expected) {
      expect(diarizationStatusMessage(status)).toBe(message);
    }
  });

  test('a manual run never explains two different refusals with the same text', () => {
    const messages = [
      DIARIZATION_NO_MODEL_MESSAGE,
      DIARIZATION_NO_RUNTIME_MESSAGE,
      DIARIZATION_DEFERRED_MESSAGE,
      DIARIZATION_FAILED_MESSAGE,
    ];

    expect(new Set(messages).size).toBe(messages.length);
  });
});

describe('mirrored native texts', () => {
  // The native constants in
  // `src-tauri/src/audio/transcription/diarization_service.rs` are the source
  // of truth; the frontend duplicates them for direct command answers.
  test('match the native message strings verbatim', () => {
    const expected: Array<[string, string]> = [
      [
        DIARIZATION_NO_MODEL_MESSAGE,
        'Speaker labels were skipped. Download the Nemotron-3-Diarization model in Settings to see who said what.',
      ],
      [
        DIARIZATION_NO_RUNTIME_MESSAGE,
        'Speaker labels were skipped. The audio.cpp engine is unavailable on this system.',
      ],
      [
        DIARIZATION_FAILED_MESSAGE,
        'Speaker labels were skipped because diarization failed.',
      ],
      [
        DIARIZATION_UNAVAILABLE_HINT,
        'Download the Nemotron-3-Diarization model in Settings to see who said what.',
      ],
      [
        SPEAKER_NAMES_RESET_HINT,
        'Speaker names were reset because the voices were recomputed.',
      ],
    ];

    for (const [actual, expectedText] of expected) {
      expect(actual).toBe(expectedText);
    }
  });

  test('names the native transcript update event', () => {
    expect(TRANSCRIPT_UPDATED_EVENT).toBe('transcript-updated');
  });
});

describe('hasInstalledDiarizationModel', () => {
  test('accepts only an installed diarization model', () => {
    const expected: Array<[Array<{ task: 'asr' | 'diar'; status: unknown }>, boolean]> = [
      [[], false],
      [[{ task: 'asr', status: 'Available' }], false],
      [[{ task: 'diar', status: 'Missing' }], false],
      [[{ task: 'diar', status: { Downloading: { progress: 40 } } }], false],
      [[{ task: 'diar', status: 'Available' }], true],
      [[{ task: 'asr', status: 'Available' }, { task: 'diar', status: 'Available' }], true],
    ];

    for (const [models, expectedPresence] of expected) {
      expect(
        hasInstalledDiarizationModel(models as Parameters<typeof hasInstalledDiarizationModel>[0]),
      ).toBe(expectedPresence);
    }
  });
});
