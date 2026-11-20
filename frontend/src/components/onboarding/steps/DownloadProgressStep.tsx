import React, { useEffect, useState, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Mic, Users, Sparkles, Check, Loader2, Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { toast } from 'sonner';
import { motion, AnimatePresence } from 'framer-motion';
import { getSummaryModelSizeMb } from '@/lib/onboarding-summary-model';
import { formatDecimal, formatMegabytes } from '@/lib/format-locale';
import type { AppLanguageCode } from '@/i18n';
import { AUDIOCPP_COMMANDS } from '@/lib/audiocpp';
import { TRANSCRIBECPP_COMMANDS } from '@/lib/transcribeCpp';
import {
  onboardingRetryModelName,
  isContinueEnabled,
  onboardingDownloadRetryTarget,
  resolveTranscriptionGate,
  shouldShowBackgroundDownloadNotice,
  type AudioCppModelCardState,
  type AudioCppModelDownloadStatus,
  type AudioCppModelRole,
  type AudioCppModelStates,
  type OnboardingDownloadCard,
} from '@/lib/onboarding-audiocpp-status';

/** One download card's visible state; shared by the audio.cpp and summary cards. */
interface DownloadState {
  status: AudioCppModelDownloadStatus;
  progress: {
    percent: number;
    downloadedMb: number;
    totalMb: number;
    speedMbps: number;
  };
  error?: string;
}

function cardFromContext(
  status: AudioCppModelDownloadStatus,
  progress: DownloadState['progress'],
  error: string | null
): AudioCppModelCardState {
  return {
    status,
    progress: { ...progress },
    ...(error ? { error } : {}),
  };
}

interface DownloadCardProps {
  title: string;
  icon: React.ReactNode;
  state: DownloadState;
  /** Card subtitle, e.g. "~422 MB"; hidden when empty. */
  modelSize: string;
  /** Progress figures are printed in this unit (the summary card uses MiB). */
  sizeUnit: string;
  onRetry?: () => void;
}

/** One model card: title, status indicator, progress bar and retry block. */
function DownloadCard({
  title,
  icon,
  state,
  modelSize,
  sizeUnit,
  onRetry,
}: DownloadCardProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.language as AppLanguageCode;
  const hasSizes = state.progress.totalMb > 0;

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-5">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-gray-100 flex items-center justify-center">
            {icon}
          </div>
          <div>
            <h3 className="font-medium text-gray-900">{title}</h3>
            {modelSize && <p className="text-sm text-gray-500">{modelSize}</p>}
          </div>
        </div>
        <div>
          {state.status === 'waiting' && (
            <span className="text-sm text-gray-500">{t('onboarding.downloads.waiting')}</span>
          )}
          {state.status === 'downloading' && (
            <Loader2 className="w-5 h-5 text-gray-700 animate-spin" />
          )}
          {state.status === 'completed' && (
            <div className="w-6 h-6 rounded-full bg-green-100 flex items-center justify-center">
              <Check className="w-4 h-4 text-green-600" />
            </div>
          )}
          {state.status === 'error' && (
            <span className="text-sm text-red-500">{t('onboarding.downloads.failed')}</span>
          )}
          {state.status === 'cancelled' && (
            <span className="text-sm text-gray-500">{t('onboarding.downloads.cancelled')}</span>
          )}
        </div>
      </div>

      {/* Progress Bar */}
      {(state.status === 'downloading' || (state.status === 'completed' && hasSizes)) && (
        <div className="space-y-2">
          <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-gray-700 to-gray-900 rounded-full transition-all duration-300"
              style={{ width: `${state.progress.percent}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-sm">
            <span className="text-gray-600">
              {hasSizes
                ? `${formatDecimal(state.progress.downloadedMb, language)} ${sizeUnit} / ${formatDecimal(state.progress.totalMb, language)} ${sizeUnit}`
                : t('onboarding.downloads.downloading')}
            </span>
            <div className="flex items-center gap-2">
              {hasSizes && state.progress.speedMbps > 0 && (
                <span className="text-gray-500">
                  {t('onboarding.downloads.speed', {
                    speed: formatDecimal(state.progress.speedMbps, language),
                    unit: sizeUnit,
                  })}
                </span>
              )}
              <span className="font-semibold text-gray-900">
                {Math.round(state.progress.percent)}%
              </span>
            </div>
          </div>
        </div>
      )}

      {(state.status === 'error' || state.status === 'cancelled') && (
        <div className="mt-2 p-3 bg-red-50 border border-red-200 rounded-md">
          <p className="text-sm text-red-600 font-medium">
            {state.status === 'cancelled'
              ? t('onboarding.downloads.cancelledTitle')
              : t('onboarding.downloads.errorTitle')}
          </p>
          {state.error && <p className="text-xs text-red-500 mt-1">{state.error}</p>}
          {onRetry && (
            <button
              onClick={onRetry}
              className="mt-3 w-full h-9 px-4 bg-gray-900 hover:bg-gray-800 text-white text-sm font-medium rounded-md transition-colors flex items-center justify-center gap-2"
            >
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                      d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              {t('onboarding.downloads.tryAgain')}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export function DownloadProgressStep() {
  const { t, i18n } = useTranslation();
  const language = i18n.language as AppLanguageCode;
  const {
    goNext,
    selectedSummaryModel,
    recommendedSummaryModel,
    transcriptionModelDownloaded,
    setTranscriptionModelDownloaded,
    transcriptionModelProgressInfo,
    transcriptionModelStatus,
    transcriptionModelError,
    diarizationModelDownloaded,
    setDiarizationModelDownloaded,
    diarizationModelProgressInfo,
    diarizationModelStatus,
    diarizationModelError,
    markAudioCppModelError,
    summaryModelDownloaded,
    setSummaryModelDownloaded,
    startBackgroundDownloads,
    completeOnboarding,
  } = useOnboarding();

  const [isMac, setIsMac] = useState(false);

  const [summaryState, setSummaryState] = useState<DownloadState>({
    status: summaryModelDownloaded ? 'completed' : 'waiting',
    progress: {
      percent: summaryModelDownloaded ? 100 : 0,
      downloadedMb: 0,
      totalMb: 0,
      speedMbps: 0,
    },
  });

  const [isCompleting, setIsCompleting] = useState(false);
  const downloadsStartedRef = useRef(false);
  const summaryDownloadStartedRef = useRef(false);
  const retryingRef = useRef(false);
  const retryingSummaryRef = useRef(false);

  // Both audio.cpp cards read the states the context derives from the native
  // `audiocpp-model-download-*` events; the step only renders them.
  const audioCppStates: AudioCppModelStates = {
    transcription: cardFromContext(
      transcriptionModelStatus,
      transcriptionModelProgressInfo,
      transcriptionModelError
    ),
    diarization: cardFromContext(
      diarizationModelStatus,
      diarizationModelProgressInfo,
      diarizationModelError
    ),
  };

  // Retry a download. There is no dedicated retry command: downloading is the
  // idempotent entry point, so retrying re-requests the catalogued model name
  // from the card's engine and the native manager resumes the download.
  const handleRetryDownload = async (role: AudioCppModelRole) => {
    const modelName = onboardingRetryModelName(role);
    const command =
      role === 'transcription'
        ? TRANSCRIBECPP_COMMANDS.downloadModel
        : AUDIOCPP_COMMANDS.downloadModel;
    if (!modelName) return;

    // Prevent multiple simultaneous retries
    if (retryingRef.current) {
      console.log('[DownloadProgressStep] Retry already in progress, ignoring');
      return;
    }

    console.log('[DownloadProgressStep] Retrying model download:', modelName);
    retryingRef.current = true;

    // Clearing the verified flag turns a failed/cancelled card back to
    // "waiting"; the progress events then take over.
    if (role === 'transcription') {
      setTranscriptionModelDownloaded(false);
    } else {
      setDiarizationModelDownloaded(false);
    }

    try {
      await invoke(command, { modelName });
      // Progress events will update state
    } catch (error) {
      console.error('[DownloadProgressStep] Retry failed:', error);
      // Without a native error event the card would stay in "waiting",
      // leaving no error text and no way to retry again.
      markAudioCppModelError(
        role,
        error instanceof Error ? error.message : String(error)
      );
      toast.error(
        role === 'transcription'
          ? t('onboarding.downloads.retry.transcriptionFailed')
          : t('onboarding.downloads.retry.diarizationFailed'),
        {
          description: t('onboarding.downloads.retry.connectionHint'),
        }
      );
    } finally {
      // Allow retry again after 2 seconds
      setTimeout(() => {
        retryingRef.current = false;
      }, 2000);
    }
  };

  // Retry summary download handler
  const handleRetrySummaryDownload = async () => {
    // Prevent multiple simultaneous retries
    if (retryingSummaryRef.current) {
      console.log('[DownloadProgressStep] Summary retry already in progress, ignoring');
      return;
    }

    console.log('[DownloadProgressStep] Retrying summary model download');
    retryingSummaryRef.current = true;

    // Reset error state
    setSummaryState((prev) => ({
      ...prev,
      status: 'downloading',
      error: undefined,
      progress: {
        percent: 0,
        downloadedMb: 0,
        totalMb: getSummaryModelSizeMb(selectedSummaryModel || recommendedSummaryModel),
        speedMbps: 0,
      },
    }));

    try {
      // Call download command directly (no retry command exists for built-in AI)
      const modelName = selectedSummaryModel;
      if (!modelName) {
        throw new Error(t('onboarding.downloads.errors.summaryModelNotReady'));
      }
      await invoke('builtin_ai_download_model', { modelName });
    } catch (error) {
      console.error('[DownloadProgressStep] Summary retry failed:', error);
      setSummaryState((prev) => ({
        ...prev,
        status: 'error',
        error: error instanceof Error ? error.message : t('onboarding.downloads.retry.genericFailed'),
      }));

      toast.error(t('onboarding.downloads.retry.summaryFailed'), {
        description: t('onboarding.downloads.retry.connectionHint'),
      });
    } finally {
      // Allow retry again after 2 seconds
      setTimeout(() => {
        retryingSummaryRef.current = false;
      }, 2000);
    }
  };

  // Detect platform on mount
  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };

    checkPlatform();
  }, []);

  // Start both audio.cpp downloads immediately; summary readiness must not
  // block them. Diarization keeps running in the background and never gates
  // Continue.
  useEffect(() => {
    if (downloadsStartedRef.current) return;
    downloadsStartedRef.current = true;

    startBackgroundDownloads({
      includeTranscription: true,
      includeDiarization: true,
      includeSummary: false,
    }).catch((error) => {
      console.error('Failed to start onboarding model downloads:', error);
    });
  }, []);

  // Start the selected summary model only after the backend recommendation is known.
  useEffect(() => {
    if (summaryDownloadStartedRef.current) return;
    if (!selectedSummaryModel) return;
    summaryDownloadStartedRef.current = true;

    startSummaryDownload();
  }, [selectedSummaryModel]);

  // Listen to Summary Model download progress (always downloading for builtin-ai)
  useEffect(() => {
    const unlisten = listen<{
      model: string;
      progress: number;
      downloaded_mb?: number;
      total_mb?: number;
      speed_mbps?: number;
      status: string;
      error?: string;
    }>('builtin-ai-download-progress', (event) => {
      const { model, progress, downloaded_mb, total_mb, speed_mbps, status, error } = event.payload;
      if (selectedSummaryModel && model === selectedSummaryModel) {
        setSummaryState((prev) => ({
          ...prev,
          status: status === 'completed'
            ? 'completed'
            : status === 'error'
            ? 'error'
            : 'downloading',
          progress: {
            percent: progress,
            downloadedMb: downloaded_mb ?? prev.progress.downloadedMb,
            totalMb: (total_mb ?? prev.progress.totalMb) || getSummaryModelSizeMb(model),
            speedMbps: speed_mbps ?? prev.progress.speedMbps,
          },
          error: status === 'error' ? error : undefined,
        }));

        if (status === 'completed' || progress >= 100) {
          setSummaryModelDownloaded(true);
        }
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [selectedSummaryModel]);

  useEffect(() => {
    const modelForSize = selectedSummaryModel || recommendedSummaryModel;
    if (!modelForSize) return;

    setSummaryState((prev) => ({
      ...prev,
      status: summaryModelDownloaded
        ? 'completed'
        : prev.status === 'completed'
        ? 'waiting'
        : prev.status,
      progress: {
        ...prev.progress,
        percent: summaryModelDownloaded
          ? 100
          : prev.status === 'completed'
          ? 0
          : prev.progress.percent,
        totalMb: prev.progress.totalMb || getSummaryModelSizeMb(modelForSize),
      },
    }));
  }, [selectedSummaryModel, recommendedSummaryModel, summaryModelDownloaded]);

  const startSummaryDownload = async () => {
    if (!summaryModelDownloaded && selectedSummaryModel) {
      try {
        setSummaryState((prev) => ({
          ...prev,
          status: 'downloading',
          progress: {
            ...prev.progress,
            totalMb: getSummaryModelSizeMb(selectedSummaryModel),
          },
        }));
        await startBackgroundDownloads({
          includeTranscription: false,
          includeDiarization: false,
          includeSummary: true,
          summaryModel: selectedSummaryModel,
        });
      } catch (error) {
        console.error('Failed to start summary model download:', error);
        setSummaryState((prev) => ({ ...prev, status: 'error', error: String(error) }));
      }
    }
  };

  const handleContinue = async () => {
    let transcriptionReady = transcriptionModelDownloaded;

    // Verify the native ASR gate (catches state drift). Only the transcription
    // model gates onboarding: an installed Nemotron alone does not pass.
    try {
      const installedOnDisk = await invoke<boolean>(
        TRANSCRIBECPP_COMMANDS.hasTranscriptionModel
      );
      const gate = resolveTranscriptionGate({
        installedOnDisk,
        state: audioCppStates.transcription,
      });

      if (gate === 'promote') {
        console.log('[DownloadProgressStep] Model available but state not updated');
        setTranscriptionModelDownloaded(true);
        transcriptionReady = true;
      } else if (gate === 'blocked') {
        toast.error(t('onboarding.downloads.transcriptionRequired'), {
          description: t('onboarding.downloads.transcriptionRequiredDescription'),
        });
        return;
      }
    } catch (error) {
      // A failed check must not fall through: completing onboarding would
      // write the audioCpp transcript config without the model on disk.
      console.warn('[DownloadProgressStep] Failed to verify model:', error);
      toast.error(t('onboarding.downloads.verifyFailed'), {
        description: t('onboarding.downloads.verifyFailedDescription'),
      });
      return;
    }

    // Check if downloads are complete for toast notification
    const downloadsComplete =
      transcriptionReady &&
      diarizationModelDownloaded &&
      summaryModelDownloaded;

    // Show toast if downloads still in progress
    if (!downloadsComplete) {
      toast.info(t('onboarding.downloads.backgroundInfo'), {
        description: t('onboarding.downloads.backgroundInfoDescription'),
        duration: 5000,
      });
    }

    if (isMac) {
      // macOS: Go to Permissions step (will complete after permissions granted)
      goNext();
    } else {
      // Non-macOS: Complete onboarding immediately (downloads continue in background)
      setIsCompleting(true);
      try {
        await completeOnboarding();

        // Small delay to ensure state is saved before reload
        await new Promise(resolve => setTimeout(resolve, 100));

        window.location.reload();
      } catch (error) {
        console.error('Failed to complete onboarding:', error);
        toast.error(t('onboarding.downloads.completeFailed'), {
          description: t('onboarding.downloads.completeFailedDescription'),
        });
        setIsCompleting(false);
      }
    }
  };

  // "Try Again" for a card: each engine's retry re-requests the catalogued
  // model, the summary card keeps its built-in AI path.
  const retryHandlerFor = (card: OnboardingDownloadCard): (() => void) | undefined => {
    const target = onboardingDownloadRetryTarget(card);
    if (target === 'builtin-ai') return handleRetrySummaryDownload;
    if ((target === 'transcribecpp' || target === 'audiocpp') && card !== 'summary') {
      return () => handleRetryDownload(card);
    }
    return undefined;
  };

  const transcriptionCard = audioCppStates.transcription;
  const diarizationCard = audioCppStates.diarization;
  const continueEnabled = isContinueEnabled(audioCppStates);
  const showBackgroundNotice = shouldShowBackgroundDownloadNotice({
    transcriptionDownloaded: transcriptionModelDownloaded,
    diarizationDownloaded: diarizationModelDownloaded,
    summaryDownloaded: summaryModelDownloaded,
  });
  const summaryModelSizeMb = getSummaryModelSizeMb(
    selectedSummaryModel || recommendedSummaryModel
  );

  return (
    <OnboardingContainer
      title={t('onboarding.downloads.title')}
      description={t('onboarding.downloads.description')}
      step={3}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-6">
        {/* Download Cards */}
        <div className="w-full max-w-lg space-y-4">
          <DownloadCard
            title={t('onboarding.downloads.cards.transcription')}
            icon={<Mic className="w-5 h-5 text-gray-600" />}
            state={transcriptionCard}
            modelSize={
              transcriptionCard.progress.totalMb > 0
                ? `~${formatMegabytes(transcriptionCard.progress.totalMb, language)}`
                : ''
            }
            sizeUnit="MB"
            onRetry={retryHandlerFor('transcription')}
          />

          <DownloadCard
            title={t('onboarding.downloads.cards.diarization')}
            icon={<Users className="w-5 h-5 text-gray-600" />}
            state={diarizationCard}
            modelSize={
              diarizationCard.progress.totalMb > 0
                ? `~${formatMegabytes(diarizationCard.progress.totalMb, language)}`
                : ''
            }
            sizeUnit="MB"
            onRetry={retryHandlerFor('diarization')}
          />

          <DownloadCard
            title={t('onboarding.downloads.cards.summary')}
            icon={<Sparkles className="w-5 h-5 text-gray-600" />}
            state={summaryState}
            modelSize={
              summaryModelSizeMb > 0
                ? `~${formatMegabytes(summaryModelSizeMb, language)}`
                : ''
            }
            sizeUnit="MiB"
            onRetry={retryHandlerFor('summary')}
          />
        </div>

        {/* Info Message - show once the app is usable but downloads continue */}
        <AnimatePresence>
          {showBackgroundNotice && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              transition={{ duration: 0.3, ease: 'easeOut' }}
              className="w-full max-w-lg bg-gray-100 rounded-lg p-4 text-sm text-gray-800"
            >
              <div className="flex items-start gap-3">
                <Download className="w-5 h-5 text-gray-600 flex-shrink-0 mt-0.5" />
                <div>
                  <p className="font-medium">{t('onboarding.downloads.backgroundNotice.title')}</p>
                  <p className="text-gray-700 mt-1">
                    {t('onboarding.downloads.backgroundNotice.description')}
                  </p>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Continue Button */}
        <div className="w-full max-w-xs">
          <Button
            onClick={handleContinue}
            disabled={!continueEnabled || isCompleting}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {(isCompleting || !continueEnabled) ? (
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              t('onboarding.downloads.continue')
            )}
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
