import { describe, expect, test } from 'bun:test';

import {
  AUDIO_CPP_PROVIDER,
  TRANSCRIBE_CPP_PROVIDER,
  modelForProviderSwitch,
  supportsTranslation,
} from '../../src/lib/transcription-provider';
import enCatalog from '../../src/i18n/locales/en/translation.json';
import ruCatalog from '../../src/i18n/locales/ru/translation.json';
import {
  AUDIOCPP_COMMANDS,
  defaultSelectedTranscriptionModel,
  isTranscriptionModel,
  type AudioCppModelInfo,
} from '../../src/lib/audiocpp';
import { getProviderCommands } from '../../src/lib/transcription-model-readiness';

function model(overrides: Partial<AudioCppModelInfo> = {}): AudioCppModelInfo {
  return {
    name: 'gigaam-v3-e2e-ctc-f16',
    display_name: 'GigaAM v3 e2e CTC',
    family: 'gigaam_asr',
    task: 'asr',
    quantization: 'F16',
    filename: 'gigaam-v3-e2e-ctc-f16.gguf',
    path: '/models/audiocpp/gigaam-v3-e2e-ctc-f16.gguf',
    size_mb: 422,
    size_bytes: 442_520_704,
    description: 'Russian speech recognition',
    status: 'Available',
    ...overrides,
  };
}

const diarization = model({
  name: 'nemotron-3-diarization-bf16',
  display_name: 'Nemotron-3 Diarization',
  family: 'nemotron_3_diar',
  task: 'diar',
  quantization: 'BF16',
  filename: 'nemotron-3-diarization-bf16.gguf',
  size_mb: 190,
  size_bytes: 198_720_928,
});

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

  test('cannot translate: translation stays a Whisper-only capability', () => {
    expect(supportsTranslation('transcribeCpp')).toBeFalse();
  });

  test('does not carry another provider’s model id into transcribeCpp', () => {
    // Whisper's "large-v3-turbo" is not a transcribe.cpp model id.
    expect(modelForProviderSwitch('transcribeCpp', 'localWhisper', 'large-v3-turbo')).toBe('');
    expect(modelForProviderSwitch('transcribeCpp', 'transcribeCpp', 'parakeet-ultra-Q8_0')).toBe(
      'parakeet-ultra-Q8_0',
    );
  });

  test('both interface languages name the provider in the selector', () => {
    const en = enCatalog.settingsTranscription.providerTranscribeCpp;
    const ru = ruCatalog.settingsTranscription.providerTranscribeCpp;
    for (const label of [en, ru]) {
      expect(label).toContain('Transcribe.cpp');
    }
  });
});

describe('audioCpp transcript provider', () => {
  test('is the provider value saved in the transcript config', () => {
    expect(AUDIO_CPP_PROVIDER).toBe('audioCpp');
  });

  test('checks readiness through the audio.cpp model manager', () => {
    expect(getProviderCommands('audioCpp')).toEqual({
      initialize: 'audiocpp_model_init',
      // A downloaded diarization model is not enough to record.
      hasAvailableModels: 'audiocpp_has_transcription_model',
      getAvailableModels: 'audiocpp_get_available_models',
    });
    expect(AUDIOCPP_COMMANDS.hasTranscriptionModel).toBe('audiocpp_has_transcription_model');
  });

  test('keeps Whisper and Parakeet readiness untouched', () => {
    expect(getProviderCommands('localWhisper')?.hasAvailableModels).toBe(
      'whisper_has_available_models',
    );
    expect(getProviderCommands('parakeet')?.hasAvailableModels).toBe(
      'parakeet_has_available_models',
    );
  });
});

describe('auto-translation availability', () => {
  test('is a Whisper-only capability', () => {
    expect(supportsTranslation('localWhisper')).toBeTrue();
    expect(supportsTranslation('audioCpp')).toBeFalse();
    expect(supportsTranslation('parakeet')).toBeFalse();
  });

  test('both interface languages explain which provider can translate', () => {
    const en = enCatalog.languageSelection.translationWhisperOnly;
    const ru = ruCatalog.languageSelection.translationWhisperOnly;
    // Each names the provider that does translate and the one that does not.
    for (const message of [en, ru]) {
      expect(message).toContain('Whisper');
      expect(message).toContain('audioCpp');
    }
  });

  test('classifies every selectable local provider for translation', () => {
    // The backend mirrors this rule in `provider::check_translation_support`;
    // a mismatch would let the UI offer a mode the provider refuses.
    expect(supportsTranslation('localWhisper')).toBeTrue();
    expect(supportsTranslation('audioCpp')).toBeFalse();
    expect(supportsTranslation('parakeet')).toBeFalse();
  });
});

describe('audioCpp model selection', () => {
  test('only transcription models are selectable', () => {
    expect(isTranscriptionModel(model())).toBeTrue();
    expect(isTranscriptionModel(diarization)).toBeFalse();
  });

  test('prefers the configured installed transcription model', () => {
    const selected = defaultSelectedTranscriptionModel(
      [model({ name: 'gigaam-v3-e2e-ctc-f16' }), model({ name: 'other-asr' })],
      'other-asr',
    );
    expect(selected?.name).toBe('other-asr');
  });

  test('falls back to the installed transcription model when the configured one is missing', () => {
    const selected = defaultSelectedTranscriptionModel(
      [model({ status: 'Missing' }), model({ name: 'other-asr' })],
      'gigaam-v3-e2e-ctc-f16',
    );
    expect(selected?.name).toBe('other-asr');
  });

  test('never selects the diarization model', () => {
    expect(defaultSelectedTranscriptionModel([diarization], 'nemotron-3-diarization-bf16'))
      .toBeUndefined();
  });
  test('reports no selection when nothing transcribes yet', () => {
    expect(defaultSelectedTranscriptionModel([], undefined)).toBeUndefined();
    expect(defaultSelectedTranscriptionModel([model({ status: 'Missing' })], undefined))
      .toBeUndefined();
  });
});

describe('provider switching', () => {
  test('does not carry another provider’s model id into audioCpp', () => {
    // Whisper's "large-v3-turbo" is not an audioCpp model id.
    expect(modelForProviderSwitch('audioCpp', 'localWhisper', 'large-v3-turbo')).toBe('');
    expect(modelForProviderSwitch('localWhisper', 'audioCpp', 'gigaam-v3-e2e-ctc-f16')).toBe('');
  });

  test('keeps the model when the provider does not change', () => {
    expect(modelForProviderSwitch('audioCpp', 'audioCpp', 'gigaam-v3-e2e-ctc-f16')).toBe(
      'gigaam-v3-e2e-ctc-f16',
    );
  });
});
