# Удаление Usage Analytics (PostHog) из приложения

Дата: 2026-11-17. Исследование только для чтения, ничего не изменено.

## Вывод кратко

Аналитика реализована целиком на **PostHog** (эндпойнт `https://us.i.posthog.com`).
Сбор **opt-in**: по умолчанию выключен, включение — переключателем в настройках
(About / Preferences). JS-библиотек аналитики нет — вся отправка идёт через
Rust-ядро (`posthog-rs`). Sentry/Amplitude/Mixpanel/Matomo не используются.
Единственная аналитическая зависимость — `posthog-rs = "0.3.7"`
(`frontend/src-tauri/Cargo.toml:71`).

Захардкоженный PostHog API key: `phc_***`
(`frontend/src-tauri/src/analytics/commands.rs:12`) — после удаления кода его
стоит отозвать/ротировать в кабинете PostHog.

## 1. Файлы с аналитическим кодом

### Ядро Rust (`frontend/src-tauri/`)

| Файл | Строки | Что там |
|---|---|---|
| `src/analytics/analytics.rs` | 1–572 | Весь Rust-клиент: `AnalyticsConfig` (38, дефолтный host `https://us.i.posthog.com`, 48), `AnalyticsClient` (78) с `posthog_rs::Client`, сессии (`UserSession`), санитайзер `SENSITIVE_ANALYTICS_KEYS`/`sanitize_analytics_properties` (9–28), ~30 методов трекинга, хелпер `create_analytics_client` (459), юнит-тесты (463–572). |
| `src/analytics/commands.rs` | 1–373 | Tauri-команды + глобальный статик `ANALYTICS_CLIENT` (7). Захардкожен PostHog API key (12) и host (13). 25 команд: `init_analytics`, `disable_analytics`, `track_event`, `identify_user`, `track_meeting_started/ended`, `track_recording_started/stopped`, `track_meeting_deleted`, `track_settings_changed`, `track_feature_used`, `is_analytics_enabled`, `start/end_analytics_session`, `track_daily_active_user`, `track_user_first_launch`, `track_summary_generation_started/completed`, `track_summary_regenerated`, `track_model_changed`, `track_custom_prompt_used`, `track_analytics_enabled/disabled/transparency_viewed`, `is_analytics_session_active`. |
| `src/analytics/mod.rs` | 1–4 | `pub mod analytics; pub use analytics::*;` |
| `src/lib.rs` | 39, 458, 753–777 | `pub mod analytics;` (39), регистрация всех 25 команд в `invoke_handler` (753–777). |
| `src/lib_old_complex.rs` | 11, 25, 47, 1814–1913+ | **Мёртвый код**: старая монолитная копия с собственным `init_analytics`. В сборку не подключён (не объявлен как модуль). **Удалён** вместе с этим изменением. |
| `LOGGING_OPTIMIZATIONS.md` | 69 | Упоминание `analytics/analytics.rs`. |

### TS-клиент (`frontend/src/lib`)

| Файл | Строки | Что там |
|---|---|---|
| `src/lib/analytics.ts` | 1–857 | Класс `Analytics` — обёртка над Tauri `invoke`: init/disable/isEnabled/track/identify, сессии, Tauri-store `analytics.json` (user_id, счётчики), детект устройства, `trackPageView`, `trackButtonClick`, `trackError`, `trackBackendConnection`, `trackTranscriptionError/Success` (с дедупликацией), summary/model/custom-prompt трекинг. |

### Инициализация / провайдер / контекст (UI)

