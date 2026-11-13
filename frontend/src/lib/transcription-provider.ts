/**
 * Shared facts about the selectable transcription providers.
 *
 * Kept free of Tauri imports so plain unit tests and components can import it.
 */

/** Provider values the transcript settings can persist. */
export type TranscriptProvider =
  | 'localWhisper'
  | 'parakeet'
  | 'audioCpp'
  | 'deepgram'
  | 'elevenLabs'
  | 'groq'
  | 'openai';

/** Watched by name in the settings modal, saved by value in the database. */
export const AUDIO_CPP_PROVIDER = 'audioCpp';

/**
 * Shown wherever automatic translation is offered for a provider that cannot
 * translate. audioCpp (GigaAM) and Parakeet transcribe in the language they
 * hear; only the local Whisper decoder has a translate mode.
 */
export const TRANSLATION_WHISPER_ONLY_MESSAGE =
  'Automatic translation is only available with the Whisper provider. audioCpp transcribes in the language of the recording.';

/** Whether a provider can translate audio to English. */
export function supportsTranslation(provider: string): boolean {
  return provider === 'localWhisper';
}

/** Whether a provider runs locally (no API key, no manual model id). */
export function isLocalProvider(provider: string): boolean {
  return provider === 'localWhisper' || provider === 'parakeet' || provider === AUDIO_CPP_PROVIDER;
}

/**
 * Providers offered in the transcript settings, in display order.
 *
 * The catalogue-backed providers (Whisper, Parakeet, audioCpp) have no free
 * model id: their models are managed by a dedicated manager component.
 */
export interface TranscriptProviderOption {
  value: TranscriptProvider;
  label: string;
  /** Catalogue-backed: the model is picked from a model manager. */
  catalogBacked: boolean;
  /** Free-form model ids the provider accepts (empty when catalogue-backed). */
  modelIds: string[];
}

export const TRANSCRIPT_PROVIDER_OPTIONS: TranscriptProviderOption[] = [
  {
    value: 'parakeet',
    label: '⚡ Parakeet (Recommended - Real-time / Accurate)',
    catalogBacked: true,
    modelIds: [],
  },
  {
    value: 'localWhisper',
    label: '🏠 Local Whisper (High Accuracy)',
    catalogBacked: true,
    modelIds: [],
  },
  {
    value: 'audioCpp',
    label: '🎙️ audioCpp (GigaAM — Russian)',
    catalogBacked: true,
    modelIds: [],
  },
  {
    value: 'deepgram',
    label: '☁️ Deepgram (Backup)',
    catalogBacked: false,
    modelIds: ['nova-2-phonecall'],
  },
  {
    value: 'elevenLabs',
    label: '☁️ ElevenLabs',
    catalogBacked: false,
    modelIds: ['eleven_multilingual_v2'],
  },
  {
    value: 'groq',
    label: '☁️ Groq',
    catalogBacked: false,
    modelIds: ['llama-3.3-70b-versatile'],
  },
  {
    value: 'openai',
    label: '☁️ OpenAI',
    catalogBacked: false,
    modelIds: ['gpt-4o'],
  },
];

/**
 * The language value to send for a provider. A provider without translation
 * must never receive `auto-translate`, even if the saved preference carries it
 * from a previous Whisper session.
 */
export function languageForProvider(provider: string, selectedLanguage: string): string {
  if (selectedLanguage === 'auto-translate' && !supportsTranslation(provider)) {
    return 'auto';
  }
  return selectedLanguage;
}

/**
 * The model id to save when the user switches provider.
 *
 * A model id only means something inside its provider's catalogue: switching
 * from Whisper to audioCpp must not persist "large-v3-turbo" as an audioCpp
 * model. The empty string tells the backend to resolve the configured
 * provider's own model (it falls back to the first installed transcription
 * model), and the settings model manager highlights that same model.
 */
export function modelForProviderSwitch(
  nextProvider: string,
  previousProvider: string,
  previousModel: string,
): string {
  return nextProvider === previousProvider ? previousModel : '';
}
