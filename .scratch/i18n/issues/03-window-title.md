# Update the window title from the client

Status: resolved
Type: task

## Context

Spec: `.scratch/i18n/spec.md`, decision 7: "The window title is updated from the
client." Audit BUG-2: no `document.title`, `setTitle`, or
`getCurrentWindow()` call exists anywhere in `frontend/src/`. The title stays
whatever `src-tauri/tauri.conf.json` sets.

## Work

- On language change (and on mount), set the window title from the client so it
  follows the active language. Use the translated brand/title key (add
  `app.title` to both catalogs — the brand `Samobranka` stays Latin in both
  locales per spec; if the title string is identical in both languages, wire the
  mechanism anyway so future titles localize).
- Prefer `document.title` plus, when running under Tauri,
  `getCurrentWindow().setTitle()` from `@tauri-apps/api/window` (guard so the
  static-export browser build does not crash; dynamic import is fine — see how
  `src/lib/app-language.ts` guards Tauri imports).
- Put the effect in or near `LanguageContext.tsx` so it re-runs on language
  switch.

## Done when

- Changing the language updates the window/document title without restart;
  browser (`pnpm run dev`) build does not crash.