| Файл | Строки | Что там |
|---|---|---|
| `src/components/AnalyticsProvider.tsx` | 1–151 | React-провайдер, оборачивает всё приложение. Читает `analytics.json` (`analyticsOptedIn`, миграция `analyticsDefaultOffMigrationV1`), при opt-in: user id → `Analytics.init()` → identify → startSession → `trackSessionStarted` → `checkAndTrackFirstLaunch` → `trackAppStarted` → `checkAndTrackDailyUsage` → cleanup на `beforeunload`. Экспортирует `AnalyticsContext` (`isAnalyticsOptedIn`, `setIsAnalyticsOptedIn`). |
| `src/app/layout.tsx` | 8, 236, 277 | Подключение `<AnalyticsProvider>` в корневом layout. |
| `src/components/AnalyticsConsentSwitch.tsx` | 1–~230 | UI-переключатель «Usage Analytics»: заголовок (152), описание (154), switch (160–170), показ user id + копирование, `track_analytics_transparency_viewed` (61), `track_analytics_enabled/disabled` (111/120), запись в `analytics.json` (78–85). |
| `src/components/AnalyticsDataModal.tsx` | 1–~160 | Модалка «What Analytics Collects» — описание собираемых данных и кнопки включения/выключения. |
| `src/components/About.tsx` | 4, 110 | Рендерит `<AnalyticsConsentSwitch />`. |
| `src/components/PreferenceSettings.tsx` | 7–8, 32–45, 102, 128, 223–225 | Секция `{/* Analytics Section */}` с `<AnalyticsConsentSwitch />`; трекает `preferences_viewed`, `notification_settings_changed`, `storage_folder_opened`. |

### Точки вызова трекинга (UI / хуки)

| Файл | Строки | События |
|---|---|---|
| `src/app/page.tsx` | 11, 66 | `trackPageView('home')` |
| `src/app/meeting-details/page.tsx` | 7, 200 | `trackPageView('meeting_details')` |
| `src/app/meeting-details/page-content.tsx` | 6, 146 | `trackPageView('meeting_details')` |
| `src/contexts/ConfigContext.tsx` | 8, 399–410 | `Analytics.track('beta_feature_toggled', …)` |
| `src/contexts/RecordingPostProcessingProvider.tsx` | 17 | только комментарий |
| `src/components/Sidebar/index.tsx` | 12, 207, 235, 339, 406 | `trackSettingsChanged('model_config'/'transcript_config')`, `trackMeetingDeleted`, `trackButtonClick('edit_meeting_title')` |
| `src/components/Sidebar/SidebarProvider.tsx` | 5, 123, 127, 189 | `trackBackendConnection(true/false, …)`, `trackButtonClick('start_recording')` |
| `src/components/RecordingControls.tsx` | 13, 168, 265, 288, 393, 419, 422, 449 | `trackTranscriptionSuccess`, `trackTranscriptionError` ×2, `trackButtonClick` (start/resume/pause/stop) |
| `src/components/DeviceSelection.tsx` | 10, 156–177 | `microphone_selected`, `system_audio_selected` |
| `src/components/RecordingSettings.tsx` | 6, 85, 109–132 | `auto_save_recording_toggled`, `default_devices_changed`, `recording_notification_preference_changed` |
| `src/components/LanguageSelection.tsx` | 3, 164–166 | `language_selected` |
| `src/components/MeetingDetails/TranscriptButtonGroup.tsx` | 7, 48, 63, 78 | `trackButtonClick`: copy_transcript, open_recording_folder, enhance_transcript |
| `src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx` | 6, 30, 54 | save_changes, copy_summary |
| `src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx` | 20, 92, 106 | stop_summary_generation, generate_summary |
| `src/components/MeetingDetails/SummaryPanel.tsx` | 9, 308 | regenerate_summary |
| `src/components/MeetingDetails/RetranscribeDialog.tsx` | 25, 153, 180, 234, 252 | enhance_transcript_completed/started/failed |
| `src/lib/recordingNotification.tsx` | 2, 46 | `recording_notification_acknowledged` |
| `src/hooks/useRecordingStop.ts` | 10, 373, 383, 388–436 | `view_meeting_from_toast`, `trackPageView`, `trackMeetingCompleted`, `updateMeetingCount`, `user_activated` |
| `src/hooks/useImportAudio.ts` | 4, 105, 137, 240, 261 | import_audio_started/completed/failed |
| `src/hooks/meeting-details/useSummaryGeneration.ts` | 13, 143, 342, 349 | `trackSummaryGenerationCompleted/Started`, `trackCustomPromptUsed` |
| `src/hooks/meeting-details/useModelConfiguration.ts` | 5, 126, 151 | `trackModelChanged`, `trackSettingsChanged` |
| `src/hooks/meeting-details/useTemplates.ts` | 4, 38 | `trackFeatureUsed('template_selected')` |

