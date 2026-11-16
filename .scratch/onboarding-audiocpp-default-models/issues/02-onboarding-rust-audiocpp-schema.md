# Onboarding (Rust): схема статуса под audio.cpp + провайдер audioCpp при завершении

Status: resolved
Type: task
Blocked by: —

## Контекст

Спека: `.scratch/onboarding-audiocpp-default-models/spec.md` (решения «Схема onboarding-status.json — полная замена», «Завершение онбординга (Rust)»).
Код: `frontend/src-tauri/src/onboarding.rs`.

## Задача

1. **Схема `ModelStatus` (полная замена)**: удалить поле `parakeet`; добавить поля `gigaam: String` и `diarization: String` (те же значения "downloaded" | "not_downloaded" | "downloading"); `summary` и `selected_summary_model` оставить. `Default` — "not_downloaded" для всех. Старый JSON (с `parakeet`) должен десериализовываться: serde игнорирует неизвестные поля; новые строковые поля без `#[serde(default)]` упадут — добавить default-обработку (default-реализация через `Default` на уровне `ModelStatus` + `#[serde(default)]`).
2. **`complete_onboarding`**: вместо `save_transcript_config(pool, "parakeet", DEFAULT_PARAKEET_MODEL)` писать `save_transcript_config(pool, "audioCpp", "gigaam-v3-e2e-ctc-f16")`. Имя модели — константа (рядом с кодом или из `audio_cpp_engine::catalog`, если там есть публичный доступ по имени; не дублировать строку размазанно). Конфиг builtin-ai (summary) не менять. В `status.model_status` выставлять новые поля: `gigaam = "downloaded"`, `diarization = "downloaded"`.

Проверить, нет ли других мест в Rust, где читается/пишется `model_status.parakeet` (grep `parakeet` в `frontend/src-tauri/src`, кроме движка `parakeet_engine` и его команд) — попутно не сломать.

## Тесты (TDD)

Юнит-тесты в `onboarding.rs` (там есть `mod tests`):
- старый JSON (с `parakeet`, без новых полей) десериализуется, новые поля — "not_downloaded";
- новый JSON сериализуется и читается обратно с новыми полями;
- маппинг «onboarding → transcript config»: вынести выбор провайдера/модели в маленькую чистую функцию (например `onboarding_transcript_config() -> (&'static str, &'static str)`) и тестировать её, чтобы не тащить БД в тест.

Прогон: `cargo test --lib onboarding` (крейт `app_lib`, `frontend/src-tauri`).

## Заметки

- Фронтенд-зеркало схемы (`OnboardingStatus` в `OnboardingContext.tsx`) обновляется в тикете 03 — здесь только Rust.
- Пользователи с `completed: true` не затрагиваются: `loadOnboardingStatus` доверяет завершённому статусу.

## Result

Implemented on `feat/onboarding-audiocpp-default-models`; review fixes in `feat/onboarding-audiocpp-review-fixes`. Integration tip after review: `1e03c73`.
