# 05: Панель Speakers

Type: task
Status: resolved
Blocked by: 02

## Что сделать

Панель выбора имён для спикеров встречи.

- Кнопка **Speakers** в `frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx`: иконка + подпись, подпись скрывается на узкой ширине, как у соседних кнопок. Кнопка видна всегда.
- Панель — правый `Sheet` (`frontend/src/components/ui/sheet.tsx`): транскрипт остаётся виден.
- Содержимое: строки спикеров встречи из `api_get_meeting_speakers` — подпись `Speaker N`, число реплик, поле выбора. Пустое состояние без меток: текст-подсказка про модель диаризации (`DIARIZATION_UNAVAILABLE_HINT` в `frontend/src/lib/diarization.ts:63` или его переиспользование).
- Поле выбора: Popover + `Command` по образцу `frontend/src/components/ModelSettingsModal.tsx:897`. Список: «Без имени», совпадения из `api_get_speaker_names`, и — только когда точного совпадения нет — первая строка «Создать „Мария“». Точное совпадение (`trim` + без учёта регистра) выбирает существующего, пункта «Создать» нет.
- Кнопки **Done** / **Cancel**. Done вызывает `api_assign_meeting_speakers`; после успеха — перезагрузка сегментов и `toast.success`. Escape и клик вне панели отменяют локальные изменения.
- Ошибку бэкенда (например, «этот человек уже сопоставлен спикеру Speaker 1») показывать у соответствующей строки и держать панель открытой.
- Кнопки Listen нет.

## Тесты (TDD)

`frontend/tests/components/` — по образцу `tests/components/diarization-speaker-labels.test.tsx` (react-test-renderer + `mock.module` для Tauri и Radix):

- рендер строк спикеров с числом реплик;
- выбор существующего имени и вызов Done с ожидаемым payload;
- ввод нового имени показывает строку «Создать …» и создаёт назначение;
- точное совпадение не предлагает создать;
- «Без имени» уходит в payload как `null`;
- Escape отбрасывает изменения — `api_assign_meeting_speakers` не вызван;
- пустое состояние без меток показывает подсказку.

## Контекст

- Спека: `.scratch/speaker-names/spec.md` — «UI».
- Шаблон combobox: `frontend/src/components/ModelSettingsModal.tsx:897`; popover-список с поиском: `frontend/src/components/LanguagePickerPopover.tsx:88`.
- Строки — английские литералы на месте (i18n в проекте нет).

## Готовность

`bun test` по компоненту зелёный; в Tauri-приложении имена назначаются, сохраняются по Done и видны в транскрипте после перезагрузки страницы.

## Comments

Реализовано в ветке `feature/speaker-names` (коммиты `5aff398..363f38d`).
