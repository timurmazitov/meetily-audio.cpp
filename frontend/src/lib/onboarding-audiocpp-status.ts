/**
 * Pure state resolution for the audio.cpp models downloaded during onboarding.
 *
 * The onboarding context receives raw native events and model lists; the
 * mapping to the "Transcription Engine" / "Speaker Diarization" card states
 * lives here so it can be tested without Tauri (see
 * `frontend/tests/lib/onboarding-audiocpp-status.test.mjs`).
 *
 * This module deliberately has no runtime imports: model names are duplicated
 * from the native catalog (`audio_cpp_engine::catalog`), matching the earlier
 * `PARAKEET_MODEL` constant. Event/model shapes are imported type-only, so the
 * module stays importable from plain unit tests.
 */

import type {
  AudioCppDownloadCompleteEvent,
  AudioCppDownloadErrorEvent,
  AudioCppDownloadProgressEvent,
  AudioCppModelInfo,
} from './audiocpp';

/** Catalogued audio.cpp ASR model onboarding installs. */
export const AUDIOCPP_TRANSCRIPTION_MODEL = 'gigaam-v3-e2e-ctc-f16';

/** Catalogued audio.cpp diarization model onboarding installs. */
export const AUDIOCPP_DIARIZATION_MODEL = 'nemotron-3-diarization-bf16';

/** The two onboarding download cards backed by audio.cpp. */
export type AudioCppModelRole = 'transcription' | 'diarization';

export type AudioCppModelDownloadStatus =
  | 'waiting'
  | 'downloading'
  | 'completed'
  | 'cancelled'
  | 'error';

/** Progress shown on a card; mirrors the fields of the progress event. */
export interface AudioCppModelProgressInfo {
  percent: number;
  downloadedMb: number;
  totalMb: number;
  speedMbps: number;
}

export interface AudioCppModelCardState {
  status: AudioCppModelDownloadStatus;
  progress: AudioCppModelProgressInfo;
  error?: string;
}

export interface AudioCppModelStates {
  transcription: AudioCppModelCardState;
  diarization: AudioCppModelCardState;
}

/** The subset of a catalogued model these helpers read. */
export type AudioCppModelSummary = Pick<AudioCppModelInfo, 'name' | 'status'>;

const ZERO_PROGRESS: AudioCppModelProgressInfo = {
  percent: 0,
  downloadedMb: 0,
  totalMb: 0,
  speedMbps: 0,
};

function idleCard(): AudioCppModelCardState {
  return { status: 'waiting', progress: { ...ZERO_PROGRESS } };
}

/** Card state before anything has been reported for either model. */
export function initialAudioCppModelStates(): AudioCppModelStates {
  return {
    transcription: idleCard(),
    diarization: idleCard(),
  };
}

/** Which onboarding card a catalogued model name belongs to; foreign names get `null`. */
export function audioCppModelRole(modelName: string): AudioCppModelRole | null {
  if (modelName === AUDIOCPP_TRANSCRIPTION_MODEL) return 'transcription';
  if (modelName === AUDIOCPP_DIARIZATION_MODEL) return 'diarization';
  return null;
}

function updateRole(
  states: AudioCppModelStates,
  role: AudioCppModelRole,
  card: AudioCppModelCardState
): AudioCppModelStates {
  return {
    ...states,
    [role]: card,
  };
}

/**
 * Apply a progress report. Reports for another model leave both cards
 * untouched, so concurrent transcription and diarization downloads never
 * overwrite each other's state.
 */
export function applyAudioCppProgressEvent(
  states: AudioCppModelStates,
  event: AudioCppDownloadProgressEvent
): AudioCppModelStates {
  const role = audioCppModelRole(event.modelName);
  if (!role) return states;

  if (event.status === 'cancelled') {
    return updateRole(states, role, {
      status: 'cancelled',
      progress: { ...ZERO_PROGRESS },
    });
  }

  const completed = event.status === 'completed' || event.progress >= 100;

  return updateRole(states, role, {
    status: completed ? 'completed' : 'downloading',
    progress: {
      percent: event.progress,
      downloadedMb: event.downloaded_mb ?? 0,
      totalMb: event.total_mb ?? 0,
      speedMbps: event.speed_mbps ?? 0,
    },
  });
}

