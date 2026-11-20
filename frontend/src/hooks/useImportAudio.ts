import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { applyPinnedSummaryLanguageToMeeting } from '@/lib/summary-language-preferences';
import {
  shouldToastDiarizationUnavailable,
  showDiarizationFailure,
  type DiarizationUnavailablePayload,
} from '@/lib/diarization';
import { toast } from 'sonner';
import i18n from '@/i18n';

export interface AudioFileInfo {
  path: string;
  filename: string;
  duration_seconds: number;
  size_bytes: number;
  format: string;
}

export interface ImportProgress {
  stage: string;
  progress_percentage: number;
  message: string;
}

export interface ImportResult {
  meeting_id: string;
  title: string;
  segments_count: number;
  duration_seconds: number;
}

export interface ImportError {
  error: string;
}

export type ImportStatus = 'idle' | 'validating' | 'processing' | 'complete' | 'error';

export interface UseImportAudioOptions {
  onComplete?: (result: ImportResult) => void;
  onError?: (error: string) => void;
}

export interface UseImportAudioReturn {
  status: ImportStatus;
  fileInfo: AudioFileInfo | null;
  progress: ImportProgress | null;
  error: string | null;
  isProcessing: boolean;
  isBusy: boolean;
  selectFile: () => Promise<AudioFileInfo | null>;
  validateFile: (path: string) => Promise<AudioFileInfo | null>;
  startImport: (
    sourcePath: string,
    title: string,
    language?: string | null,
    model?: string | null,
    provider?: 'transcribeCpp' | null
  ) => Promise<void>;
  cancelImport: () => Promise<void>;
  reset: () => void;
}

