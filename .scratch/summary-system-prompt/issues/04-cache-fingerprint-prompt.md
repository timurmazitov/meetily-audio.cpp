# 04: Эффективный промпт в fingerprint кэша саммари

Type: task
Status: resolved
Blocked by: 03

## Что сделать

- `template_cache_fingerprint` (`frontend/src-tauri/src/summary/service.rs`) расширить так, чтобы он учитывал эффективный системный промпт (после выбора и подстановки плейсхолдеров — то, что реально уходит в системное сообщение). Смена базового или шаблонного промпта должна менять fingerprint и инвалидировать `english_cache`.
- Использовать существующий механизм fingerprint, не строить параллельный.

## Тесты (TDD) — шов 2 из спецификации

По приоритету существующих тестов fingerprint в `service.rs`:

- разные эффективные промпты → разные fingerprint;
- одинаковые входы → одинаковый fingerprint (регрессия не ломает детерминизм).

## Контекст

- Спецификация: `.scratch/summary-system-prompt/spec.md` (Testing Decisions, шов 2; Implementation Decisions, п. про кэш).

## Готовность

`cargo test` в `frontend/src-tauri` зелёный.
## Comments

Реализовано в ветке feature/summary-system-prompt (коммиты 6e9e6c9..2298d28).