/** Apply `audiocpp-model-download-complete` for the matching card only. */
export function applyAudioCppCompleteEvent(
  states: AudioCppModelStates,
  event: AudioCppDownloadCompleteEvent
): AudioCppModelStates {
  const role = audioCppModelRole(event.modelName);
  if (!role) return states;

  return updateRole(states, role, {
    status: 'completed',
    progress: { ...states[role].progress, percent: 100 },
  });
}

/** Apply `audiocpp-model-download-error` for the matching card only. */
export function applyAudioCppErrorEvent(
  states: AudioCppModelStates,
  event: AudioCppDownloadErrorEvent
): AudioCppModelStates {
  const role = audioCppModelRole(event.modelName);
  if (!role) return states;

  return updateRole(states, role, {
    status: 'error',
    progress: { ...states[role].progress },
    error: event.error,
  });
}

/**
 * Reflect a verified download flag on a card.
 *
 * Clearing a flag resets a failed, cancelled or previously completed card so a
 * retry shows a clean "waiting" card; an in-flight download keeps reporting its
 * own state.
 */
export function setAudioCppModelDownloaded(
  states: AudioCppModelStates,
  role: AudioCppModelRole,
  downloaded: boolean
): AudioCppModelStates {
  const current = states[role];

  if (downloaded) {
    if (current.status === 'completed') return states;
    return updateRole(states, role, {
      status: 'completed',
      progress: { ...current.progress, percent: 100 },
    });
  }

  if (current.status === 'downloading') {
    return states;
  }

  if (current.status === 'waiting' && !current.error) {
    return states;
  }

  return updateRole(states, role, idleCard());
}

/**
 * Mark a card as downloading as soon as the download is requested.
 *
 * Progress events arrive a moment later; without this the card would still say
 * "waiting" right after a retry, which reads as a dead button.
 */
export function startAudioCppModelDownload(
  states: AudioCppModelStates,
  role: AudioCppModelRole
): AudioCppModelStates {
  return updateRole(states, role, {
    status: 'downloading',
    progress: { ...states[role].progress },
  });
}

/**
 * Apply both verified downloaded flags at once, as reported by the saved
 * onboarding status or by a check of the installed models.
 */
export function setAudioCppModelsDownloaded(
  states: AudioCppModelStates,
  downloaded: { transcription: boolean; diarization: boolean }
): AudioCppModelStates {
  const withTranscription = setAudioCppModelDownloaded(
    states,
    'transcription',
    downloaded.transcription
  );
  return setAudioCppModelDownloaded(withTranscription, 'diarization', downloaded.diarization);
}

/** Whether any catalogued model currently reports an in-flight download. */
export function hasActiveAudioCppDownload(models: AudioCppModelSummary[]): boolean {
  return models.some(
    model =>
      typeof model.status === 'object' &&
      model.status !== null &&
      'Downloading' in model.status
  );
}

/** Whether a named catalogued model is installed and usable. */
export function isAudioCppModelInstalled(
  models: AudioCppModelSummary[],
  modelName: string
): boolean {
  return models.some(model => model.name === modelName && model.status === 'Available');
}

/**
 * Restore card state from the native model list when onboarding re-enters.
 *
 * An in-flight download re-appears as `downloading` with its reported
 * percentage, an installed model as `completed`. A card already shown as
 * completed is never rewound by a stale listing, and models other than the two
 * onboarding ones are ignored.
 */
export function reconcileAudioCppModelList(
  states: AudioCppModelStates,
  models: AudioCppModelSummary[]
): AudioCppModelStates {
  let next = states;

  for (const role of ['transcription', 'diarization'] as const) {
    const modelName =
      role === 'transcription' ? AUDIOCPP_TRANSCRIPTION_MODEL : AUDIOCPP_DIARIZATION_MODEL;
    const model = models.find(candidate => candidate.name === modelName);
    if (!model) continue;

    if (model.status === 'Available') {
      next = setAudioCppModelDownloaded(next, role, true);
      continue;
    }

    const downloading =
      typeof model.status === 'object' && model.status !== null && 'Downloading' in model.status;
    if (downloading && next[role].status !== 'completed') {
      next = updateRole(next, role, {
        status: 'downloading',
        progress: {
          ...next[role].progress,
          percent: (model.status as { Downloading: { progress: number } }).Downloading.progress,
        },
      });
    }
  }

  return next;
}
