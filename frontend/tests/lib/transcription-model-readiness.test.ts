import { describe, expect, test } from 'bun:test';

import {
  getProviderCommands,
  hasDownloadingModel,
} from '../../src/lib/transcription-model-readiness';

describe('transcription model readiness', () => {
  test('uses transcribe.cpp commands when transcribeCpp is the configured provider', () => {
    expect(getProviderCommands('transcribeCpp')).toEqual({
      initialize: 'transcribecpp_model_init',
      // The transcribe.cpp catalog is ASR-only, so the gate checks for an
      // installed transcription model rather than any catalogued artifact.
      hasAvailableModels: 'transcribecpp_has_transcription_model',
      getAvailableModels: 'transcribecpp_get_available_models',
    });
  });

  test('a removed provider soft-migrates to the transcribe.cpp commands', () => {
    // Saved configs predating the migration (and the repository's read-side
    // normalization) must never route to a command of a removed engine.
    for (const removed of ['localWhisper', 'parakeet', 'audioCpp']) {
      expect(getProviderCommands(removed)).toEqual(getProviderCommands('transcribeCpp'));
    }
  });

  test('an unknown provider also lands on the only local engine', () => {
    expect(getProviderCommands('deepgram')).toEqual(getProviderCommands('transcribeCpp'));
  });

  test('recognizes only active downloads', () => {
    expect(hasDownloadingModel([{ status: 'Available' }])).toBeFalse();
    expect(hasDownloadingModel([{ status: 'Downloading' }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 0 } } }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 42 } } }])).toBeTrue();
  });
});
