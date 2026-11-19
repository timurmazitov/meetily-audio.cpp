# Add missing speakers/summary/transcript catalog namespaces

Status: resolved
Type: task

## Context

Spec: `.scratch/i18n/spec.md`. Audit: `/tmp/opencode/i18n-exploration/audit.md`
(BUG-1 section lists every call site with path:line).

Modified meeting-details components call `t()` on ~55 keys in three namespaces
(`speakers.*`, `summary.*`, `transcript.*`) that exist in **neither**
`frontend/src/i18n/locales/en/translation.json` nor `ru/translation.json`.
i18next falls back to rendering the raw key path, so the meeting-details screen
shows strings like `speakers.title` / `summary.generate` / `transcript.copyTooltip`.

## Work

- Add all keys from audit BUG-1 to both catalogs, following the spec Vocabulary
  table and register rules (neutral, no вы/ты, sentence case, perfective
  imperatives on buttons). Keep interpolation placeholders identical between
  languages (the parity test checks this).
- Call sites (do not change code unless a key call is plainly wrong):
  - `src/components/MeetingDetails/SpeakersPanel.tsx`
  - `src/components/MeetingDetails/TranscriptButtonGroup.tsx`
  - `src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx`
  - `src/components/MeetingDetails/SummaryPanel.tsx`
  - `src/components/MeetingDetails/TranscriptPanel.tsx` (`summary.promptPlaceholder`)
- After adding keys, run from `frontend/`: `bun test tests/` — the
  `speakers-panel.test.tsx` cluster (27 fails) and the summary/transcript
  copy/export cluster must go green. Report any failures that are NOT caused by
  these missing keys (those belong to ticket 06).

## Done when

- No raw key paths render in meeting-details; parity test passes; the
  key-gap-caused test failures are gone.
