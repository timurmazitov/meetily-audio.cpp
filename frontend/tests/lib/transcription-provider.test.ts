import { describe, expect, test } from 'bun:test';

import { TRANSCRIBE_CPP_PROVIDER } from '../../src/lib/transcription-provider';
import enCatalog from '../../src/i18n/locales/en/translation.json';
import ruCatalog from '../../src/i18n/locales/ru/translation.json';
import { getProviderCommands } from '../../src/lib/transcription-model-readiness';

describe('transcribeCpp transcript provider', () => {
  test('is the provider value saved in the transcript config', () => {
    expect(TRANSCRIBE_CPP_PROVIDER).toBe('transcribeCpp');
  });

  test('checks readiness through the transcribe.cpp model manager', () => {
    expect(getProviderCommands('transcribeCpp')).toEqual({
      initialize: 'transcribecpp_model_init',
      hasAvailableModels: 'transcribecpp_has_transcription_model',
      getAvailableModels: 'transcribecpp_get_available_models',
    });
  });

  test('translates nothing: the removed Whisper-only capability has no UI', () => {
    // Translation existed only for Whisper, so its strings are gone from both
    // interface catalogs and the transcribe.cpp notice names its own engine.
    for (const catalog of [enCatalog, ruCatalog]) {
      const selection = catalog.languageSelection as Record<string, unknown>;
      expect(selection.translationWhisperOnly).toBeUndefined();
      expect(selection.translationModeTitle).toBeUndefined();
      expect(selection.translationModeBody).toBeUndefined();
      expect(selection.translationUnavailable).toBeUndefined();
      expect(selection.parakeetTitle).toBeUndefined();
      expect(selection.parakeetBody).toBeUndefined();
      expect(selection.transcribeCppBody).toBeDefined();
    }
  });

  test('both interface languages name the provider in the settings', () => {
    const en = enCatalog.settingsTranscription.providerTranscribeCpp;
    const ru = ruCatalog.settingsTranscription.providerTranscribeCpp;
    for (const label of [en, ru]) {
      expect(label).toContain('Transcribe.cpp');
    }
  });
});
