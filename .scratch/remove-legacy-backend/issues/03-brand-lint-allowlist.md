Status: resolved
Blocked by: 01

# 03: Убрать исключение backend/ из brand-lint

## Задача

В `scripts/check-brand.py` удалить из `EXCLUDED_DIRS` строку `"backend"` вместе с комментарием «legacy archive». Остальные исключения и паттерны не менять.

Важно: выполнять только после тикета 01 — пока папка `backend/` существует, она содержит строки старого бренда (например, `meetily-backend` в скриптах setup-db), и lint упадёт без исключения.

## Верификация

- `python3 scripts/check-brand.py` из корня репо → `OK: no old-brand occurrences outside allowed zones` (шов 2 из спеки).

## Контекст

- Спека: `.scratch/remove-legacy-backend/spec.md`
- Исследование: `docs/research/backend-legacy-deletion.md`

## Out of scope

Любые другие правки lint-скрипта (allowlist-паттерны, `EXCLUDED_FILES`, `ALLOWED_FILES`).

## Resolution

Закрыто в рамках ветки `chore/remove-legacy-backend` (2026-11-17). См. git log main..HEAD и спеку `.scratch/remove-legacy-backend/spec.md`.
