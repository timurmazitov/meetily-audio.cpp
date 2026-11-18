# 09: Шаблоны берут дату из блока, а не выдумывают

Type: task
Status: resolved
Blocked by: 08

## Что сделать

Несколько bundled-шаблонов прямо просят у модели дату, и до сих пор модель её выдумывала или писала «None noted in this section». Теперь дата есть в контексте — шаблоны нужно научить, откуда её брать.

Правятся инструкции секций с датой в шаблонах: `daily_standup` (секция `Date`), `project_sync` (`Meeting Date & Time`), `psychatric_session`, `sales_marketing_client_call`, `retrospective`.

Формулировка должна содержать обе половины: «возьми дословно из блока с датой» и «если блока нет — не выдумывай и скажи, что дата неизвестна».

Ограничения:

- Дефолтный базовый системный промпт **не** правится: до пользователей с сохранённым промптом правка не долетит.
- Схема шаблона не меняется — только строки инструкций.
- Формат даты в отчёте остаётся таким, какой просит шаблон.

## Критерии готовности

- [x] Генерирую по `daily_standup` — в секции `Date` настоящая дата встречи, а не выдуманная и не «None noted». — на шве инструкции/промпта (юнит-тесты); живой прогон на реальном LLM-провайдере не делался, по спеке он ручной.
- [x] Без даты в контексте шаблон не заставляет модель её придумывать.
- [x] Все bundled-шаблоны валидны, существующие тесты шаблонов зелёные.
- [x] В release notes отмечено, что уже скопированные пользовательские копии шаблонов правку не получат. — файла release notes в репозитории нет (заметки редактируются в draft-релизе на GitHub, `.github/workflows/release.yml`), поэтому оговорка записана в `frontend/src-tauri/templates/README.md`; саму строку в release notes ещё должен написать тот, кто публикует релиз.

## Comments

- Ветка: `ticket/meeting-date-09` (`c83fcee`, `f06e833`), влита merge-коммитом `3c5f5db`; конфликтов не было.
- Правлены только строки инструкций в пяти шаблонах. `daily_standup` (`Date`): «Meeting date taken from the `<meeting_date>` block in the user message and converted to YYYY-MM-DD (drop its time); if the block is absent, do not invent a date and write that the date is unknown» — дата конвертируется в формат секции, а не копируется дословно.
- `retrospective` (`Sprint`) намеренно устроен иначе: диапазон спринта берётся из транскрипта, а про блок сказано «the `<meeting_date>` block is the meeting's own date, not a substitute for the sprint range, so never fill the range from it». Остальные три (`project_sync`, `psychatric_session`, `sales_marketing_client_call`) берут дату из блока дословно.
- Схема шаблона и дефолтный базовый системный промпт не тронуты; последнее закреплено тестом `default_system_prompt_does_not_reference_the_meeting_date_block`.
- Новых тестов 7 в `summary/templates/mod.rs`. Четыре из шести шаблонов не вкомпилированы в бинарь, поэтому тесты читают файлы из `CARGO_MANIFEST_DIR` (`read_bundled_template`), а не через `list_template_ids()`.
- Про переводы строк: в git все шесть blob'ов — LF (`git ls-files --eol templates/` → `i/lf` для каждого), диф составляет одну строку на файл. CRLF в рабочем дереве — артефакт `core.autocrlf=true` на этой машине, а не правка.
- Проверка: `cargo test --lib` — **662 passed / 0 failed / 7 ignored**.
- Не проверено: живой прогон на реальном LLM-провайдере по `daily_standup` — по спеке он ручной.

