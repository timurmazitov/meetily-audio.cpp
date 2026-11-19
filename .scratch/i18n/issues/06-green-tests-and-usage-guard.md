# Get bun test suite green and add usage-vs-catalog guard

Status: needs-triage
Type: task
Blocked by: 02, 04, 05

## Context

Spec: `.scratch/i18n/spec.md` — acceptance: catalogs identical key trees (test),
no user-facing English literals. Audit BUG-3: `bun test tests/` reported
328 pass / 61 fail at audit time; most failures were missing catalog keys
(tickets 02/04/05), but some are stale assertions.

## Work

1. Run from `frontend/`: `bun test tests/`. Fix every remaining failure:
   - stale assertions that encode pre-i18n behavior (e.g.
     `tests/lib/audiocpp.test.ts` expects `"422MB"`, the formatter now emits
     `"422 MB"` — update expectations to the intended new behavior, verify
     against the implementation, do not weaken tests);
   - i18n-caused failures if any were left by earlier tickets
     (e.g. `panel-meeting-date` family, `logo-doodle`,
     `summary-system-prompt-settings`, `recording-controls-stop`).
2. Add a **usage-vs-catalog guard test** (e.g.
   `frontend/tests/lib/i18n-usage.test.ts`): scan `frontend/src/**/*.{ts,tsx}`
   for literal `t('a.b')` / `i18n.t('a.b')` key strings and assert every key
   exists in both catalogs (dynamic/template-string keys exempt via an
   allowlist). This catches BUG-1-class regressions the parity test cannot.

## Done when

- `bun test tests/` is fully green; report the final pass/fail count.
