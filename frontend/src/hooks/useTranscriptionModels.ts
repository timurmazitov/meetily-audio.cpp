import { useState, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { isAvailableStatus as isTranscribeCppAvailable, type TranscribeCppModelInfo } from '@/lib/transcribeCpp';

export interface ModelOption {
  provider: 'transcribeCpp';
  name: string;
  displayName: string;
  size_mb: number;
}

interface TranscriptModelConfig {
  provider?: string;
  model?: string;
}

/**
 * Custom hook for fetching and managing transcription models.
 *
 * transcribe.cpp is the only local transcription engine; the model lists of
 * the retranscribe and import dialogs come from its catalog.
 *
 * This hook centralizes the model fetching logic that was previously duplicated
 * in ImportAudioDialog and RetranscribeDialog components.
 *
 * @param transcriptModelConfig - User's saved model configuration from context
 * @returns Object containing available models, selected model key, loading state, and fetch function
 */
export function useTranscriptionModels(transcriptModelConfig: TranscriptModelConfig | undefined) {
  const [availableModels, setAvailableModels] = useState<ModelOption[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState<string>('');
  const [loadingModels, setLoadingModels] = useState(false);
  // Track whether the user has manually changed the model selection
  const userSelectedRef = useRef(false);

  // Wrap setSelectedModelKey to track user-initiated changes
  const setSelectedModelKeyWithTracking = useCallback((key: string) => {
    userSelectedRef.current = true;
    setSelectedModelKey(key);
  }, []);

  const fetchModels = useCallback(async () => {
    setLoadingModels(true);
    const allModels: ModelOption[] = [];

    // Fetch transcribeCpp models. The catalog is ASR-only, so every installed
    // model can produce a transcript; a partial download must never be handed
    // to a batch job, hence the Available-only filter.
    try {
      const transcribeCppModels = await invoke<TranscribeCppModelInfo[]>('transcribecpp_get_available_models');
      const availableTranscribeCpp = transcribeCppModels
        .filter((m) => isTranscribeCppAvailable(m.status))
        .map((m) => ({
          provider: 'transcribeCpp' as const,
          name: m.name,
          displayName: `🧠 Transcribe.cpp: ${m.display_name}`,
          size_mb: m.size_mb,
        }));
      allModels.push(...availableTranscribeCpp);
    } catch (err) {
      console.error('Failed to fetch transcribeCpp models:', err);
    }

    setAvailableModels(allModels);

    // Set default model based on user's saved configuration
    const configuredModel = transcriptModelConfig?.model || '';

    // Try to match the configured model. The repository soft-migrates removed
    // providers on read and the catalog is the only model list, so a name
    // match is enough to prefer the configured model.
    const configuredMatch = allModels.find((m) => m.name === configuredModel);

    // Only set default model if user hasn't manually selected one
    if (!userSelectedRef.current) {
      if (configuredMatch) {
        // Use the configured model if available
        setSelectedModelKey(`${configuredMatch.provider}:${configuredMatch.name}`);
      } else if (allModels.length > 0) {
        // Fall back to first available model
        setSelectedModelKey(`${allModels[0].provider}:${allModels[0].name}`);
      }
    }

    setLoadingModels(false);
  }, [transcriptModelConfig]);

  // Reset user selection tracking (call when dialog opens fresh)
  const resetSelection = useCallback(() => {
    userSelectedRef.current = false;
  }, []);

  return {
    availableModels,
    selectedModelKey,
    setSelectedModelKey: setSelectedModelKeyWithTracking,
    loadingModels,
    fetchModels,
    resetSelection,
  };
}
