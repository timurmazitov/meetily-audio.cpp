# Translate hook/context toasts and leftovers in already-swept files

Status: needs-triage
Type: task

## Context

Spec: `.scratch/i18n/spec.md`. Audit sections **A** and **C**
(`/tmp/opencode/i18n-exploration/audit.md`) list every literal with path:line.

## Work

Convert to `t()` / `i18n.t(...)` at call time (existing pattern for toasts —
see `src/components/Sidebar/index.tsx` and `src/lib/recordingNotification.tsx`);
add keys to BOTH catalogs:

Section C — hooks and contexts (English toasts/alerts):
- `src/hooks/useRecordingStart.ts` (9 sites, incl. `alert('Failed to start recording…')`)
- `src/hooks/useRecordingStop.ts` (4 sites)
- `src/hooks/useImportAudio.ts:124`, `src/hooks/useTranscriptRecovery.ts:191`
- `src/hooks/meeting-details/useSummaryGeneration.ts` (12 toasts)
- `src/hooks/meeting-details/useModelConfiguration.ts` (3)
- `src/hooks/meeting-details/useMeetingData.ts` (2)
- `src/hooks/meeting-details/useTemplates.ts:34`
- `src/hooks/useSpeakerNamesResetListener.ts:61`
- `src/hooks/useModalState.ts:179`
- `src/contexts/OllamaDownloadContext.tsx` (2)
- `src/lib/diarization.ts:177`

Section A — leftovers in modified files:
- `src/components/FeedbackLink.tsx:30` — visible label `Send Feedback`
  (reuse `nav.feedback`)
- `src/components/Sidebar/index.tsx` — 4 `toast.error` literals + tooltip
  `Import Audio` (reuse `nav.importAudio`)
- `src/components/Sidebar/MeetingFileItem.tsx:96` — `Match:` label
- `src/hooks/meeting-details/useCopyOperations.ts` — 6 toast literals
- `src/components/ImportAudio/ImportAudioDialog.tsx:332` — format list is
  borderline; leave format tokens as-is (Latin exemption), no action needed
  unless surrounding prose is English.

Hooks that are not components: import the `i18n` instance and use `i18n.t(key)`
inside the toast/callback bodies (call time, not module scope — module-scope
one-shot `t()` is a bug the previous sweep avoided; keep it that way).

Note: ticket 04 runs in parallel and also edits the catalogs — only ADD keys;
never reformat or reorder existing entries.

## Done when

- Listed literals gone; `bun test tests/` does not get worse; i18n-parity passes.
