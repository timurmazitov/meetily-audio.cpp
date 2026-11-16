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

function loadTsModule(filePath) {
  const source = fs.readFileSync(filePath, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2020,
    },
  }).outputText;

  const module = { exports: {} };
  vm.runInNewContext(compiled, {
    exports: module.exports,
    module,
    require,
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
  hasActiveAudioCppDownload,
  initialAudioCppModelStates,
  isAudioCppModelInstalled,
  reconcileAudioCppModelList,
  setAudioCppModelDownloaded,
  setAudioCppModelsDownloaded,
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
  const downloaded = setAudioCppModelDownloaded(
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
    stateOf(setAudioCppModelDownloaded(initialAudioCppModelStates(), 'transcription', true), 'transcription'),
    {
      status: 'completed',
      percent: 100,
      downloadedMb: 0,
      totalMb: 0,
      speedMbps: 0,
    }
  );
  assert.equal(
    setAudioCppModelDownloaded(downloading, 'transcription', false).transcription.status,
    'downloading',
    'clearing an undownloaded flag must not reset an in-flight download'
  );
  assert.equal(
    setAudioCppModelDownloaded(
      setAudioCppModelDownloaded(initialAudioCppModelStates(), 'transcription', true),
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
  const reset = setAudioCppModelDownloaded(failed, 'diarization', false);
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
  const next = setAudioCppModelsDownloaded(initialAudioCppModelStates(), {
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
  const completed = setAudioCppModelsDownloaded(initialAudioCppModelStates(), {
    transcription: true,
    diarization: false,
  });
  const next = reconcileAudioCppModelList(completed, [
    { name: AUDIOCPP_TRANSCRIPTION_MODEL, status: { Downloading: { progress: 10 } } },
  ]);
  assert.equal(next.transcription.status, 'completed');
}

console.log('onboarding-audiocpp-status tests passed');