export function useImportAudio({
  onComplete,
  onError,
}: UseImportAudioOptions = {}): UseImportAudioReturn {
  const [status, setStatus] = useState<ImportStatus>('idle');
  const [fileInfo, setFileInfo] = useState<AudioFileInfo | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Stable refs for callbacks to avoid listener re-registration on every render
  const onCompleteRef = useRef(onComplete);
  const onErrorRef = useRef(onError);
  useEffect(() => { onCompleteRef.current = onComplete; }, [onComplete]);
  useEffect(() => { onErrorRef.current = onError; }, [onError]);

  // Cancellation guard: prevents late events from updating state after cancel
  const isCancelledRef = useRef(false);

  // The meeting this dialog's import produced. The diarization listener is
  // process-wide, so without it a failed pass for any other meeting would toast
  // inside the open dialog. Filled by `import-complete`, which is the first
  // moment the id exists.
  const importedMeetingIdRef = useRef<string | null>(null);

  // Set up event listeners (registered once, use refs for callbacks)
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const cleanedUpRef = { current: false };

    const setupListeners = async () => {
      // Progress events
      const unlistenProgress = await listen<ImportProgress>(
        'import-progress',
        (event) => {
          if (isCancelledRef.current) return;
          setProgress(event.payload);
          setStatus('processing');
        }
      );
      if (cleanedUpRef.current) {
        unlistenProgress();
        return;
      }
      unlisteners.push(unlistenProgress);

      // Completion event
      const unlistenComplete = await listen<ImportResult>(
        'import-complete',
        async (event) => {
          if (isCancelledRef.current) return;

          setStatus('complete');
          setProgress(null);
          // The diarization listener now knows which meeting belongs to this
          // dialog; the pass for it runs right after the save.
          importedMeetingIdRef.current = event.payload.meeting_id;
          try {
            await applyPinnedSummaryLanguageToMeeting(event.payload.meeting_id);
          } catch (error) {
            console.warn('Failed to apply pinned summary language to imported meeting:', error);
            toast.warning(i18n.t('toasts.summaryLanguage.pinned'), {
              description: i18n.t('toasts.summaryLanguage.pinnedDescriptionImported'),
            });
          }
          onCompleteRef.current?.(event.payload);
        }
      );
      if (cleanedUpRef.current) {
        unlistenComplete();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenComplete);

      // Error event
      const unlistenError = await listen<ImportError>(
        'import-error',
        (event) => {
          if (isCancelledRef.current) return;

          setStatus('error');
          setError(event.payload.error);
          onErrorRef.current?.(event.payload.error);
        }
      );
      if (cleanedUpRef.current) {
        unlistenError();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenError);

      // A diarization pass that was attempted and failed. Native code emits
      // this event only for `failed`; the status check keeps the hook silent
      // for configuration reasons if that contract is ever widened. The
      // transcript is already complete, so this is never an error state. Only
      // this import's meeting speaks: the event is process-wide, and a failure
      // from another meeting must not surface in the open dialog.
      const unlistenDiarization = await listen<DiarizationUnavailablePayload>(
        'diarization-unavailable',
        (event) => {
          if (isCancelledRef.current) return;
          if (
            importedMeetingIdRef.current === null ||
            event.payload.meeting_id !== importedMeetingIdRef.current
          ) {
            return;
          }
          if (!shouldToastDiarizationUnavailable(event.payload.status)) return;
          showDiarizationFailure(toast, event.payload.message);
        }
      );
      if (cleanedUpRef.current) {
        unlistenDiarization();
        unlisteners.forEach(u => u());
        return;
      }
      unlisteners.push(unlistenDiarization);
    };

    setupListeners();

    return () => {
      cleanedUpRef.current = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  // Select file using native file dialog
  const selectFile = useCallback(async (): Promise<AudioFileInfo | null> => {
    setStatus('validating');
    setError(null);

    try {
      const result = await invoke<AudioFileInfo | null>('select_and_validate_audio_command');
      if (result) {
        setFileInfo(result);
        setStatus('idle');
        return result;
      } else {
        // User cancelled
        setStatus('idle');
        return null;
      }
    } catch (err: any) {
      setStatus('error');
      const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || 'Failed to validate file');
      setError(errorMsg);
      onErrorRef.current?.(errorMsg);
      return null;
    }
  }, []);

  // Validate a file from a given path (for drag-drop)
  const validateFile = useCallback(async (path: string): Promise<AudioFileInfo | null> => {
    setStatus('validating');
    setError(null);

    try {
      const result = await invoke<AudioFileInfo>('validate_audio_file_command', { path });
      setFileInfo(result);
      setStatus('idle');
      return result;
    } catch (err: any) {
      setStatus('error');
      const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || 'Failed to validate file');
      setError(errorMsg);
      onErrorRef.current?.(errorMsg);
      return null;
    }
  }, []);

  // Start the import process
  const startImport = useCallback(
    async (
      sourcePath: string,
      title: string,
      language?: string | null,
      model?: string | null,
      provider?: string | null
    ) => {
      isCancelledRef.current = false;
      // A new import starts before its meeting id exists; the previous
      // import's meeting must not claim a failure from this one.
      importedMeetingIdRef.current = null;
      setStatus('processing');
      setError(null);
      setProgress(null);

      try {
        await invoke('start_import_audio_command', {
          sourcePath,
          title,
          language: language || null,
          model: model || null,
          provider: provider || null,
        });
      } catch (err: any) {
        setStatus('error');
        const errorMsg = typeof err === 'string' ? err : (err?.message || String(err) || 'Failed to start import');
        setError(errorMsg);

        onErrorRef.current?.(errorMsg);
      }
    },
    []
  );

  // Cancel ongoing import
  const cancelImport = useCallback(async () => {
    isCancelledRef.current = true;
    try {
      await invoke('cancel_import_command');
      setStatus('idle');
      setProgress(null);
    } catch (err: any) {
      console.error('Failed to cancel import:', err);
    }
  }, []);

  // Reset all state
  const reset = useCallback(() => {
    isCancelledRef.current = false;
    importedMeetingIdRef.current = null;
    setStatus('idle');
    setFileInfo(null);
    setProgress(null);
    setError(null);
  }, []);

  return {
    status,
    fileInfo,
    progress,
    error,
    isProcessing: status === 'processing',
    isBusy: status === 'processing' || status === 'validating',
    selectFile,
    validateFile,
    startImport,
    cancelImport,
    reset,
  };
}
