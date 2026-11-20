import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import {
  TRANSCRIBECPP_COMMANDS,
  TRANSCRIBECPP_EVENTS,
  type TranscribeCppCancelDownloadOutcome,
  type TranscribeCppDownloadCompleteEvent,
  type TranscribeCppDownloadErrorEvent,
  type TranscribeCppDownloadProgressEvent,
  type TranscribeCppModelClient,
  type TranscribeCppModelInfo,
  type UnlistenFn,
} from './transcribeCpp';

/**
 * Tauri-backed implementation of {@link TranscribeCppModelClient}.
 *
 * Kept apart from `./transcribeCpp` so that pure types and helpers stay
 * importable without pulling in Tauri (and so unit tests that only need the
 * types never touch the native bridge).
 */
export class TranscribeCppAPI {
  static async init(): Promise<void> {
    await invoke(TRANSCRIBECPP_COMMANDS.init);
  }

  static async getAvailableModels(): Promise<TranscribeCppModelInfo[]> {
    return await invoke(TRANSCRIBECPP_COMMANDS.getAvailableModels);
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke(TRANSCRIBECPP_COMMANDS.hasAvailableModels);
  }

  static async hasTranscriptionModel(): Promise<boolean> {
    return await invoke(TRANSCRIBECPP_COMMANDS.hasTranscriptionModel);
  }

  static async getModelsDirectory(): Promise<string> {
    return await invoke(TRANSCRIBECPP_COMMANDS.getModelsDirectory);
  }

  static async downloadModel(modelName: string): Promise<void> {
    await invoke(TRANSCRIBECPP_COMMANDS.downloadModel, { modelName });
  }

  static async cancelDownload(modelName: string): Promise<TranscribeCppCancelDownloadOutcome> {
    return await invoke(TRANSCRIBECPP_COMMANDS.cancelDownload, { modelName });
  }

  static async deleteModel(modelName: string): Promise<string> {
    return await invoke(TRANSCRIBECPP_COMMANDS.deleteModel, { modelName });
  }

  static async openModelsFolder(): Promise<void> {
    await invoke(TRANSCRIBECPP_COMMANDS.openModelsFolder);
  }
}

/** {@link TranscribeCppModelClient} backed by Tauri commands and events. */
class TauriTranscribeCppModelClient implements TranscribeCppModelClient {
  init = () => TranscribeCppAPI.init();
  getAvailableModels = () => TranscribeCppAPI.getAvailableModels();
  hasAvailableModels = () => TranscribeCppAPI.hasAvailableModels();
  hasTranscriptionModel = () => TranscribeCppAPI.hasTranscriptionModel();
  getModelsDirectory = () => TranscribeCppAPI.getModelsDirectory();
  downloadModel = (modelName: string) => TranscribeCppAPI.downloadModel(modelName);
  cancelDownload = (modelName: string) => TranscribeCppAPI.cancelDownload(modelName);
  deleteModel = (modelName: string) => TranscribeCppAPI.deleteModel(modelName);
  openModelsFolder = () => TranscribeCppAPI.openModelsFolder();

  listenProgress(
    handler: (event: TranscribeCppDownloadProgressEvent) => void,
  ): Promise<UnlistenFn> {
    return listen<TranscribeCppDownloadProgressEvent>(TRANSCRIBECPP_EVENTS.progress, event =>
      handler(event.payload)
    );
  }

  listenComplete(
    handler: (event: TranscribeCppDownloadCompleteEvent) => void,
  ): Promise<UnlistenFn> {
    return listen<TranscribeCppDownloadCompleteEvent>(TRANSCRIBECPP_EVENTS.complete, event =>
      handler(event.payload)
    );
  }

  listenError(handler: (event: TranscribeCppDownloadErrorEvent) => void): Promise<UnlistenFn> {
    return listen<TranscribeCppDownloadErrorEvent>(TRANSCRIBECPP_EVENTS.error, event =>
      handler(event.payload)
    );
  }
}

/** The client the settings UI uses by default. */
export const transcribeCppClient: TranscribeCppModelClient = new TauriTranscribeCppModelClient();