### Тесты (`frontend/tests/`)

| Файл | Что там |
|---|---|
| `tests/lib/analytics.test.ts` | Дедикейтед тесты модуля аналитики |
| `tests/hooks/meeting-details-refresh.test.tsx` | мок `src/lib/analytics` (7, 14, 30) |
| `tests/hooks/summary-generation.test.tsx` | мок аналитики (8, 12, 21–22), тесты «completion analytics…» (201, 257) |
| `tests/components/recording-controls-stop.test.tsx` | мок аналитики (4, 27) |
| `tests/components/device-selection.test.tsx` | мок аналитики (3, 9) |
| `tests/components/logo-doodle.test.tsx` | мок `trackBackendConnection` (13, 39) |
| `tests/components/language-selection-provider.test.tsx` | 6 — комментарий |

### Зависимости / конфиг

- `frontend/src-tauri/Cargo.toml:71` — `posthog-rs = "0.3.7"`.
- `Cargo.lock` — `posthog-rs` (4610), зависимость пакета приложения (5446).
- `package.json` / `pnpm-lock.yaml` — аналитических пакетов нет
  (`@opentelemetry/api` в lock — транзитивная зависимость Next.js, не связана).
- `tauri.conf.json`, `.github/workflows`, `scripts/`, `llama-helper/` — упоминаний нет;
  CI-секретов аналитики нет (API key захардкожен).
- `docs/`, `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `.scratch/` — аналитика не упоминается.

## 2. Архитектура механизма

1. **Инициализация**: `AnalyticsProvider` (в `layout.tsx`) при монтировании читает
   Tauri-store `analytics.json`. Если `analyticsOptedIn === true` →
   `Analytics.getPersistentUserId()` → `Analytics.init()` → `invoke('init_analytics')` →
   Rust создаёт `posthog_rs::Client` с захардкоженным key/host и кладёт в глобальный
   `ANALYTICS_CLIENT`. Далее identify + startSession + `session_started`,
   `user_first_launch`, `app_started`, `daily_active_user`.
2. **Сбор**: ~25 точек вызова на фронтенде; TS-обёртка всегда вызывает Rust-команды;
   без `identify` события отбрасываются. Rust добавляет `app_version`, `session_id`,
   `session_duration`; санитайзер вырезает чувствительные ключи.
3. **Отправка**: `posthog_rs` → `https://us.i.posthog.com`, capture-события
   (`$identify`, `$set`, `session_*`, `app_started`, `daily_active_user`,
   `user_first_launch`, `user_activated`, `meeting_*`, `recording_*`,
   `summary_*`, `model_changed`, `custom_prompt_used`, `settings_changed`,
   `feature_used`, `page_view_*`, `button_click_*`, `transcription_*`,
   `backend_connection`, `beta_feature_toggled`, `preferences_viewed`,
   `language_selected`, `microphone_selected`, `system_audio_selected`,
   `import_audio_*`, `enhance_transcript_*`, `*_changed`, `*_copied`,
   `analytics_enabled/disabled/transparency_viewed`).
4. **Пользовательская настройка**: opt-in, по умолчанию выключен (принудительно
   сбрасывается миграцией `analyticsDefaultOffMigrationV1`). Состояние — в
   `analytics.json`; тот же файл — локальная «аналитическая БД» (user_id, счётчики).
   При выключении: `track_analytics_disabled` → `Analytics.disable()` →
   `invoke('disable_analytics')` (обнуляет глобальный клиент).
