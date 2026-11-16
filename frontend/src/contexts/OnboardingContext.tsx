'use client';

import React, { createContext, useContext, useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PermissionStatus, OnboardingPermissions } from '@/types/onboarding';
import { resolveOnboardingSummaryModelStatus } from '@/lib/onboarding-summary-model';
import {
  AUDIOCPP_DIARIZATION_MODEL,
  AUDIOCPP_TRANSCRIPTION_MODEL,
  applyAudioCppCompleteEvent,
  applyAudioCppErrorEvent,
  applyAudioCppProgressEvent,
  hasActiveAudioCppDownload,
  initialAudioCppModelStates,
  isAudioCppModelInstalled,
  reconcileAudioCppModelList,
  setAudioCppModelDownloaded,
  setAudioCppModelsDownloaded,
  startAudioCppModelDownload,
  type AudioCppModelDownloadStatus,
  type AudioCppModelProgressInfo,
  type AudioCppModelStates,
} from '@/lib/onboarding-audiocpp-status';
import {
  AUDIOCPP_COMMANDS,
  AUDIOCPP_EVENTS,
  type AudioCppDownloadCompleteEvent,
  type AudioCppDownloadErrorEvent,
  type AudioCppDownloadProgressEvent,
  type AudioCppModelInfo,
} from '@/lib/audiocpp';

interface OnboardingStatus {
  version: string;
  completed: boolean;
  current_step: number;
  model_status: {
    gigaam: string;
    diarization: string;
    summary: string;
    selected_summary_model?: string;
  };
  last_updated: string;
}

interface SummaryModelProgressInfo {
  percent: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
}

interface OnboardingContextType {
  currentStep: number;
  transcriptionModelDownloaded: boolean;
  transcriptionModelProgress: number;
  transcriptionModelProgressInfo: AudioCppModelProgressInfo;
  /** Card status, including `error`/`cancelled`, so the step can offer a retry. */
  transcriptionModelStatus: AudioCppModelDownloadStatus;
  transcriptionModelError: string | null;
  diarizationModelDownloaded: boolean;
  diarizationModelProgress: number;
  diarizationModelProgressInfo: AudioCppModelProgressInfo;
  /** Card status, including `error`/`cancelled`, so the step can offer a retry. */
  diarizationModelStatus: AudioCppModelDownloadStatus;
  diarizationModelError: string | null;
  summaryModelDownloaded: boolean;
  summaryModelProgress: number;
  summaryModelProgressInfo: SummaryModelProgressInfo;
  selectedSummaryModel: string;
  recommendedSummaryModel: string;
  databaseExists: boolean;
  isBackgroundDownloading: boolean;
  // Permissions
  permissions: OnboardingPermissions;
  permissionsSkipped: boolean;
  // Navigation
  goToStep: (step: number) => void;
  goNext: () => void;
  goPrevious: () => void;
  // Setters
  setTranscriptionModelDownloaded: (value: boolean) => void;
  setDiarizationModelDownloaded: (value: boolean) => void;
  setSummaryModelDownloaded: (value: boolean) => void;
  setSelectedSummaryModel: (value: string) => void;
  setDatabaseExists: (value: boolean) => void;
  setPermissionStatus: (permission: keyof OnboardingPermissions, status: PermissionStatus) => void;
  setPermissionsSkipped: (skipped: boolean) => void;
  completeOnboarding: () => Promise<void>;
  startBackgroundDownloads: (options: StartBackgroundDownloadsOptions) => Promise<void>;
}

interface StartBackgroundDownloadsOptions {
  includeAudioCppTranscription: boolean;
  includeAudioCppDiarization: boolean;
  includeSummary: boolean;
  summaryModel?: string;
}

const OnboardingContext = createContext<OnboardingContextType | undefined>(undefined);

