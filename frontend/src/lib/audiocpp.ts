/**
 * Types and Tauri command wrappers for the audio.cpp (GigaAM + Nemotron)
 * model manager.
 *
 * The catalog metadata is engine-independent: `family` and `task` describe
 * what the model is in audio.cpp terms, so the same list serves transcription
 * (asr) and diarization (diar) once those paths exist.
 */

/** Task a catalogued model performs, in audio.cpp terms. */
export type AudioCppModelTask = 'asr' | 'diar';

/** Quantization of the published weights. */
export type AudioCppQuantization = 'F16' | 'BF16';

export type AudioCppModelStatus =
  | 'Available'
  | 'Missing'
  | { Downloading: { progress: number } }
  | { Error: string }
  | { Corrupted: { file_size: number; expected_min_size: number } };

export interface AudioCppModelInfo {
  name: string;
  display_name: string;
  family: string;
  task: AudioCppModelTask;
  quantization: AudioCppQuantization;
  filename: string;
  path: string;
  size_mb: number;
  size_bytes: number;
  description: string;
  status: AudioCppModelStatus;
}

export type AudioCppCancelDownloadOutcome = 'cancelled' | 'pending';

export type AudioCppDownloadEventStatus = 'downloading' | 'completed' | 'cancelled';

export interface AudioCppDownloadProgressEvent {
  modelName: string;
  progress: number;
  /** Downloaded size in MiB, from the catalogued model size. */
  downloaded_mb?: number;
  /** Catalogued model size in MiB. */
  total_mb?: number;
  /** Measured rate since the previous report, in MiB/s. */
  speed_mbps?: number;
  status: AudioCppDownloadEventStatus;
}

export interface AudioCppDownloadCompleteEvent {
  modelName: string;
}

export interface AudioCppDownloadErrorEvent {
  modelName: string;
  error: string;
}

/**
 * Tauri command names of the native audio.cpp model manager. The Rust side
 * registers exactly these in `lib.rs`; keeping them in one place makes the
 * contract testable without importing the Tauri bridge.
 */
export const AUDIOCPP_COMMANDS = {
  init: 'audiocpp_model_init',
  getAvailableModels: 'audiocpp_get_available_models',
  hasAvailableModels: 'audiocpp_has_available_models',
  hasTranscriptionModel: 'audiocpp_has_transcription_model',
  getModelsDirectory: 'audiocpp_get_models_directory',
  downloadModel: 'audiocpp_download_model',
  cancelDownload: 'audiocpp_cancel_download',
  deleteModel: 'audiocpp_delete_model',
  openModelsFolder: 'open_audiocpp_models_folder',
} as const;

/** Event names emitted by the native audio.cpp model manager. */
export const AUDIOCPP_EVENTS = {
  progress: 'audiocpp-model-download-progress',
  complete: 'audiocpp-model-download-complete',
  error: 'audiocpp-model-download-error',
} as const;

/** Task label shown in the model manager. */
export function taskLabel(task: AudioCppModelTask): string {
  return task === 'asr' ? 'Transcription' : 'Diarization';
}

/** Whether a status value represents an in-flight download. */
export function isDownloadingStatus(status: AudioCppModelStatus): boolean {
  return typeof status === 'object' && status !== null && 'Downloading' in status;
}

/** Progress percentage of an in-flight download, otherwise `null`. */
export function downloadProgress(status: AudioCppModelStatus): number | null {
  return isDownloadingStatus(status)
    ? (status as { Downloading: { progress: number } }).Downloading.progress
    : null;
}

/** Whether a status value means the artifact is installed and usable. */
export function isAvailableStatus(status: AudioCppModelStatus): boolean {
  return status === 'Available';
}

/** Human-readable size, matching the other model managers. */
export function formatModelSize(sizeMb: number): string {
  if (sizeMb >= 1000) {
    return `${(sizeMb / 1000).toFixed(1)}GB`;
  }
  return `${sizeMb}MB`;
}

/** Unlisten handle returned by an event subscription. */
export type UnlistenFn = () => void;

/** Whether a model can transcribe (as opposed to diarize). */
export function isTranscriptionModel(model: Pick<AudioCppModelInfo, 'task'>): boolean {
  return model.task === 'asr';
}

/**
 * The model the settings should offer for selection: the configured one when
 * it is an installed transcription model, otherwise the first installed
 * transcription model. Mirrors the native selection rules.
 */
export function defaultSelectedTranscriptionModel(
  models: AudioCppModelInfo[],
  configured?: string,
): AudioCppModelInfo | undefined {
  const transcription = models.filter(isTranscriptionModel);
  return transcription.find(model => model.name === configured && isAvailableStatus(model.status))
    ?? transcription.find(model => isAvailableStatus(model.status));
}

/**
 * The seam between the model manager UI and the native audio.cpp model
 * manager: commands plus the three download events. Production code uses
 * `audioCppClient` from `./audiocppClient`; tests inject a fake so component
 * behaviour can be exercised without Tauri or process-wide module mocks.
 *
 * This module is deliberately free of Tauri imports so it can be imported by
 * plain unit tests.
 */
export interface AudioCppModelClient {
  init(): Promise<void>;
  getAvailableModels(): Promise<AudioCppModelInfo[]>;
  hasAvailableModels(): Promise<boolean>;
  /** Whether an installed model can transcribe (asr), not just diarize. */
  hasTranscriptionModel(): Promise<boolean>;
  getModelsDirectory(): Promise<string>;
  downloadModel(modelName: string): Promise<void>;
  cancelDownload(modelName: string): Promise<AudioCppCancelDownloadOutcome>;
  deleteModel(modelName: string): Promise<string>;
  openModelsFolder(): Promise<void>;
  listenProgress(handler: (event: AudioCppDownloadProgressEvent) => void): Promise<UnlistenFn>;
  listenComplete(handler: (event: AudioCppDownloadCompleteEvent) => void): Promise<UnlistenFn>;
  listenError(handler: (event: AudioCppDownloadErrorEvent) => void): Promise<UnlistenFn>;
}
