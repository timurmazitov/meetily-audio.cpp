Status: ready-for-agent

# 02: Почистить Rust-ядро: мёртвые fallback-пути и тест-фикстуры

## Задача

Убрать из поддерживаемого кода `frontend/src-tauri` единственные две ссылки на пути `backend/...`:

1. **Мёртвые fallback-ветки в движке Whisper.** В резолве каталога моделей (под `cfg!(debug_assertions)`, защищены `.exists()`) удалить ветки `backend/whisper-server-package/models` и `../backend/whisper-server-package/models` вместе с их комментариями. Каталога нет в репо — ветки мёртвые. Порядок резолва для prod (models_dir из app-data-dir) не менять.
2. **Условные тест-фикстуры в тестах аудио-импорта.** Три теста (`test_extract_duration_from_metadata_wav`, `test_extract_duration_from_metadata_mp3`, `test_validate_audio_file_with_metadata`) сейчас обёрнуты в `if test_path.exists()` и ссылаются на `../../backend/whisper.cpp/samples/jfk.wav/.mp3` — сабмодуль не инициализирован, тесты молча пропускаются. Сделать тесты самодостаточными: генерировать крошечный валидный WAV на лету в тесте (предпочтительно, чтобы не плодить бинарники в git) и убрать guard `exists()`. Ассерты сохранить. Транскрибация/модели не нужны — тесты проверяют только метаданные/валидацию файла. Если генерация WAV окажется нетривиальной для существующего парсера (например, нужен конкретный формат заголовка), допустим закоммиченный минимальный `.wav` в testdata crate'а — решение зафиксировать в комментарии тикета.

## Верификация

- `cargo test` в `frontend/src-tauri` (шов 1 из спеки). Внимание окружению: в этом клоне проект ещё не собирался, `target/` нет, LLVM/libclang для bindgen не настроен (см. docs/BUILDING.md). Если сборка невозможна без подготовки toolchain — выполнить возможное, а ограничение честно зафиксировать в отчёте (никаких выдуманных результатов).
- `python3 scripts/check-brand.py` из корня — не сломан чужими правками.

## Контекст

- Спека: `.scratch/remove-legacy-backend/spec.md` (раздел Implementation Decisions)
- Исследование: `docs/research/backend-legacy-deletion.md`
- Не трогать: пути/именования со словом «backend» в database-коде (живые пути данных), prod-резолв моделей, event/command-контракты.

## Out of scope

Удаление самой папки `backend/` (тикет 01), brand-lint (03), документация (04).
