/**
 * Pure state resolution for the models downloaded during onboarding.
 *
 * The onboarding context receives raw native events and model lists; the
 * mapping to the "Transcription Engine" / "Speaker Diarization" card states
 * lives here so it can be tested without Tauri (see
 * `frontend/tests/lib/onboarding-audiocpp-status.test.mjs`).
 *
 * The transcription card tracks the Transcribe.CPP catalog default model
 * (events `transcribecpp-model-download-*`); the diarization card tracks the
 * audio.cpp Nemotron model (events `audiocpp-model-download-*`). Model names
 * are duplicated from the native catalogs, matching the settings managers.
 * Status predicates are reused from `./audiocpp`; the unit test loader
 * resolves that relative import through the same transpiler.
 */

import i18n, { type AppLanguageCode } from '@/i18n';
import { formatMegabytes } from '@/lib/format-locale';
import { DEFAULT_TRANSCRIBE_CPP_MODEL } from '../constants/modelDefaults';
import {
  downloadProgress,
  isAvailableStatus,
  isDownloadingStatus,
} from './audiocpp';
import type {
  AudioCppDownloadCompleteEvent,
  AudioCppDownloadErrorEvent,
  AudioCppDownloadProgressEvent,
  AudioCppModelInfo,
} from './audiocpp';

/** Transcribe.CPP model onboarding installs for transcription. */
export const ONBOARDING_TRANSCRIPTION_MODEL = DEFAULT_TRANSCRIBE_CPP_MODEL;

/** Catalogued audio.cpp diarization model onboarding installs. */
export const AUDIOCPP_DIARIZATION_MODEL = 'nemotron-3-diarization-bf16';

/** The two onboarding download cards, backed by transcribe.cpp and audio.cpp. */
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
export type AudioCppModelSummary = Pick<AudioCppModelInfo, 'name' | 'status'> &
  Partial<Pick<AudioCppModelInfo, 'size_mb'>>;

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
  if (modelName === ONBOARDING_TRANSCRIPTION_MODEL) return 'transcription';
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

/** Apply a download-complete event for the matching card only. */
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

