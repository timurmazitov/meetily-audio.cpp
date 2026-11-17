# Удаление Usage Analytics: фронтенд (TS-клиент, вызовы, UI, тесты)

Status: resolved

Spec: [../spec.md](../spec.md)
Research: [docs/research/remove-usage-analytics.md](../../../docs/research/remove-usage-analytics.md)

## Задача

Полностью удалить аналитический код из фронтенда: TS-клиент, точки вызова, UI-компоненты, тесты.

## Скоуп

- Удалить `frontend/src/lib/analytics.ts`.
- Убрать импорты и вызовы `Analytics.*` во всех файлах из раздела «Точки вызова» research-заметки (~25 файлов: страницы, сайдбар, RecordingControls, DeviceSelection, RecordingSettings, LanguageSelection, MeetingDetails-компоненты, RetranscribeDialog, recordingNotification, хуки). В `useRecordingStop` убрать также локальный подсчёт встреч и `user_activated`.
- Удалить `frontend/src/components/AnalyticsProvider.tsx`, `AnalyticsConsentSwitch.tsx`, `AnalyticsDataModal.tsx`; снять обёртку `<AnalyticsProvider>` в корневом layout; убрать рендер переключателя из About и PreferenceSettings вместе с трекингом `preferences_viewed`/`notification_settings_changed`/`storage_folder_opened`.
- Убрать аналитические упоминания в комментариях (`betaFeatures.ts`, `RecordingPostProcessingProvider.tsx`, `language-selection-provider.test.tsx`).
- Тесты: удалить `frontend/tests/lib/analytics.test.ts`; из 5 тестовых файлов убрать моки `src/lib/analytics` и аналитические ассерты (в `summary-generation.test.tsx` тесты «…analytics…» удалить или переписать без аналитики).
- `@tauri-apps/plugin-store` оставить (используется для `preferences.json`).

## Критерии приёмки

- Прогон `bun test` в `frontend/` проходит (в `package.json` скрипта `test` нет; раннер — `bun:test`).
- `npx tsc --noEmit` проходит. `pnpm run lint` не запускается без первичной настройки ESLint.
- `rg -i "analytics" frontend/src frontend/tests` не находит аналитического кода/UI-текстов (оставшиеся совпадения объяснить).
- В UI нет раздела Usage Analytics.

## Замечание

Работа идёт параллельно с тикетом 01 (Rust core) — файловые множества не пересекаются (`frontend/src-tauri` vs `frontend/src` + `frontend/tests`). Перед сдачей влить интеграционную ветку, если она ушла вперёд.
