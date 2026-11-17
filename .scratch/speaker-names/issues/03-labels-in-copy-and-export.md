# 03: Подписи спикеров в копировании и экспорте

Type: task
Status: resolved
Blocked by: 01

## Что сделать

Привести текст копирования и экспорта к тому, что видно в UI, вместо сырого `speaker_N`.

- `frontend/src/lib/transcript-format.ts:53`: вместо `${t.speaker}: ` подставлять имя спикера; при отсутствии имени — `Speaker N` (1-based, номер из суффикса метки); при отсутствии метки — префикса спикера нет.
- Имя берётся из нового поля `speaker_name`, которое приходит в сегментах после тикета `01`. Фолбэк на расчёт из метки — только для случая «имени нет».
- Легаси-ветка сохраняется байт-в-байт: если ни у одного сегмента нет метки, строки остаются `[MM:SS] text` без диапазона и без префикса.
- Проверить оба вызова: `frontend/src/hooks/meeting-details/useCopyOperations.ts:63` и живой панель `frontend/src/contexts/TranscriptContext.tsx:459`.
- Добавить `speaker_name?: string` в `Transcript` и `TranscriptSegmentData` (`frontend/src/types/index.ts:7`, `:140`).

Не менять: заголовок файла, `markdownLineBreaks`, имя файла и путь экспорта (`frontend/src/lib/transcript-export.ts`).

## Тесты (TDD)

`frontend/tests/lib/transcript-format.test.ts` — текущие ожидания `[00:00–00:24] speaker_0: …` осознанно заменяются. Кейсы:

- имя задано → `[00:00–00:24] Alex: …`;
- имени нет → `[00:00–00:24] Speaker 2: …` (проверить, что номер берётся из метки `speaker_1`, а не из порядка массива);
- сегмент без метки среди именованных → диапазон есть, префикса спикера нет;
- ни одного спикера → формат байт-в-байт прежний;
- `markdownLineBreaks` и заголовок не изменились.

## Контекст

- Спека: `.scratch/speaker-names/spec.md` — «Текст».
- Текущий формат и его тесты: `frontend/src/lib/transcript-format.ts:35`, `frontend/tests/lib/transcript-format.test.ts`.

## Готовность

`bun test` для `tests/lib/transcript-format.test.ts` зелёный; копирование и экспорт в Tauri-приложении дают идентичный текст.

## Comments

Реализовано в ветке `feature/speaker-names` (коммиты `5aff398..363f38d`).
