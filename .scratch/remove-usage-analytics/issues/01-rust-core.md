# Удаление Usage Analytics: Rust core

Status: resolved

Spec: [../spec.md](../spec.md)
Research: [docs/research/remove-usage-analytics.md](../../../docs/research/remove-usage-analytics.md)

## Задача

Полностью удалить аналитический код из Rust-ядра Tauri.

## Скоуп

- Удалить каталог `frontend/src-tauri/src/analytics/` (`mod.rs`, `commands.rs`, `analytics.rs`) целиком.
- `frontend/src-tauri/src/lib.rs`: убрать `pub mod analytics;` и все ~25 регистраций аналитических команд в `invoke_handler`.
- `frontend/src-tauri/Cargo.toml`: убрать `posthog-rs = "0.3.7"`; обновить `Cargo.lock` (через `cargo check`).
- `frontend/src-tauri/LOGGING_OPTIMIZATIONS.md`: убрать упоминание `analytics/analytics.rs`.
- Общие крейты (`uuid`, `chrono`, `serde`, `tokio`, `log`) НЕ трогать.
- `src/lib_old_complex.rs` НЕ удалять (out of scope).

## Критерии приёмки

- `cargo check` и `cargo test` в `frontend/src-tauri` проходят.
- `rg -i "posthog|analytics" frontend/src-tauri/src frontend/src-tauri/Cargo.toml` не находит аналитического кода (остаточные нерелевантные совпадения, если вдруг есть, — объяснить).
- Захардкоженного PostHog API key в коде нет.

## Comments

2026-11-17 (implementer):
- Остаточные совпадения `rg -i "posthog|analytics"` в `frontend/src-tauri`:
  - `src/lib_old_complex.rs` — мёртвый файл (не объявлен как модуль, не компилируется), содержит старую копию аналитики и **второй** захардкоженный ключ `phc_cohh…`. Удаление файла — Out of Scope по спеке; ключ требует отдельной ротации.
  - `src/notifications/manager.rs:325` — «for analytics/debugging»: внутренняя диагностика, не Usage Analytics (Out of Scope).
  - `src/tray.rs:94,192` — комментарии про analytics вычищены в рамках тикета.
- Блок `track_meeting_ended` в `src/audio/recording_commands.rs` удалён — он ссылался на удалённый модуль (в research-заметке эта точка не была перечислена, добавлена при верификации компиляции).
- Побочно: `RecordingManager::has_fatal_error` (`recording_manager.rs:536`) остался без продакшн-вызовов — кандидат на удаление в отдельной задаче.
