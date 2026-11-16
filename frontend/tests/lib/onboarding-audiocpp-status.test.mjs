import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'src',
  'lib',
  'onboarding-audiocpp-status.ts'
);
const require = createRequire(import.meta.url);

// Loads a `src/lib` TypeScript module, resolving relative imports between
// those modules through the same transpiler (they are not compiled on disk).
const moduleCache = new Map();

function loadTsModule(filePath) {
  const resolved = path.resolve(filePath);
  const cached = moduleCache.get(resolved);
  if (cached) return cached.exports;

  const source = fs.readFileSync(resolved, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2020,
    },
  }).outputText;

  const module = { exports: {} };
  moduleCache.set(resolved, module);

  const localRequire = specifier => {
    if (specifier.startsWith('.')) {
      return loadTsModule(path.resolve(path.dirname(resolved), `${specifier}.ts`));
    }
    return require(specifier);
  };

  vm.runInNewContext(compiled, {
    exports: module.exports,
    module,
    require: localRequire,
  });
  return module.exports;
}

const {
  AUDIOCPP_DIARIZATION_MODEL,
  AUDIOCPP_TRANSCRIPTION_MODEL,
  applyAudioCppCompleteEvent,
  applyAudioCppErrorEvent,
  applyAudioCppProgressEvent,
  audioCppModelRole,
  audioCppRetryModelName,
  formatAudioCppModelSizeLabel,
  hasActiveAudioCppDownload,
  initialAudioCppModelStates,
  isAudioCppModelInstalled,
  isContinueEnabled,
  markAudioCppCardError,
  onboardingDownloadRetryTarget,
  reconcileAudioCppModelList,
  resolveTranscriptionGate,
  seedAudioCppModelSizes,
  setAudioCppCardDownloaded,
  setAudioCppCardsDownloaded,
  shouldShowBackgroundDownloadNotice,
  startAudioCppModelDownload,
} = loadTsModule(modulePath);

function progressEvent(overrides = {}) {
  return {
    modelName: AUDIOCPP_TRANSCRIPTION_MODEL,
    progress: 40,
    downloaded_mb: 168.8,
    total_mb: 422,
    speed_mbps: 12.5,
    status: 'downloading',
    ...overrides,
  };
}

function stateOf(states, role) {
  return {
    status: states[role].status,
    percent: states[role].progress.percent,
    downloadedMb: states[role].progress.downloadedMb,
    totalMb: states[role].progress.totalMb,
    speedMbps: states[role].progress.speedMbps,
    ...(states[role].error === undefined ? {} : { error: states[role].error }),
  };
}

const idleTranscription = {
  status: 'waiting',
  percent: 0,
  downloadedMb: 0,
  totalMb: 0,
  speedMbps: 0,
};
const idleDiarization = { ...idleTranscription };

assert.equal(AUDIOCPP_TRANSCRIPTION_MODEL, 'gigaam-v3-e2e-ctc-f16');
assert.equal(AUDIOCPP_DIARIZATION_MODEL, 'nemotron-3-diarization-bf16');

assert.equal(audioCppModelRole(AUDIOCPP_TRANSCRIPTION_MODEL), 'transcription');
assert.equal(audioCppModelRole(AUDIOCPP_DIARIZATION_MODEL), 'diarization');
assert.equal(audioCppModelRole('parakeet-tdt-0.6b-v3-int8'), null);

assert.deepEqual(stateOf(initialAudioCppModelStates(), 'transcription'), idleTranscription);
assert.deepEqual(stateOf(initialAudioCppModelStates(), 'diarization'), idleDiarization);

// A GigaAM progress report moves only the transcription card.
{
  const next = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  assert.deepEqual(stateOf(next, 'transcription'), {
    status: 'downloading',
    percent: 40,
    downloadedMb: 168.8,
    totalMb: 422,
    speedMbps: 12.5,
  });
  assert.deepEqual(stateOf(next, 'diarization'), idleDiarization);
}

