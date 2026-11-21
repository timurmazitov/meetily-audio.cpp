import { TRANSCRIBE_CPP_PROVIDER } from './transcription-provider';

export interface ModelWithStatus {
  status?: unknown;
}

interface ProviderCommands {
  initialize: string;
  hasAvailableModels: string;
  getAvailableModels: string;
}

const PROVIDER_COMMANDS: Record<string, ProviderCommands> = {
  [TRANSCRIBE_CPP_PROVIDER]: {
    initialize: 'transcribecpp_model_init',
    // The transcribe.cpp catalog is ASR-only: only an installed model can
    // produce a transcript, so the gate is the transcription-model command.
    hasAvailableModels: 'transcribecpp_has_transcription_model',
    getAvailableModels: 'transcribecpp_get_available_models',
  },
};

/**
 * Commands that check whether the provider has a usable model.
 *
 * transcribe.cpp is the only local engine; any other provider string —
 * including the removed `localWhisper`/`parakeet`/`audioCpp` values still
 * found in saved configs that predate the soft migration — maps to the
 * transcribe.cpp commands, mirroring the repository's read-side normalization.
 */
export function getProviderCommands(provider: string): ProviderCommands {
  return PROVIDER_COMMANDS[provider] ?? PROVIDER_COMMANDS[TRANSCRIBE_CPP_PROVIDER];
}

export function hasDownloadingModel(models: ModelWithStatus[]): boolean {
  return models.some(({ status }) => (
    status === 'Downloading'
    || (status !== null && typeof status === 'object' && 'Downloading' in status)
  ));
}
