# 06: UI базового системного промпта на вкладке Summary

Type: task
Status: resolved
Blocked by: 02

## Что сделать

- В `SummaryModelSettings` (вкладка Summary настроек) добавить блок «Base system prompt»:
  - многострочное поле (textarea) без валидации содержимого;
  - при загрузке — значение из `api_get_summary_system_prompt` (команда возвращает сохранённый промпт или дефолт);
  - сохранение — по существующему паттерну сохранения настроек этой вкладки (`api_save_summary_system_prompt`);
  - подсказка о доступных плейсхолдерах: `{section_instructions}` и `{template}`;
  - кнопка «Reset to default», возвращающая текст дефолтного промпта в поле (с сохранением — по паттерну остальных настроек).
- Стиль — по образцу соседних блоков вкладки Summary (язык UI — английский, как весь интерфейс).

## Тесты (TDD) — шов 3 из спецификации

`bun:test` в `frontend/tests/` (по приоритету `tests/components/`):

- поле рендерится с промптом, возвращённым командой (invoke замокан);
- подсказка с плейсхолдерами видна;
- сброс возвращает дефолт и сохраняет его;
- ввод и сохранение вызывают `api_save_summary_system_prompt` с введённым текстом.

## Контекст

- Спецификация: `.scratch/summary-system-prompt/spec.md` (Testing Decisions, шов 3).
- Контракт команд зафиксирован в тикете 02: `api_get_summary_system_prompt` / `api_save_summary_system_prompt`.

## Готовность

`bun test` для затронутых тестов зелёный; `bun run build` (или tsc/lint по аналогии с соседними компонентами) без ошибок.
## Comments

Реализовано в ветке feature/summary-system-prompt (коммиты 6e9e6c9..2298d28).
