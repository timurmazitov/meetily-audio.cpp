/**
 * Shared facts about the selectable transcription providers.
 *
 * Kept free of Tauri imports so plain unit tests and components can import it.
 */

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
