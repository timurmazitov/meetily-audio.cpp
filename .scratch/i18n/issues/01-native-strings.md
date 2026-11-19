# Translate native (Rust) strings

Status: needs-triage
Type: task

## Context

The interface localization (`.scratch/i18n/spec.md`) covers the frontend. Text
that originates in Rust stays English, because it is not produced by
react-i18next.

## Strings that remain English

- Tray menu — `frontend/src-tauri/src/tray.rs:331-395`:
  `Start Recording`, `Pause Recording`, `Settings`, `Check for Updates`, `Quit`,
  `⏳ Downloading transcription model...`.
- Native notifications — `frontend/src-tauri/src/notifications/types.rs:144-195`:
  `Recording has started. Please inform others…`,
  `Recording has been stopped and saved`,
  `Meeting '{}' starts in {} minutes`, `Samobranka Error`.
- Rust-originated error prose surfaced in the UI, e.g.
  `frontend/src-tauri/src/audio_cpp_engine/transcription.rs:73-104`
  (`Download GigaAM v3 in Settings → …`),
  `frontend/src-tauri/src/audio_cpp_engine/session_thread.rs:187`.

## Why it is separate work

Localizing these needs a Rust-side string table, the active locale passed to
Rust, and re-rendering triggered tray and notification text on language change.
The better long-term shape is for Rust to return error codes or categories that
the frontend maps to translation keys, rather than prose.

## Suggested approach

1. Read `app_language` from `preferences.json` in Rust at startup and on change.
2. Add a small Rust message catalog for tray and notifications.
3. Rebuild the tray menu when the language changes.
4. Convert the most visible Rust error strings to codes the frontend translates;
   leave the rest as logs/fallback prose.