export function OnboardingProvider({ children }: { children: React.ReactNode }) {
  const [currentStep, setCurrentStep] = useState(1);
  const [completed, setCompleted] = useState(false);
  // audio.cpp model cards: one source of truth for the downloaded flag,
  // percentage and MB/speed of the two onboarding downloads.
  const [audioCppModelStates, setAudioCppModelStates] =
    useState<AudioCppModelStates>(initialAudioCppModelStates);
  const transcriptionModelDownloaded =
    audioCppModelStates.transcription.status === 'completed';
  const transcriptionModelProgress = audioCppModelStates.transcription.progress.percent;
  const transcriptionModelProgressInfo = audioCppModelStates.transcription.progress;
  const transcriptionModelStatus = audioCppModelStates.transcription.status;
  const transcriptionModelError = audioCppModelStates.transcription.error ?? null;
  const diarizationModelDownloaded = audioCppModelStates.diarization.status === 'completed';
  const diarizationModelProgress = audioCppModelStates.diarization.progress.percent;
  const diarizationModelProgressInfo = audioCppModelStates.diarization.progress;
  const diarizationModelStatus = audioCppModelStates.diarization.status;
  const diarizationModelError = audioCppModelStates.diarization.error ?? null;
  const [summaryModelDownloaded, setSummaryModelDownloaded] = useState(false);
  const [summaryModelProgress, setSummaryModelProgress] = useState(0);
  const [summaryModelProgressInfo, setSummaryModelProgressInfo] = useState<SummaryModelProgressInfo>({
    percent: 0,
    downloadedMb: 0,
    totalMb: 0,
    speedMbps: 0,
  });
  const [selectedSummaryModel, setSelectedSummaryModel] = useState<string>('');
  const [recommendedSummaryModel, setRecommendedSummaryModel] = useState<string>('');
  const [databaseExists, setDatabaseExists] = useState(false);
  const [isBackgroundDownloading, setIsBackgroundDownloading] = useState(false);

  // Permissions state
  const [permissions, setPermissions] = useState<OnboardingPermissions>({
    microphone: 'not_determined',
    systemAudio: 'not_determined',
    screenRecording: 'not_determined',
  });
  const [permissionsSkipped, setPermissionsSkipped] = useState(false);

  const saveTimeoutRef = useRef<NodeJS.Timeout>();

  const initializeSummaryModelSelection = async (preferredModel = selectedSummaryModel) => {
    try {
      const recommendedModel = await invoke<string>('builtin_ai_get_recommended_model');
      setRecommendedSummaryModel(recommendedModel);
      const modelToCheck = preferredModel || recommendedModel;
      setSelectedSummaryModel(modelToCheck);

      const selectedModelReady = await invoke<boolean>('builtin_ai_is_model_ready', {
        modelName: modelToCheck,
        refresh: true,
      });
      const resolved = resolveOnboardingSummaryModelStatus({
        selectedModel: preferredModel,
        recommendedModel,
        selectedModelReady,
      });

      setSelectedSummaryModel(resolved.selectedSummaryModel);
      setSummaryModelDownloaded(resolved.summaryModelDownloaded);
      console.log('[OnboardingContext] Set recommended model:', resolved.selectedSummaryModel);

      return resolved;
    } catch (error) {
      console.error('[OnboardingContext] Failed to initialize summary model:', error);
      return null;
    }
  };

  const requestSummaryModelDownload = (modelName: string) => {
    console.log('[OnboardingContext] Starting Summary Model download');
    invoke('builtin_ai_download_model', { modelName })
      .catch(err => {
        if (String(err).includes('Download already in progress')) {
          return;
        }
        console.error('[OnboardingContext] Summary Model download failed:', err);
      });
  };

  // Load status on mount and initialize database
  useEffect(() => {
    loadOnboardingStatus();
    checkDatabaseStatus();
    initializeDatabaseInBackground();
  }, []);

  // Initialize database silently in background (moved from SetupOverviewStep)
  const initializeDatabaseInBackground = async () => {
    try {
      console.log('[OnboardingContext] Starting background database initialization');
      const isFirstLaunch = await invoke<boolean>('check_first_launch');

      if (!isFirstLaunch) {
        console.log('[OnboardingContext] Database exists, skipping initialization');
        setDatabaseExists(true);
        return;
      }

      // First launch - attempt auto-detection and import
      await performAutoDetection();
    } catch (error) {
      console.error('[OnboardingContext] Database initialization failed:', error);
      // Don't throw - database init failure shouldn't block onboarding
    }
  };

  const performAutoDetection = async () => {
    // Check Homebrew (macOS only)
    if (typeof navigator !== 'undefined' && navigator.platform?.toLowerCase().includes('mac')) {
      const homebrewDbPath = '/usr/local/var/meetily/meeting_minutes.db';
      try {
        const homebrewCheck = await invoke<{ exists: boolean; size: number } | null>(
          'check_homebrew_database',
          { path: homebrewDbPath }
        );

        if (homebrewCheck?.exists) {
          console.log('[OnboardingContext] Found Homebrew database, importing');
          await invoke('import_and_initialize_database', { legacyDbPath: homebrewDbPath });
          setDatabaseExists(true);
          return;
        }
      } catch (e) {
        console.log('[OnboardingContext] Homebrew check failed, continuing:', e);
      }
    }

    // Check default legacy database location
    try {
      const legacyPath = await invoke<string | null>('check_default_legacy_database');
      if (legacyPath) {
        console.log('[OnboardingContext] Found legacy database, importing');
        await invoke('import_and_initialize_database', { legacyDbPath: legacyPath });
        setDatabaseExists(true);
        return;
      }
    } catch (e) {
      console.log('[OnboardingContext] Legacy check failed, continuing:', e);
    }

    // No legacy database found - initialize fresh
    console.log('[OnboardingContext] No legacy database found, initializing fresh');
    await invoke('initialize_fresh_database');
    setDatabaseExists(true);
  };

  const isCompletingRef = useRef(false);

  // Auto-save on state change (debounced)
  useEffect(() => {
    if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);

    // Don't auto-save if completed (to avoid overwriting completion status)
    // Also don't auto-save if we are currently in the process of completing
    if (completed || isCompletingRef.current) return;

    saveTimeoutRef.current = setTimeout(() => {
      saveOnboardingStatus();
    }, 1000);

    return () => {
      if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);
    };
  }, [currentStep, transcriptionModelDownloaded, diarizationModelDownloaded, summaryModelDownloaded, completed]);

  // Listen to audio.cpp model download events (transcription + diarization)
  useEffect(() => {
    const unlistenProgress = listen<AudioCppDownloadProgressEvent>(
      AUDIOCPP_EVENTS.progress,
      (event) => {
        if (event.payload.status === 'cancelled') {
          setIsBackgroundDownloading(false);
        }
        setAudioCppModelStates(states => applyAudioCppProgressEvent(states, event.payload));
      }
    );

    const unlistenComplete = listen<AudioCppDownloadCompleteEvent>(
      AUDIOCPP_EVENTS.complete,
      (event) => {
        setAudioCppModelStates(states => applyAudioCppCompleteEvent(states, event.payload));
      }
    );

    const unlistenError = listen<AudioCppDownloadErrorEvent>(
      AUDIOCPP_EVENTS.error,
      (event) => {
        console.error('audio.cpp model download error:', event.payload.modelName, event.payload.error);
        setAudioCppModelStates(states => applyAudioCppErrorEvent(states, event.payload));
      }
    );

    return () => {
      unlistenProgress.then(fn => fn());
      unlistenComplete.then(fn => fn());
      unlistenError.then(fn => fn());
    };
  }, []);

  // Listen to summary model (Built-in AI) download progress
  useEffect(() => {
    const unlisten = listen<{
      model: string;
      progress: number;
      downloaded_mb?: number;
      total_mb?: number;
      speed_mbps?: number;
      status: string;
    }>(
      'builtin-ai-download-progress',
      (event) => {
        const { model, progress, downloaded_mb, total_mb, speed_mbps, status } = event.payload;
        if (selectedSummaryModel && model === selectedSummaryModel) {
          setSummaryModelProgress(progress);
          setSummaryModelProgressInfo({
            percent: progress,
            downloadedMb: downloaded_mb ?? 0,
            totalMb: total_mb ?? 0,
            speedMbps: speed_mbps ?? 0,
          });
          if (status === 'completed' || progress >= 100) {
            setSummaryModelDownloaded(true);
          }
        }
      }
    );

    return () => {
      unlisten.then(fn => fn());
    };
  }, [selectedSummaryModel]);

  const checkDatabaseStatus = async () => {
    try {
      const isFirstLaunch = await invoke<boolean>('check_first_launch');
      setDatabaseExists(!isFirstLaunch);
      console.log('[OnboardingContext] Database exists:', !isFirstLaunch);
    } catch (error) {
      console.error('[OnboardingContext] Failed to check database status:', error);
      setDatabaseExists(false);
    }
  };

  const loadOnboardingStatus = async () => {
    try {
      const status = await invoke<OnboardingStatus | null>('get_onboarding_status');
      if (status) {
        console.log('[OnboardingContext] Loaded saved status:', status);

        if (status.completed) {
          setCurrentStep(status.current_step);
          setCompleted(true);
          setAudioCppModelStates(states =>
            setAudioCppModelsDownloaded(states, {
              transcription: status.model_status.gigaam === 'downloaded',
              diarization: status.model_status.diarization === 'downloaded',
            })
          );
          setSummaryModelDownloaded(status.model_status.summary === 'downloaded');
          if (status.model_status.selected_summary_model) {
            setSelectedSummaryModel(status.model_status.selected_summary_model);
          }
          console.log('[OnboardingContext] Restored completed onboarding status without model verification');
          return;
        }

        // Don't trust saved status - verify actual model status on disk
        const verifiedStatus = await verifyModelStatus(status);

        setCurrentStep(verifiedStatus.currentStep);
        setCompleted(verifiedStatus.completed);
        setAudioCppModelStates(states =>
          setAudioCppModelsDownloaded(states, {
            transcription: verifiedStatus.transcriptionModelDownloaded,
            diarization: verifiedStatus.diarizationModelDownloaded,
          })
        );
        setSummaryModelDownloaded(verifiedStatus.summaryModelDownloaded);
        if (verifiedStatus.selectedSummaryModel) {
          setSelectedSummaryModel(verifiedStatus.selectedSummaryModel);
        }

        console.log('[OnboardingContext] Verified status:', verifiedStatus);

        // Check if any downloads are active to restore isBackgroundDownloading state
        await checkActiveDownloads();
      } else {
        await initializeSummaryModelSelection();
      }
    } catch (error) {
      console.error('[OnboardingContext] Failed to load onboarding status:', error);
    }
  };

  // Verify that models actually exist on disk, not just trust saved JSON
  const verifyModelStatus = async (savedStatus: OnboardingStatus) => {
    let transcriptionModelDownloaded = false;
    let diarizationModelDownloaded = false;
    let summaryModelDownloaded = false;
    let selectedSummaryModel = '';

    // Verify the audio.cpp transcription model (ASR gate) and diarization
    // model (advisory: a missing Nemotron must not block onboarding).
    try {
      await invoke(AUDIOCPP_COMMANDS.init);
      transcriptionModelDownloaded = await invoke<boolean>(
        AUDIOCPP_COMMANDS.hasTranscriptionModel
      );
      const models = await invoke<AudioCppModelInfo[]>(
        AUDIOCPP_COMMANDS.getAvailableModels
      );
      diarizationModelDownloaded = isAudioCppModelInstalled(
        models,
        AUDIOCPP_DIARIZATION_MODEL
      );
      console.log(
        '[OnboardingContext] audio.cpp models verified on disk:',
        { transcriptionModelDownloaded, diarizationModelDownloaded }
      );
    } catch (error) {
      console.warn('[OnboardingContext] Failed to verify audio.cpp models:', error);
      transcriptionModelDownloaded = false;
      diarizationModelDownloaded = false;
    }

    // Verify the selected/recommended Summary model exists on disk.
    try {
      const recommendedModel = await invoke<string>('builtin_ai_get_recommended_model');
      setRecommendedSummaryModel(recommendedModel);
      const savedSelectedModel = savedStatus.model_status.selected_summary_model || '';
      const modelToCheck = savedSelectedModel || recommendedModel;
      const selectedModelReady = await invoke<boolean>('builtin_ai_is_model_ready', {
        modelName: modelToCheck,
        refresh: true,
      });
      const resolved = resolveOnboardingSummaryModelStatus({
        selectedModel: savedSelectedModel,
        recommendedModel,
        selectedModelReady,
      });
      selectedSummaryModel = resolved.selectedSummaryModel;
      summaryModelDownloaded = resolved.summaryModelDownloaded;
      console.log('[OnboardingContext] Summary model verified on disk:', summaryModelDownloaded, 'model:', selectedSummaryModel);
    } catch (error) {
      console.warn('[OnboardingContext] Failed to verify Summary model:', error);
      summaryModelDownloaded = false;
    }

    // Determine the correct step based on verified status
    // New simplified flow: Step 1: Welcome, Step 2: Setup Overview, Step 3: Download Progress, Step 4: Permissions (macOS)
    let currentStep = savedStatus.current_step;
    let completed = savedStatus.completed;

    // Clamp step to new max (4)
    if (currentStep > 4) {
      currentStep = 3; // Go to download progress step
    }

    // Trust the completed status - don't revert based on model downloads
    // Downloads continue in background; user stays in main app regardless
    return {
      currentStep,
      completed,
      transcriptionModelDownloaded,
      diarizationModelDownloaded,
      summaryModelDownloaded,
      selectedSummaryModel,
    };
  };

  const saveOnboardingStatus = async () => {
    // Safety check: if we are in the process of completing, DO NOT save
    // This prevents a race condition where a download completion event triggers a save
    // that overwrites the "completed" status set by completeOnboarding
    if (isCompletingRef.current) {
      console.log('[OnboardingContext] Skipping saveOnboardingStatus because completion is in progress');
      return;
    }

    try {
      await invoke('save_onboarding_status_cmd', {
        status: {
          version: '1.0',
          completed: completed,
          current_step: currentStep,
          model_status: {
            gigaam: transcriptionModelDownloaded ? 'downloaded' : 'not_downloaded',
            diarization: diarizationModelDownloaded ? 'downloaded' : 'not_downloaded',
            summary: summaryModelDownloaded ? 'downloaded' : 'not_downloaded',
            selected_summary_model: selectedSummaryModel || undefined,
          },
          last_updated: new Date().toISOString(),
        },
      });
    } catch (error) {
      console.error('[OnboardingContext] Failed to save onboarding status:', error);
    }
  };

  const completeOnboarding = async () => {
    try {
      // Set completion flag to prevent race conditions with auto-save
      isCompletingRef.current = true;

      // Clear any pending auto-saves
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
        saveTimeoutRef.current = undefined;
      }

      let modelToSave = selectedSummaryModel;
      if (!modelToSave) {
        modelToSave = await invoke<string>('builtin_ai_get_recommended_model');
        setSelectedSummaryModel(modelToSave);
      }

      const selectedModelReady = await invoke<boolean>('builtin_ai_is_model_ready', {
        modelName: modelToSave,
        refresh: true,
      });
      setSummaryModelDownloaded(selectedModelReady);
      if (!selectedModelReady) {
        requestSummaryModelDownload(modelToSave);
      }

      // Mark both audio.cpp models as installed. The completion command writes
      // the audioCpp transcript config; the ASR gate is re-checked here instead
      // of trusting the card state alone.
      try {
        await invoke(AUDIOCPP_COMMANDS.init);
        const transcriptionReady = await invoke<boolean>(
          AUDIOCPP_COMMANDS.hasTranscriptionModel
        );
        if (!transcriptionReady) {
          console.warn('[OnboardingContext] audio.cpp transcription model is not installed on completion');
        }
        setAudioCppModelStates(states =>
          setAudioCppModelsDownloaded(states, { transcription: true, diarization: true })
        );
      } catch (error) {
        console.warn('[OnboardingContext] Failed to verify audio.cpp models on completion:', error);
      }

      // Onboarding always uses builtin-ai with selected model
      await invoke('complete_onboarding', {
        model: modelToSave,
      });
      setCompleted(true);
      console.log('[OnboardingContext] Onboarding completed with model:', modelToSave);

      // Reset the flag so subsequent state updates can be saved
      isCompletingRef.current = false;
    } catch (error) {
      console.error('[OnboardingContext] Failed to complete onboarding:', error);
      isCompletingRef.current = false; // Reset flag on error
      throw error; // Re-throw so PermissionsStep can handle it
    }
  };

  // Start background downloads for models.
  const startBackgroundDownloads = async ({
    includeAudioCppTranscription,
    includeAudioCppDiarization,
    includeSummary,
    summaryModel,
  }: StartBackgroundDownloadsOptions) => {
    console.log('[OnboardingContext] Starting background downloads:', {
      includeAudioCppTranscription,
      includeAudioCppDiarization,
      includeSummary,
      summaryModel,
    });

    try {
      // A model already in flight (or installed) must not be requested again:
      // the native downloader rejects a duplicate request for the same model.
      const shouldStartTranscription =
        includeAudioCppTranscription &&
        !transcriptionModelDownloaded &&
        transcriptionModelStatus !== 'downloading';
      const shouldStartDiarization =
        includeAudioCppDiarization &&
        !diarizationModelDownloaded &&
        diarizationModelStatus !== 'downloading';
      const shouldStartSummary = includeSummary && !summaryModelDownloaded && !!summaryModel;

      if (!shouldStartTranscription && !shouldStartDiarization && !shouldStartSummary) {
        if (includeSummary && !summaryModelDownloaded && !summaryModel) {
          console.warn('[OnboardingContext] Summary Model download skipped until recommendation is loaded');
        }
        return;
      }

      setIsBackgroundDownloading(true);

      // audio.cpp downloads need the model manager, which is created lazily.
      if (shouldStartTranscription || shouldStartDiarization) {
        await invoke(AUDIOCPP_COMMANDS.init);
      }

      // Transcription model first (speech recognition - always required).
      if (shouldStartTranscription) {
        console.log('[OnboardingContext] Starting audio.cpp transcription model download');
        setAudioCppModelStates(states => startAudioCppModelDownload(states, 'transcription'));
        invoke(AUDIOCPP_COMMANDS.downloadModel, { modelName: AUDIOCPP_TRANSCRIPTION_MODEL })
          .catch(err => {
            console.error('[OnboardingContext] audio.cpp transcription download failed:', err);
            if (String(err).includes('Download already in progress')) return;
            setAudioCppModelStates(states =>
              applyAudioCppErrorEvent(states, {
                modelName: AUDIOCPP_TRANSCRIPTION_MODEL,
                error: String(err),
              })
            );
          });
      }

      // Diarization downloads in the background; it never blocks Continue.
      if (shouldStartDiarization) {
        console.log('[OnboardingContext] Starting audio.cpp diarization model download');
        setAudioCppModelStates(states => startAudioCppModelDownload(states, 'diarization'));
        invoke(AUDIOCPP_COMMANDS.downloadModel, { modelName: AUDIOCPP_DIARIZATION_MODEL })
          .catch(err => {
            console.error('[OnboardingContext] audio.cpp diarization download failed:', err);
            if (String(err).includes('Download already in progress')) return;
            setAudioCppModelStates(states =>
              applyAudioCppErrorEvent(states, {
                modelName: AUDIOCPP_DIARIZATION_MODEL,
                error: String(err),
              })
            );
          });
      }

      // Start selected Summary Model download immediately so completion cannot race the request.
      if (shouldStartSummary && summaryModel) {
        requestSummaryModelDownload(summaryModel);
      }
    } catch (error) {
      console.error('[OnboardingContext] Failed to start background downloads:', error);
      setIsBackgroundDownloading(false);
      throw error;
    }
  };

  // Check if any models are currently downloading (for re-entry)
  const checkActiveDownloads = async () => {
    try {
      await invoke(AUDIOCPP_COMMANDS.init);
      const models = await invoke<AudioCppModelInfo[]>(AUDIOCPP_COMMANDS.getAvailableModels);

      setAudioCppModelStates(states => reconcileAudioCppModelList(states, models));

      if (hasActiveAudioCppDownload(models)) {
        console.log('[OnboardingContext] Detected active background downloads on mount');
        setIsBackgroundDownloading(true);
      }

      // Also check for Built-in AI downloads if possible (though less critical as the transcription model is the main blocker)

    } catch (error) {
      console.warn('[OnboardingContext] Failed to check active downloads:', error);
    }
  };

  const setTranscriptionModelDownloaded = useCallback((value: boolean) => {
    setAudioCppModelStates(states =>
      setAudioCppModelDownloaded(states, 'transcription', value)
    );
  }, []);

  const setDiarizationModelDownloaded = useCallback((value: boolean) => {
    setAudioCppModelStates(states =>
      setAudioCppModelDownloaded(states, 'diarization', value)
    );
  }, []);

  const setPermissionStatus = useCallback((permission: keyof OnboardingPermissions, status: PermissionStatus) => {
    setPermissions((prev: OnboardingPermissions) => ({
      ...prev,
      [permission]: status,
    }));
  }, []);

  const goToStep = useCallback((step: number) => {
    setCurrentStep(Math.max(1, Math.min(step, 4)));
  }, []);

  const goNext = useCallback(() => {
    setCurrentStep((prev: number) => {
      const next = prev + 1;
      // Don't go past step 4
      return Math.min(next, 4);
    });
  }, []);

  const goPrevious = useCallback(() => {
    setCurrentStep((prev: number) => {
      const previous = prev - 1;
      // Don't go below step 1
      return Math.max(previous, 1);
    });
  }, []);

  return (
    <OnboardingContext.Provider
      value={{
        currentStep,
        transcriptionModelDownloaded,
        transcriptionModelProgress,
        transcriptionModelProgressInfo,
        transcriptionModelStatus,
        transcriptionModelError,
        diarizationModelDownloaded,
        diarizationModelProgress,
        diarizationModelProgressInfo,
        diarizationModelStatus,
        diarizationModelError,
        summaryModelDownloaded,
        summaryModelProgress,
        summaryModelProgressInfo,
        selectedSummaryModel,
        recommendedSummaryModel,
        databaseExists,
        isBackgroundDownloading,
        permissions,
        permissionsSkipped,
        goToStep,
        goNext,
        goPrevious,
        setTranscriptionModelDownloaded,
        setDiarizationModelDownloaded,
        setSummaryModelDownloaded,
        setSelectedSummaryModel,
        setDatabaseExists,
        setPermissionStatus,
        setPermissionsSkipped,
        completeOnboarding,
        startBackgroundDownloads,
      }}
    >
      {children}
    </OnboardingContext.Provider>
  );
}

export function useOnboarding() {
  const context = useContext(OnboardingContext);
  if (!context) {
    throw new Error('useOnboarding must be used within OnboardingProvider');
  }
  return context;
}
