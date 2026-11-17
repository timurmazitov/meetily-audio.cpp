# 01: Таблицы имён спикеров и резолв имени в чтении

Type: task
Status: resolved
Blocked by: —

## Что сделать

Ввести хранение имён спикеров и отдать имя в существующих ответах чтения.

- Миграция `frontend/src-tauri/migrations/20261118000000_add_speaker_names.sql` — ровно как в спецификации (`.scratch/speaker-names/spec.md`, раздел «Данные»): таблицы `speakers` и `meeting_speakers`, три индекса. Имена индексов — как в спеке.
- Модели в `frontend/src-tauri/src/database/models.rs`: `Speaker` и `MeetingSpeaker` в стиле существующих `FromRow`-структур.
- Репозиторий `frontend/src-tauri/src/database/repositories/speaker.rs` (новый модуль, зарегистрировать в `repositories/mod.rs`): чтение спикеров встречи одним запросом (`DISTINCT speaker` по `transcripts` c `LEFT JOIN` на `meeting_speakers` и `speakers`), резолв `speaker → speaker_name` для сегментов.
- В `frontend/src-tauri/src/database/repositories/meeting.rs:134` (`get_meeting_transcripts_paginated`) и в `get_meeting` подтянуть имя для каждого сегмента.
- Аддитивное поле `speaker_name: Option<String>` в `MeetingTranscript` (`frontend/src-tauri/src/api/api.rs:132`) и в `Transcript`-ответах; заполнить его в `api_get_meeting` и `api_get_meeting_transcripts` (`api.rs:918`).
- `GLOSSARY.md`: добавить термин «имя спикера (speaker name)» с определением из спеки. Определение «спикера» не менять.

Ограничения: `transcripts.speaker` остаётся меткой `speaker_N`, схема колонки не меняется. Учитываются только метки вида `speaker_N`; `mic`/`system` и свободные строки имён не получают.

## Тесты (TDD)

В стиле существующих файлов:

- `frontend/src-tauri/src/database/migrations_tests.rs` — миграция применяется к базе со старыми транскриптами; старые строки читаются без изменений и без имени; уникальные индексы срабатывают: дубль `(meeting_id, label)`, дубль `(meeting_id, speaker_id)`, и `speakers.name` при вставке `Alex`/`alex`.
- `speaker.rs` (инлайн `#[cfg(test)]`) — резолв имени по метке; список спикеров встречи содержит только `speaker_N` и считает реплики; сегмент без метки не попадает в список.

## Контекст

- Спека: `.scratch/speaker-names/spec.md` — «Данные», «Команды».
- Образец миграции: `frontend/src-tauri/migrations/20251223000000_add_meeting_notes.sql`.

## Готовность

`cargo test` в `frontend/src-tauri` зелёный для database-модулей; существующие тесты API-контрактов не сломаны.

## Comments

Реализовано в ветке `feature/speaker-names` (коммиты `5aff398..363f38d`).