// A Nemotron progress report moves only the diarization card.
{
  const next = applyAudioCppProgressEvent(
    initialAudioCppModelStates(),
    progressEvent({
      modelName: AUDIOCPP_DIARIZATION_MODEL,
      progress: 75,
      downloaded_mb: 1500,
      total_mb: 2000,
      speed_mbps: 3.5,
    })
  );
  assert.deepEqual(stateOf(next, 'transcription'), idleTranscription);
  assert.deepEqual(stateOf(next, 'diarization'), {
    status: 'downloading',
    percent: 75,
    downloadedMb: 1500,
    totalMb: 2000,
    speedMbps: 3.5,
  });
}

// 100% progress marks the matching card completed and downloaded.
{
  const next = applyAudioCppProgressEvent(
    initialAudioCppModelStates(),
    progressEvent({ progress: 100, status: 'completed' })
  );
  assert.equal(next.transcription.status, 'completed');
  assert.equal(next.transcription.progress.percent, 100);
  assert.equal(next.diarization.status, 'waiting');
}

// Cancellation resets the matching card and clears its progress.
{
  const downloaded = setAudioCppCardDownloaded(
    initialAudioCppModelStates(),
    'transcription',
    true
  );
  const next = applyAudioCppProgressEvent(
    downloaded,
    progressEvent({ progress: 0, status: 'cancelled' })
  );
  assert.deepEqual(stateOf(next, 'transcription'), {
    status: 'cancelled',
    percent: 0,
    downloadedMb: 0,
    totalMb: 0,
    speedMbps: 0,
  });
}

// A completion event for Nemotron does not claim the transcription card.
{
  const next = applyAudioCppCompleteEvent(initialAudioCppModelStates(), {
    modelName: AUDIOCPP_DIARIZATION_MODEL,
  });
  assert.deepEqual(stateOf(next, 'diarization'), {
    status: 'completed',
    percent: 100,
    downloadedMb: 0,
    totalMb: 0,
    speedMbps: 0,
  });
  assert.deepEqual(stateOf(next, 'transcription'), idleTranscription);
}

// A completion event for GigaAM does not claim the diarization card.
{
  const next = applyAudioCppCompleteEvent(initialAudioCppModelStates(), {
    modelName: AUDIOCPP_TRANSCRIPTION_MODEL,
  });
  assert.equal(next.transcription.status, 'completed');
  assert.equal(next.diarization.status, 'waiting');
}

// Foreign model names are ignored by every event kind.
{
  const initial = initialAudioCppModelStates();
  const foreign = { modelName: 'parakeet-tdt-0.6b-v3-int8' };
  assert.deepEqual(
    applyAudioCppProgressEvent(initial, { ...progressEvent(), ...foreign }),
    initial
  );
  assert.deepEqual(applyAudioCppCompleteEvent(initial, foreign), initial);
  assert.deepEqual(
    applyAudioCppErrorEvent(initial, { ...foreign, error: 'boom' }),
    initial
  );
}

// An error event is reported on the matching card only.
{
  const next = applyAudioCppErrorEvent(initialAudioCppModelStates(), {
    modelName: AUDIOCPP_TRANSCRIPTION_MODEL,
    error: 'network unreachable',
  });
  assert.equal(next.transcription.status, 'error');
  assert.equal(next.transcription.error, 'network unreachable');
  assert.equal(next.diarization.status, 'waiting');
  assert.equal(next.diarization.error, undefined);
}

// Downloaded flags update the card without resurrecting a download in flight.
{
  const downloading = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  assert.deepEqual(
    stateOf(setAudioCppCardDownloaded(initialAudioCppModelStates(), 'transcription', true), 'transcription'),
    {
      status: 'completed',
      percent: 100,
      downloadedMb: 0,
      totalMb: 0,
      speedMbps: 0,
    }
  );
  assert.equal(
    setAudioCppCardDownloaded(downloading, 'transcription', false).transcription.status,
    'downloading',
    'clearing an undownloaded flag must not reset an in-flight download'
  );
  assert.equal(
    setAudioCppCardDownloaded(
      setAudioCppCardDownloaded(initialAudioCppModelStates(), 'transcription', true),
      'transcription',
      false
    ).transcription.status,
    'waiting'
  );
}

