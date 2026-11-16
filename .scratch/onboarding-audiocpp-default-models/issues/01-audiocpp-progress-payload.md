# Расширить payload события прогресса скачивания audio.cpp-моделей

Status: ready-for-agent
Type: task
Blocked by: —

## Контекст

Спека: `.scratch/onboarding-audiocpp-default-models/spec.md` (решение «События прогресса»).
Событие `audiocpp-model-download-progress` эмитится в `frontend/src-tauri/src/audio_cpp_engine/commands.rs` (`audiocpp_download_model`). Сейчас payload — только `modelName`, `progress` (u8), `status`. Карточки онбординга показывают MB и скорость, как для Parakeet/builtin-ai.

## Задача

Расширить payload события полями `downloaded_mb` (f64), `total_mb` (f64), `speed_mbps` (f64):

- `total_mb` — из каталога (`ModelSpec.size_mb` найденной модели).
- `downloaded_mb` — `progress / 100 * total_mb`.
- `speed_mbps` — считается в closure при эмиссии: дельта скачанных байт / дельта времени между эмиссиями (Instant). Достаточно точности ~0.1 MB/s; первую эмиссию можно со speed 0.0.

Имена полей — snake_case, как у parakeet/builtin-ai событий (`downloaded_mb`, `total_mb`, `speed_mbps`). Контракт событий `audiocpp-model-download-complete` / `audiocpp-model-download-error` не менять. Поведение существующего потребителя (`AudioCppModelManager.tsx` слушает progress с полями modelName/progress/status) не должно сломаться — только добавление полей.

## Тесты (TDD)

Юнит-тесты рядом с кодом (модуль `commands.rs` tests): форма payload — при эмиссии прогресса JSON содержит все пять полей, `status: "completed"` при progress == 100. Чтобы событие было проверяемо без Tauri runtime, вынести построение payload в чистую функцию и тестировать её. Прогон: `cargo test --lib audio_cpp_engine` (крейт `app_lib`, `frontend/src-tauri`).

## Заметки

- Загрузка модели идёт через `AudioCppModelManager::download_model(&model_name, progress)`; closure получает только процент — маппинг в байты делать в closure в `commands.rs`, где известен размер модели из каталога (`catalog::find_model`).
- Не логировать per-chunk; диагностика агрегированная (guardrails репо).
