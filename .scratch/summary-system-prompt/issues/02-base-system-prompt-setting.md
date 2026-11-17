# 02: Настройка «базовый системный промпт» и дефолтная константа

Type: task
Status: ready-for-agent
Blocked by: —

## Что сделать

- Создать модуль `system_prompt` в `frontend/src-tauri/src/summary/` с публичной константой дефолтного системного промпта — точный текст текущего захардкоженного промпта финального отчёта **с плейсхолдерами** `{section_instructions}` и `{template}` (источник: `build_final_report_system_prompt` в `frontend/src-tauri/src/summary/processor.rs`). Сам `processor.rs` пока не трогать — дедупликация произойдёт в тикете 03.
- Хранение: таблица `settings` — посмотреть устройство (миграции в `frontend/src-tauri/migrations/`, репозиторий `database/repositories/setting.rs`): если настройки — колонки, добавить миграцию `ALTER TABLE ... ADD COLUMN` со строковой колонкой для базового промпта; стиль именования — как у соседних колонок.
- Tauri-команды (образец — существующие get/save настроек, регистрация в `lib.rs`):
  - `api_get_summary_system_prompt` → возвращает сохранённое значение, а если NULL/пустое — дефолтную константу;
  - `api_save_summary_system_prompt(prompt: String)` → сохраняет.
- Ленивый seed: отдельная инициализация не нужна, поведение «пусто → дефолт» достигается на чтении.

## Тесты (TDD)

- Репозиторий настроек: save/get roundtrip (по приоритету `migrations_tests.rs` / инлайн-тесты репозитория).
- Команда/логика чтения: при отсутствии значения возвращается константа дефолта.

## Контекст

- Спецификация: `.scratch/summary-system-prompt/spec.md`.
- Заметки: `docs/summary-system-prompt.md`.
- Контракт команд фиксирован и используется тикетом 06: именно `api_get_summary_system_prompt` / `api_save_summary_system_prompt`.

## Готовность

`cargo test` в `frontend/src-tauri` зелёный; `cargo check`/`clippy` без новых предупреждений.