// Clearing the flag after a failure returns the card to waiting, so a retry
// does not keep showing the previous error.
{
  const failed = applyAudioCppErrorEvent(initialAudioCppModelStates(), {
    modelName: AUDIOCPP_DIARIZATION_MODEL,
    error: 'timeout',
  });
  const reset = setAudioCppCardDownloaded(failed, 'diarization', false);
  assert.deepEqual(stateOf(reset, 'diarization'), idleDiarization);
  assert.equal(reset.diarization.error, undefined);
}

// Active downloads are detected from the native model list.
{
  const models = [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: { Downloading: { progress: 12 } } },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Missing' },
  ];
  assert.equal(hasActiveAudioCppDownload(models), true);
  assert.equal(
    hasActiveAudioCppDownload([
      { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Available' },
      { name: AUDIOCPP_DIARIZATION_MODEL, status: { Error: 'boom' } },
    ]),
    false
  );
  assert.equal(hasActiveAudioCppDownload([]), false);
}

// Installed detection resolves a catalogued model by name.
{
  const models = [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Available' },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Missing' },
  ];
  assert.equal(isAudioCppModelInstalled(models, AUDIOCPP_TRANSCRIPTION_MODEL), true);
  assert.equal(isAudioCppModelInstalled(models, AUDIOCPP_DIARIZATION_MODEL), false);
  assert.equal(isAudioCppModelInstalled(models, 'unknown-model'), false);
}

// Starting a download flips the matching card before the first progress event.
{
  const next = startAudioCppModelDownload(initialAudioCppModelStates(), 'diarization');
  assert.equal(next.diarization.status, 'downloading');
  assert.equal(next.transcription.status, 'waiting');
}

// A saved status applies both flags in one step.
{
  const next = setAudioCppCardsDownloaded(initialAudioCppModelStates(), {
    transcription: true,
    diarization: false,
  });
  assert.equal(next.transcription.status, 'completed');
  assert.equal(next.diarization.status, 'waiting');
}

// Re-entering onboarding restores an in-flight card from the native model list.
{
  const models = [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: { Downloading: { progress: 37 } } },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Missing' },
  ];
  const next = reconcileAudioCppModelList(initialAudioCppModelStates(), models);
  assert.equal(next.transcription.status, 'downloading');
  assert.equal(next.transcription.progress.percent, 37);
  assert.equal(next.diarization.status, 'waiting');
}

// Installed models come back as completed cards.
{
  const next = reconcileAudioCppModelList(initialAudioCppModelStates(), [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Available' },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Available' },
  ]);
  assert.equal(next.transcription.status, 'completed');
  assert.equal(next.diarization.status, 'completed');
}

// A model list with neither onboarding model leaves the cards alone.
{
  const initial = initialAudioCppModelStates();
  assert.deepEqual(reconcileAudioCppModelList(initial, []), initial);
  assert.deepEqual(
    reconcileAudioCppModelList(initial, [
      { name: 'other-model', status: { Downloading: { progress: 5 } } },
    ]),
    initial
  );
}

// A completed card is never rewound to downloading by a stale list.
{
  const completed = setAudioCppCardsDownloaded(initialAudioCppModelStates(), {
    transcription: true,
    diarization: false,
  });
  const next = reconcileAudioCppModelList(completed, [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: { Downloading: { progress: 10 } } },
  ]);
  assert.equal(next.transcription.status, 'completed');
}

