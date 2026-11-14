/**
 * Live-recording diarization wiring: only audioCpp recordings have a
 * diarization pass, and the pass must never be invoked for Whisper or
 * Parakeet.
 *
 * The Tauri bridge is not mocked at module scope here: mocking a Tauri module
 * leaks into other test files in the same bun process. The client seam is
 * injected instead, which is exactly what the stop hook uses.
 */

import { describe, expect, mock, test } from 'bun:test';
import {
  AUDIOCPP_DIARIZE_COMMAND,
  TauriDiarizationClient,
  diarizeIfAudioCpp,
  providerSupportsDiarization,
  type DiarizationClient,
  type DiarizationResult,
} from '../../src/lib/diarization';

function client(result: DiarizationResult | Error) {
  const diarizeMeeting = mock(async (_meetingId: string) => {
    if (result instanceof Error) throw result;
    return result;
  });
  const instance: DiarizationClient = { diarizeMeeting };
  return { instance, diarizeMeeting };
}

const success: DiarizationResult = {
  meeting_id: 'meeting-42',
  labeled_segments: 3,
  total_segments: 3,
  speakers_available: true,
  error: null,
};

describe('providerSupportsDiarization', () => {
  test('is true only for audioCpp', () => {
    expect(providerSupportsDiarization('audioCpp')).toBeTrue();
    expect(providerSupportsDiarization('localWhisper')).toBeFalse();
    expect(providerSupportsDiarization('parakeet')).toBeFalse();
    expect(providerSupportsDiarization(undefined)).toBeFalse();
  });
});

describe('diarizeIfAudioCpp', () => {
  test('invokes the diarization command for an audioCpp meeting', async () => {
    const calls: Array<[string, Record<string, unknown>]> = [];
    const client = new TauriDiarizationClient(async (command, args) => {
      calls.push([command, args]);
      return success;
    });

    const result = await diarizeIfAudioCpp(client, 'audioCpp', 'meeting-42');

    expect(AUDIOCPP_DIARIZE_COMMAND).toBe('audiocpp_diarize_meeting');
    expect(calls).toEqual([[AUDIOCPP_DIARIZE_COMMAND, { meetingId: 'meeting-42' }]]);
    expect(result).toEqual(success);
  });

  test('never invokes diarization for Whisper', async () => {
    const { instance, diarizeMeeting } = client(success);

    expect(await diarizeIfAudioCpp(instance, 'localWhisper', 'meeting-1')).toBeNull();
    expect(diarizeMeeting).not.toHaveBeenCalled();
  });

  test('never invokes diarization for Parakeet', async () => {
    const { instance, diarizeMeeting } = client(success);

    expect(await diarizeIfAudioCpp(instance, 'parakeet', 'meeting-1')).toBeNull();
    expect(diarizeMeeting).not.toHaveBeenCalled();
  });

  test('stays non-fatal when the native pass fails', async () => {
    const { instance, diarizeMeeting } = client(new Error('native library missing'));

    expect(await diarizeIfAudioCpp(instance, 'audioCpp', 'meeting-1')).toBeNull();
    expect(diarizeMeeting).toHaveBeenCalledTimes(1);
  });

  test('surfaces an unavailable pass without failing the stop flow', async () => {
    const { instance } = client({
      ...success,
      labeled_segments: 0,
      speakers_available: false,
      error: 'no diarization model',
    });

    const result = await diarizeIfAudioCpp(instance, 'audioCpp', 'meeting-42');

    expect(result?.speakers_available).toBeFalse();
    expect(result?.error).toBe('no diarization model');
  });
});
