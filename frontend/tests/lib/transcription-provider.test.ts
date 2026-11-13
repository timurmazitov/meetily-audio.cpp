import { describe, expect, test } from 'bun:test';

import {
  AUDIO_CPP_PROVIDER,
  TRANSLATION_WHISPER_ONLY_MESSAGE,
  isLocalProvider,
  languageForProvider,
  modelForProviderSwitch,
  supportsTranslation,
  TRANSCRIPT_PROVIDER_OPTIONS,
} from '../../src/lib/transcription-provider';
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

describe('audioCpp transcript provider', () => {
  test('is the provider value saved in the transcript config', () => {
    expect(AUDIO_CPP_PROVIDER).toBe('audioCpp');
  });

  test('runs locally, so the settings never ask it for an API key', () => {
    expect(isLocalProvider('audioCpp')).toBeTrue();
    expect(isLocalProvider('localWhisper')).toBeTrue();
    expect(isLocalProvider('parakeet')).toBeTrue();
    expect(isLocalProvider('deepgram')).toBeFalse();
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

  test('explains in the UI message which provider can translate', () => {
    expect(TRANSLATION_WHISPER_ONLY_MESSAGE.toLowerCase()).toContain('only available');
    expect(TRANSLATION_WHISPER_ONLY_MESSAGE).toContain('Whisper');
    expect(TRANSLATION_WHISPER_ONLY_MESSAGE).toContain('audioCpp');
  });

  test('never sends auto-translate to a provider that cannot translate', () => {
    // A stale Whisper preference must not silently become untranslated audio.
    expect(languageForProvider('audioCpp', 'auto-translate')).toBe('auto');
    expect(languageForProvider('parakeet', 'auto-translate')).toBe('auto');
    expect(languageForProvider('audioCpp', 'ru')).toBe('ru');
    expect(languageForProvider('audioCpp', 'auto')).toBe('auto');
    expect(languageForProvider('localWhisper', 'auto-translate')).toBe('auto-translate');
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

describe('transcript settings provider list', () => {
  test('offers audioCpp next to Parakeet and Whisper', () => {
    const values = TRANSCRIPT_PROVIDER_OPTIONS.map(option => option.value);
    expect(values).toContain('audioCpp');
    expect(values).toContain('localWhisper');
    expect(values).toContain('parakeet');
  });

  test('treats audioCpp as catalogue-backed so no free-form model id is demanded', () => {
    const audioCpp = TRANSCRIPT_PROVIDER_OPTIONS.find(option => option.value === 'audioCpp')!;
    expect(audioCpp.catalogBacked).toBeTrue();
    expect(audioCpp.modelIds).toEqual([]);
    expect(audioCpp.label).toContain('GigaAM');
  });

  test('keeps the API-key providers untouched', () => {
    for (const provider of ['deepgram', 'elevenLabs', 'groq', 'openai'] as const) {
      const option = TRANSCRIPT_PROVIDER_OPTIONS.find(candidate => candidate.value === provider)!;
      expect(option.catalogBacked).toBeFalse();
      expect(option.modelIds.length).toBeGreaterThan(0);
    }
  });
});