// Continue is gated on the transcription (GigaAM) card alone.
{
  const downloading = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  assert.equal(isContinueEnabled(initialAudioCppModelStates()), false);
  assert.equal(isContinueEnabled(downloading), false);

  const transcriptionDone = setAudioCppCardDownloaded(
    initialAudioCppModelStates(),
    'transcription',
    true
  );
  assert.equal(isContinueEnabled(transcriptionDone), true);

  // A failed or cancelled diarization download never blocks Continue.
  const diarizationFailed = applyAudioCppErrorEvent(transcriptionDone, {
    modelName: AUDIOCPP_DIARIZATION_MODEL,
    error: 'boom',
  });
  assert.equal(isContinueEnabled(diarizationFailed), true);

  // An installed diarization model does not stand in for the ASR gate.
  const diarizationOnly = applyAudioCppErrorEvent(
    setAudioCppCardsDownloaded(initialAudioCppModelStates(), {
      transcription: false,
      diarization: true,
    }),
    { modelName: AUDIOCPP_TRANSCRIPTION_MODEL, error: 'boom' }
  );
  assert.equal(isContinueEnabled(diarizationOnly), false);

  const cancelled = applyAudioCppProgressEvent(
    transcriptionDone,
    progressEvent({ progress: 0, status: 'cancelled' })
  );
  assert.equal(isContinueEnabled(cancelled), false);
}

// Card size labels come from the reported payload, not a hardcoded size.
{
  assert.equal(formatAudioCppModelSizeLabel(0), '');
  assert.equal(formatAudioCppModelSizeLabel(-5), '');
  assert.equal(formatAudioCppModelSizeLabel(422), '~422 MB');
  assert.equal(formatAudioCppModelSizeLabel(189.6), '~190 MB');
  assert.equal(formatAudioCppModelSizeLabel(1900), '~1.9 GB');

  const next = applyAudioCppProgressEvent(
    initialAudioCppModelStates(),
    progressEvent({ total_mb: 512 })
  );
  assert.equal(formatAudioCppModelSizeLabel(next.transcription.progress.totalMb), '~512 MB');
  assert.equal(
    formatAudioCppModelSizeLabel(
      applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent({ total_mb: 190 }))
        .transcription.progress.totalMb
    ),
    '~190 MB'
  );
}

// Each card's "Try Again" resolves to its own download entry point.
{
  assert.equal(onboardingDownloadRetryTarget('transcription'), 'audiocpp');
  assert.equal(onboardingDownloadRetryTarget('diarization'), 'audiocpp');
  assert.equal(onboardingDownloadRetryTarget('summary'), 'builtin-ai');
  assert.equal(onboardingDownloadRetryTarget('other'), null);

  // The retry entry point maps back to the catalogued model name.
  assert.equal(onboardingDownloadRetryTarget('transcription'), 'audiocpp');
  assert.equal(audioCppRetryModelName('transcription'), AUDIOCPP_TRANSCRIPTION_MODEL);
  assert.equal(audioCppRetryModelName('diarization'), AUDIOCPP_DIARIZATION_MODEL);
  assert.equal(audioCppRetryModelName('summary'), null);
}

// The background-download notice appears only once the app can be used (GigaAM
// done) and something else is still pending.
{
  assert.equal(
    shouldShowBackgroundDownloadNotice({
      transcriptionDownloaded: false,
      diarizationDownloaded: false,
      summaryDownloaded: false,
    }),
    false
  );
  assert.equal(
    shouldShowBackgroundDownloadNotice({
      transcriptionDownloaded: true,
      diarizationDownloaded: false,
      summaryDownloaded: true,
    }),
    true
  );
  assert.equal(
    shouldShowBackgroundDownloadNotice({
      transcriptionDownloaded: true,
      diarizationDownloaded: true,
      summaryDownloaded: false,
    }),
    true
  );
  assert.equal(
    shouldShowBackgroundDownloadNotice({
      transcriptionDownloaded: true,
      diarizationDownloaded: true,
      summaryDownloaded: true,
    }),
    false
  );
}