5. **Завершение сессии**: `beforeunload` → `trackSessionEnded` → `end_analytics_session`.

## 3. Зависимости, которые станут не нужны

- Rust: **`posthog-rs = "0.3.7"`** → убрать из `Cargo.toml` + перегенерировать `Cargo.lock`.
- `uuid`, `chrono`, `serde`, `tokio`, `log` в `analytics.rs` — общие крейты, **не удалять**.
- npm: ничего. `@tauri-apps/plugin-store` остаётся (используется для `preferences.json`).
- Захардкоженный PostHog API key станет не нужен → отозвать/ротировать в PostHog.

## 4. План полного удаления

### Шаг 1 — Rust core
- Удалить каталог `frontend/src-tauri/src/analytics/` (`mod.rs`, `commands.rs`, `analytics.rs`).
- `src/lib.rs`: убрать `pub mod analytics;` (39) и все 25 регистраций команд (753–777).
- `Cargo.toml`: убрать `posthog-rs`; обновить `Cargo.lock` (`cargo check`).
- Мёртвый `src/lib_old_complex.rs` удалён (файл не был объявлен как модуль).
- `LOGGING_OPTIMIZATIONS.md:69` — убрать упоминание.

### Шаг 2 — TS-клиент
- Удалить `frontend/src/lib/analytics.ts`.
- Убрать вызовы во всех ~25 файлах из раздела «Точки вызова» (импорты + вызовы `Analytics.*`).
  Особое внимание: `ConfigContext.tsx` (beta_feature_toggled), `useSummaryGeneration.ts`,
  `useRecordingStop.ts` (включая локальный подсчёт встреч/`user_activated`),
  `SidebarProvider.tsx` (trackBackendConnection), `RecordingControls.tsx`
  (transcription error/success).

### Шаг 3 — UI
- Удалить `AnalyticsProvider.tsx`, `AnalyticsConsentSwitch.tsx`, `AnalyticsDataModal.tsx`.
- `layout.tsx`: убрать импорт (8) и обёртку `<AnalyticsProvider>` (236, 277).
- `About.tsx` (4, 110) и `PreferenceSettings.tsx` (7–8, 223–225): убрать рендер
  переключателя и секцию «Analytics», а также трекинг `preferences_viewed` /
  `notification_settings_changed` / `storage_folder_opened`.
- `betaFeatures.ts`: убрать пункт 3 про аналитику в комментарии.

### Шаг 4 — Тесты
- Удалить `tests/lib/analytics.test.ts`.
- Из остальных 5 тестовых файлов убрать моки `src/lib/analytics` и аналитические
  ассерты (в `summary-generation.test.tsx` тесты «…analytics…» на 201, 257 — удалить/переписать).

### Шаг 5 — Данные и пост-задачи
- Локальный `analytics.json` (app data dir) станет мусором — при желании одноразовая
  очистка при миграции (не обязательно).
- Отозвать/ротировать PostHog API key `phc_***`; второй старый ключ был в удалённом `lib_old_complex.rs` — тоже ротировать.
- Документация аналитику не упоминает — обновлять не нужно.

### Проверка
- `bun test` в `frontend/` (в `package.json` нет скрипта `test`; раннер — bun:test), `cargo test`/`cargo check` в `frontend/src-tauri/`,
  затем `pnpm run tauri:dev` — убедиться, что приложение стартует и ничего не отправляется
  (можно временно поднять HTTP-прокси/файрвол и проверить отсутствие запросов к posthog).

**Замечание по покрытию**: слова «stats/metric/event/diagnostic» в остальном коде
(audio pipeline stats, notification statistics, transcription segments) — внутренняя
диагностика, к Usage Analytics не относится, трогать не нужно.
