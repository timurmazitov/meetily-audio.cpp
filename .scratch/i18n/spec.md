# App interface localization (English + Russian)

Status: ready-for-agent

## Problem

The interface is English-only. Every user-facing string is an English literal in
place; `frontend/src/app/layout.tsx:213` hardcodes `<html lang="en">`; there is no
locale setting anywhere (`Setting`, `TranscriptSetting`, `RecordingPreferences`,
`NotificationSettings` all lack one). Users who prefer Russian cannot change the
language of the app.

## Goal

Add an application-language setting with exactly two choices, English and
Russian. The choice is persisted, survives restarts, and switching it retranslates
the whole interface immediately.

## Reference implementation

`https://github.com/cjpais/Handy/tree/main/src/i18n` — i18next +
react-i18next, one `translation.json` per locale, `LANGUAGE_METADATA` with native
names, `app_language` in settings, sync-from-settings on boot. Adapted here for
Next.js static export (no `import.meta.glob`, no Vite glob imports).

## Decisions

1. **Mechanism**: `i18next` + `react-i18next`. Rationale: Russian plural
   categories (`_one`/`_few`/`_many`), interpolation and `<Trans>` come for free;
   a hand-rolled dictionary would get plurals wrong.
2. **Layout**:
   - `frontend/src/i18n/index.ts` — i18n instance, static imports of both locale
     files, `SUPPORTED_LANGUAGES`, `normalizeAppLanguage`.
   - `frontend/src/i18n/languages.ts` — `LANGUAGE_METADATA` (English name, native
     name, priority).
   - `frontend/src/i18n/locales/{en,ru}/translation.json` — the catalogs.
   - `frontend/src/lib/app-language.ts` — read/write the persisted preference.
   - `frontend/src/contexts/LanguageContext.tsx` — `LanguageProvider` +
     `useAppLanguage()`.
3. **Persistence**: frontend-written `preferences.json` through
   `@tauri-apps/plugin-store` (already used for `show_recording_notification`),
   key `app_language`, values `"en"` / `"ru"`. Not `localStorage` (a real file is
   readable from Rust later), not a SQLite column (a migration, a repository
   method and two commands for one string).
4. **Default on first run**: no "System" entry. `navigator.language`'s primary
   subtag is `ru` → Russian, anything else → English. `navigator.language` is
   used because `@tauri-apps/plugin-os` has no Rust half installed here. An
   explicit choice is stored and never re-derived.
5. **No English flash / no hydration mismatch**: `i18n` initializes with `en` so
   the statically exported HTML is deterministic; `LanguageProvider` resolves the
   stored preference after mount and renders the app only once the language is
   known. `<html lang>` follows the active language.
6. **Dates, numbers, units** follow the UI language: Russian renders
   `19.11.2026, 14:32`, `42 мин`, `1 ч 23 мин`, `1,2 ГБ`; English keeps
   `18/11/2026, 14:32`, `42 min`, `1.2 GB`.
7. **Server components**: user-facing parts of `src/app/notes/[id]/page.tsx`
   move to client components under the provider. `src/app/metadata.ts` stays
   English (build-time browser-tab fallback). The window title is updated from
   the client.
8. **Out of scope**: native (Rust) strings — tray menu, native notifications,
   Rust-originated error prose. Tracked in
   `.scratch/i18n/issues/01-native-strings.md`.
9. **Content language stays independent**: the UI language never changes
   transcription or summary output language. Summary language keeps its own
   setting (`src/lib/summary-language-preferences.ts`).
10. **Language names in the picker**: native name, with the English name in
    parentheses only when they differ — `English`, `Русский (Russian)`.
11. **Settings placement**: General tab, first item.
12. **Parity guard**: `frontend/tests/lib/i18n-parity.test.ts` asserts both
    catalogs have the same key tree and no empty values.

## Vocabulary

Follows the repo's `GLOSSARY.md`.

| English | Russian |
| --- | --- |
| Meeting | Встреча |
| Recording | Запись |
| Transcription (process/engine/provider) | Транскрибация |
| Transcript (text) | Транскрипция |
| Summary | Саммари |
| Speaker | Спикер |
| Notes | Заметки |
| Home | Главная |
| Settings | Настройки |
| Feedback | Обратная связь |

Settings tabs: «Общие», «Записи», «Транскрибация», «Саммари».

**Stays in Latin script**: `Samobranka`, model and engine names
(`Whisper Large v3`, `Parakeet`, `GigaAM v3`, `audio.cpp`), format tokens
(`WAV`, `MP3`, `M4A`, `FLAC`, `Markdown`, `PDF`), `API` / «API-ключ», `GPU`,
`CPU`, `macOS`, `Windows`, `Linux`, `URL`, «промпт» / «системный промпт».

**Also stays English, deliberately**:

- The exported summary document's metadata labels (`**Meeting ID:**`,
  `**Date:**`, `**Copied on:**`, `**Exported on:**`) — GLOSSARY.md defines them
  as the document format, and the file is meant to be read outside the app. The
  date *values* inside the document follow the interface language.
- The `+ New Call` placeholder for a not-yet-saved recording. It is a sentinel
  compared in `src/lib/export-file.ts` (`NEW_CALL_TITLE`) to keep the
  placeholder out of export file names; localizing it would require making that
  comparison locale-aware, which is not worth it for one placeholder.
- Content-language names are not in the catalogs either: they come from
  `Intl.DisplayNames` (`src/lib/content-languages.ts`), which knows every tag in
  the hundred-entry Whisper list. English keeps the labels the data files carry.


**Register**: neutral, impersonal. No «вы»/«ты`, no gendered forms, sentence
case, buttons in perfective imperative («Начать запись», «Импортировать аудио»),
errors impersonal («Не удалось загрузить модель»).

## Acceptance

- Switching the language in Settings → General retranslates the interface without
  a restart, in both directions.
- The choice survives an app restart.
- A fresh install on a Russian-locale system opens in Russian; anywhere else in
  English.
- Dates, durations and sizes follow the active language, with correct Russian
  plurals.
- Both catalogs have identical key trees (test).
- No user-facing English literals remain in the translated surfaces
  (`grep`-checkable), except the documented Latin-script set above.