// Continue verifies the native ASR gate, catching state drift and failures.
// `proceed` requires the model to be installed on disk: a card that merely
// looks finished (or a download still in flight) must not pass the gate.
{
  const downloading = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: true, state: downloading.transcription }),
    'promote',
    'an installed model must correct a stale card and continue'
  );

  const completed = setAudioCppCardDownloaded(
    initialAudioCppModelStates(),
    'transcription',
    true
  );
  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: true, state: completed.transcription }),
    'proceed'
  );

  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: false, state: completed.transcription }),
    'blocked',
    'a completed-looking card without the model on disk must not proceed'
  );

  const failed = applyAudioCppErrorEvent(initialAudioCppModelStates(), {
    modelName: AUDIOCPP_TRANSCRIPTION_MODEL,
    error: 'boom',
  });
  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: false, state: failed.transcription }),
    'blocked'
  );

  const cancelled = applyAudioCppProgressEvent(
    initialAudioCppModelStates(),
    progressEvent({ progress: 0, status: 'cancelled' })
  );
  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: false, state: cancelled.transcription }),
    'blocked'
  );

  assert.equal(
    resolveTranscriptionGate({ installedOnDisk: false, state: downloading.transcription }),
    'blocked',
    'an in-flight download is not an installed model'
  );
  assert.equal(
    resolveTranscriptionGate({
      installedOnDisk: false,
      state: initialAudioCppModelStates().transcription,
    }),
    'blocked',
    'a waiting card without the model on disk must not proceed'
  );
}

// Card sizes are seeded from the catalog before any progress event arrives.
{
  const next = reconcileAudioCppModelList(initialAudioCppModelStates(), [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Missing', size_mb: 422 },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Missing', size_mb: 1900 },
  ]);
  assert.equal(next.transcription.progress.totalMb, 422);
  assert.equal(next.diarization.progress.totalMb, 1900);
  assert.equal(next.transcription.status, 'waiting');
  assert.equal(next.diarization.status, 'waiting');
  assert.equal(
    formatAudioCppModelSizeLabel(next.transcription.progress.totalMb),
    '~422 MB'
  );

  // A reported size on a downloading card is kept when the catalog agrees.
  const downloading = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  const reconciled = reconcileAudioCppModelList(downloading, [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: { Downloading: { progress: 40 } }, size_mb: 422 },
  ]);
  assert.equal(reconciled.transcription.progress.totalMb, 422);
  assert.equal(reconciled.transcription.progress.percent, 40);

  // A list entry without a size leaves the card's known size untouched.
  const withoutSize = reconcileAudioCppModelList(initialAudioCppModelStates(), [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Missing' },
  ]);
  assert.equal(withoutSize.transcription.progress.totalMb, 0);
}

// Sizes can be seeded from the catalog alone, without touching card statuses.
{
  const initial = initialAudioCppModelStates();
  const seeded = seedAudioCppModelSizes(initial, [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Missing', size_mb: 422 },
    { name: AUDIOCPP_DIARIZATION_MODEL, status: 'Missing', size_mb: 1900 },
  ]);
  assert.equal(seeded.transcription.status, 'waiting');
  assert.equal(seeded.diarization.status, 'waiting');
  assert.equal(seeded.transcription.progress.totalMb, 422);
  assert.equal(seeded.diarization.progress.totalMb, 1900);

  // A size already reported by an event is not overwritten.
  const reported = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  assert.equal(
    seedAudioCppModelSizes(reported, [
      { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: 'Missing', size_mb: 999 },
    ]).transcription.progress.totalMb,
    422
  );

  assert.deepEqual(seedAudioCppModelSizes(initial, []), initial);
}

// A failed retry marks the card as errored again so "Try Again" stays reachable.
{
  const errored = markAudioCppCardError(
    initialAudioCppModelStates(),
    'transcription',
    'network unreachable'
  );
  assert.equal(errored.transcription.status, 'error');
  assert.equal(errored.transcription.error, 'network unreachable');
  assert.equal(errored.diarization.status, 'waiting');
  assert.equal(errored.diarization.error, undefined);

  const previous = applyAudioCppProgressEvent(initialAudioCppModelStates(), progressEvent());
  const failedRetry = markAudioCppCardError(previous, 'transcription', 'timeout');
  assert.equal(failedRetry.transcription.status, 'error');
  assert.equal(failedRetry.transcription.error, 'timeout');
  assert.equal(
    failedRetry.transcription.progress.totalMb,
    422,
    'a failed retry keeps the size already known for the card'
  );
}

console.log('onboarding-audiocpp-status tests passed');
