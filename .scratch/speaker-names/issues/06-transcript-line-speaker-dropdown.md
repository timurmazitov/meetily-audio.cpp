# 06: Дропдаун спикера у реплики транскрипта

Type: task
Status: resolved
Blocked by: 02

## Что сделать

Немедленная правка спикера у одной строки транскрипта.

- `frontend/src/components/VirtualizedTranscriptView.tsx`: чип спикера становится кнопкой, открывающей дропдаун (`frontend/src/components/ui/dropdown-menu.tsx`).
- Состав списка: спикеры **этой** встречи (из `api_get_meeting_speakers`, передать в компонент пропсом, чтобы не вызывать команду на каждую строку) + пункт **No speaker**.
- Выбор вызывает `api_set_transcript_speaker(transcript_id, label | null)` и обновляет сегмент локально; ошибка — `toast.error` без изменения строки.
- Чип показывает имя, если задано, иначе `Speaker N` (номер из суффикса метки). Реплика без метки: дропдаун доступен, текущее значение — «No speaker». Значения `mic`/`system` и свободные строки по-прежнему не отображаются как спикер и не предлагаются.
- Дропдаун доступен только на странице встречи (в живой записи меток ещё нет — не добавлять туда интерактивность).

Не трогать автоскролл, виртуализацию и `enableStreaming` — компонент общий с живой панелью.

## Тесты (TDD)

`frontend/tests/components/diarization-speaker-labels.test.tsx` — расширить существующий файл, сохранив текущие кейсы («нет чипа без метки», «`mic`/`system` игнорируются»):

- чип показывает имя вместо `Speaker N`;
- без имени — `Speaker N`;
- дропдаун содержит спикеров встречи и «No speaker»;
- выбор спикера вызывает `api_set_transcript_speaker` с нужными аргументами;
- в живой панели (`isRecording`) дропдаун не интерактивен.

## Контекст

- Спека: `.scratch/speaker-names/spec.md` — «UI».
- Текущее поведение чипа и его тесты: `frontend/src/components/VirtualizedTranscriptView.tsx:48`, `frontend/tests/components/diarization-speaker-labels.test.tsx`.

## Готовность

`bun test` зелёный; в Tauri-приложении правка строки сохраняется после перезагрузки страницы.

## Comments

Реализовано в ветке `feature/speaker-names` (коммиты `5aff398..363f38d`).