/** Apply a download-error event for the matching card only. */
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
export function setAudioCppCardDownloaded(
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
 * Mark a card as failed without a native event, e.g. when a retried download
 * request rejects before the manager can emit its own error.
 *
 * The previous size and percentage are kept so the card does not lose the
 * information it already showed.
 */
export function markAudioCppCardError(
  states: AudioCppModelStates,
  role: AudioCppModelRole,
  error: string
): AudioCppModelStates {
  return updateRole(states, role, {
    status: 'error',
    progress: { ...states[role].progress },
    error,
  });
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
export function setAudioCppCardsDownloaded(
  states: AudioCppModelStates,
  downloaded: { transcription: boolean; diarization: boolean }
): AudioCppModelStates {
  const withTranscription = setAudioCppCardDownloaded(
    states,
    'transcription',
    downloaded.transcription
  );
  return setAudioCppCardDownloaded(withTranscription, 'diarization', downloaded.diarization);
}

/**
 * Whether the step's Continue button is enabled.
 *
 * Only the transcription model gates onboarding; diarization downloads in the
 * background and a failure there must never keep the user on this screen.
 */
export function isContinueEnabled(states: AudioCppModelStates): boolean {
  return states.transcription.status === 'completed';
}

/**
 * Size label under a card title, taken from the reported/catalogued size.
 *
 * The `~` is intentional: the catalogued size is an estimate of the download.
 * The model manager's `formatModelSize` prints the exact figure ("422 MB"), so
 * the two labels differ by design. Returns an empty string while no size is
 * known yet, so the card shows just its title until the size is seeded from
 * the catalog or the first progress event arrives.
 */
export function formatAudioCppModelSizeLabel(sizeMb: number): string {
  if (!Number.isFinite(sizeMb) || sizeMb <= 0) return '';
  return `~${formatMegabytes(sizeMb, i18n.language as AppLanguageCode)}`;
}

/** Where a card's "Try Again" button sends its download request. */
export type OnboardingDownloadRetryTarget = 'transcribecpp' | 'audiocpp' | 'builtin-ai';

/** Every card on the download step. */
export type OnboardingDownloadCard = AudioCppModelRole | 'summary';

/** Download entry point for an onboarding card; unknown cards get `null`. */
export function onboardingDownloadRetryTarget(
  card: OnboardingDownloadCard
): OnboardingDownloadRetryTarget | null {
  if (card === 'transcription') return 'transcribecpp';
  if (card === 'diarization') return 'audiocpp';
  if (card === 'summary') return 'builtin-ai';
  return null;
}

/**
 * Catalogued model name a retry re-requests.
 *
 * Neither engine has a separate retry command: downloading is the idempotent
 * entry point, so retrying is the same download call. Returns `null` for the
 * summary card, which keeps its own built-in AI download path.
 */
export function onboardingRetryModelName(role: OnboardingDownloadCard): string | null {
  if (role === 'transcription') return ONBOARDING_TRANSCRIPTION_MODEL;
  if (role === 'diarization') return AUDIOCPP_DIARIZATION_MODEL;
  return null;
}

/**
 * Whether to tell the user they can leave the screen.
 *
 * Only meaningful once the app is usable (transcription installed) while a
 * background download is still pending.
 */
export function shouldShowBackgroundDownloadNotice({
  transcriptionDownloaded,
  diarizationDownloaded,
  summaryDownloaded,
}: {
  transcriptionDownloaded: boolean;
  diarizationDownloaded: boolean;
  summaryDownloaded: boolean;
}): boolean {
  return (
    transcriptionDownloaded &&
    !(diarizationDownloaded && summaryDownloaded)
  );
}

/**
 * Decision for `handleContinue` after re-checking the native ASR gate.
 *
 * `proceed` requires the transcription model to be installed on disk; the card
 * is only a projection of the events and can drift. `promote` corrects a stale
 * card when the model is installed but the card has not caught up; `blocked`
 * means onboarding must stay put: there is no installed ASR model, whether the
 * download is in flight, failed, or never started.
 */
export type TranscriptionGateAction = 'proceed' | 'promote' | 'blocked';

export function resolveTranscriptionGate({
  installedOnDisk,
  state,
}: {
  installedOnDisk: boolean;
  state: AudioCppModelCardState;
}): TranscriptionGateAction {
  if (!installedOnDisk) return 'blocked';
  return state.status === 'completed' ? 'proceed' : 'promote';
}

/** Whether any catalogued model currently reports an in-flight download. */
export function hasActiveAudioCppDownload(models: AudioCppModelSummary[]): boolean {
  return models.some(model => isDownloadingStatus(model.status));
}

/** Whether a named catalogued model is installed and usable. */
export function isAudioCppModelInstalled(
  models: AudioCppModelSummary[],
  modelName: string
): boolean {
  return models.some(
    model => model.name === modelName && isAvailableStatus(model.status)
  );
}

/** Catalogued size of a model from the native list, or `null` if unknown. */
function cataloguedSizeMb(models: AudioCppModelSummary[], modelName: string): number | null {
  const model = models.find(candidate => candidate.name === modelName);
  if (!model) return null;
  const sizeMb = model.size_mb;
  return typeof sizeMb === 'number' && Number.isFinite(sizeMb) && sizeMb > 0 ? sizeMb : null;
}

/**
 * Seed each card's size from the catalog so it is visible before the first
 * progress event arrives.
 *
 * Only empty sizes are filled: a size already reported by an event is at least
 * as accurate. Card statuses are left untouched.
 */
export function seedAudioCppModelSizes(
  states: AudioCppModelStates,
  models: AudioCppModelSummary[]
): AudioCppModelStates {
  let next = states;

  for (const role of ['transcription', 'diarization'] as const) {
    const modelName =
      role === 'transcription' ? ONBOARDING_TRANSCRIPTION_MODEL : AUDIOCPP_DIARIZATION_MODEL;
    if (next[role].progress.totalMb > 0) continue;

    const sizeMb = cataloguedSizeMb(models, modelName);
    if (sizeMb === null) continue;

    next = updateRole(next, role, {
      ...next[role],
      progress: { ...next[role].progress, totalMb: sizeMb },
    });
  }

  return next;
}

/**
 * Restore card state from the native model lists when onboarding re-enters.
 *
 * An in-flight download re-appears as `downloading` with its reported
 * percentage, an installed model as `completed`. The card's size is seeded
 * from the catalog so it is visible before the first progress event. A card
 * already shown as completed is never rewound by a stale listing, and models
 * other than the two onboarding ones are ignored.
 */
export function reconcileAudioCppModelList(
  states: AudioCppModelStates,
  models: AudioCppModelSummary[]
): AudioCppModelStates {
  let next = seedAudioCppModelSizes(states, models);

  for (const role of ['transcription', 'diarization'] as const) {
    const modelName =
      role === 'transcription' ? ONBOARDING_TRANSCRIPTION_MODEL : AUDIOCPP_DIARIZATION_MODEL;
    const model = models.find(candidate => candidate.name === modelName);
    if (!model) continue;

    if (isAvailableStatus(model.status)) {
      next = setAudioCppCardDownloaded(next, role, true);
      continue;
    }

    const percent = downloadProgress(model.status);
    if (percent !== null && next[role].status !== 'completed') {
      next = updateRole(next, role, {
        status: 'downloading',
        progress: {
          ...next[role].progress,
          percent,
        },
      });
    }
  }

  return next;
}
