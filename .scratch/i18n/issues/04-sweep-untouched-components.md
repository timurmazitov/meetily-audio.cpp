# Translate untouched meeting-details / transcript / summary components

Status: needs-triage
Type: task

## Context

Spec: `.scratch/i18n/spec.md` — acceptance: "No user-facing English literals
remain in the translated surfaces (grep-checkable)", except the documented
Latin-script set and other exemptions. Audit section **B**
(`/tmp/opencode/i18n-exploration/audit.md`) lists every literal with path:line.
These files were never touched by the i18n sweep.

## Work

Convert user-facing literals to `t()` keys (add keys to BOTH
`frontend/src/i18n/locales/{en,ru}/translation.json`), following the spec
Vocabulary table and register rules. Files (non-exhaustive, follow the audit):

- `src/components/EmptyStateSummary.tsx`
- `src/components/AISummary/index.tsx`, `src/components/AISummary/Block.tsx`
- `src/components/MeetingDetails/RetranscribeDialog.tsx`
- `src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx`
- `src/components/MeetingDetails/MeetingDetailsSplitView.tsx` (aria-label)
- `src/components/TranscriptView.tsx`, `src/components/VirtualizedTranscriptView.tsx`
- `src/components/ConfidenceIndicator.tsx` (aria-label with interpolation)
- `src/app/_components/TranscriptPanel.tsx`
- `src/app/meeting-details/page-content.tsx` (toasts)

Reuse existing keys where they exist (e.g. `transcript.copy` / `transcript.export`
added by ticket 02, `common.*`, `nav.*`). Components must use `useTranslation()`;
toasts created outside render may use `i18n.t(...)` at call time — match the
pattern used in already-swept files (see `src/components/Sidebar/index.tsx`).

Note: ticket 02 runs in parallel and also edits the catalogs — only ADD keys;
never reformat or reorder existing entries.

## Done when

- None of the listed files contain user-facing English literals (exemptions in
  the spec: Latin-script set, `+ New Call`, document metadata labels,
  `Intl.DisplayNames` content-language names).
- `bun test tests/` in `frontend/` does not get worse; run i18n-parity test.
