import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import {
  AUDIOCPP_COMMANDS,
  AUDIOCPP_EVENTS,
  type AudioCppCancelDownloadOutcome,
  type AudioCppDownloadCompleteEvent,
  type AudioCppDownloadErrorEvent,
  type AudioCppDownloadProgressEvent,
  type AudioCppModelClient,
  type AudioCppModelInfo,
  type UnlistenFn,
} from './audiocpp';

/**
 * Tauri-backed implementation of {@link AudioCppModelClient}.
 *
 * Kept apart from `./audiocpp` so that pure types and helpers stay importable
 * without pulling in Tauri (and so unit tests that only need the types never
 * touch the native bridge).
 */
export class AudioCppAPI {
  static async init(): Promise<void> {
    await invoke(AUDIOCPP_COMMANDS.init);
  }

  static async getAvailableModels(): Promise<AudioCppModelInfo[]> {
    return await invoke(AUDIOCPP_COMMANDS.getAvailableModels);
  }

  static async hasAvailableModels(): Promise<boolean> {
    return await invoke(AUDIOCPP_COMMANDS.hasAvailableModels);
  }

  static async getModelsDirectory(): Promise<string> {
    return await invoke(AUDIOCPP_COMMANDS.getModelsDirectory);
  }

  static async downloadModel(modelName: string): Promise<void> {
    await invoke(AUDIOCPP_COMMANDS.downloadModel, { modelName });
  }

  static async cancelDownload(modelName: string): Promise<AudioCppCancelDownloadOutcome> {
    return await invoke(AUDIOCPP_COMMANDS.cancelDownload, { modelName });
  }

  static async deleteModel(modelName: string): Promise<string> {
    return await invoke(AUDIOCPP_COMMANDS.deleteModel, { modelName });
  }

  static async openModelsFolder(): Promise<void> {
    await invoke(AUDIOCPP_COMMANDS.openModelsFolder);
  }
}

/** {@link AudioCppModelClient} backed by Tauri commands and events. */
class TauriAudioCppModelClient implements AudioCppModelClient {
  init = () => AudioCppAPI.init();
  getAvailableModels = () => AudioCppAPI.getAvailableModels();
  hasAvailableModels = () => AudioCppAPI.hasAvailableModels();
  getModelsDirectory = () => AudioCppAPI.getModelsDirectory();
  downloadModel = (modelName: string) => AudioCppAPI.downloadModel(modelName);
  cancelDownload = (modelName: string) => AudioCppAPI.cancelDownload(modelName);
  deleteModel = (modelName: string) => AudioCppAPI.deleteModel(modelName);
  openModelsFolder = () => AudioCppAPI.openModelsFolder();

  listenProgress(handler: (event: AudioCppDownloadProgressEvent) => void): Promise<UnlistenFn> {
    return listen<AudioCppDownloadProgressEvent>(AUDIOCPP_EVENTS.progress, event =>
      handler(event.payload)
    );
  }

  listenComplete(handler: (event: AudioCppDownloadCompleteEvent) => void): Promise<UnlistenFn> {
    return listen<AudioCppDownloadCompleteEvent>(AUDIOCPP_EVENTS.complete, event =>
      handler(event.payload)
    );
  }

  listenError(handler: (event: AudioCppDownloadErrorEvent) => void): Promise<UnlistenFn> {
    return listen<AudioCppDownloadErrorEvent>(AUDIOCPP_EVENTS.error, event =>
      handler(event.payload)
    );
  }
}

/** The client the settings UI uses by default. */
export const audioCppClient: AudioCppModelClient = new TauriAudioCppModelClient();
