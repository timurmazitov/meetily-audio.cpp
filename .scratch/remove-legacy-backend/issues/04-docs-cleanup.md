Status: ready-for-agent
Blocked by: 01

# 04: Убрать упоминания backend/ из документации

## Задача

После удаления папки (тикет 01) убрать формулировки про `backend/` как legacy-архив из:

- `AGENTS.md` — буллет «`backend/` is an unsupported legacy archive…» в Project Overview.
- `CONTRIBUTING.md` — фраза про `backend/` как unsupported legacy archive.
- `docs/architecture.md` — абзац про Python/FastAPI под `backend/` (раздел про frontend/native communication).
- `frontend/README.md` — упоминание legacy backend.

Предложения переписывать, а не оставлять с дырами; связность текста сохранить.

## Не трогать

- `docs/research/*` и `.scratch/*` — исторические документы, упоминания `backend/` там факт истории.
- Пользовательские каталоги данных со словом «backend» в database-коде — не документация.

## Верификация

- `grep -rn "backend/" AGENTS.md CONTRIBUTING.md docs/architecture.md frontend/README.md` — нет ссылок на удалённую папку (пользовательские пути в database-коде не входят в эту проверку; если упоминание осталось осмысленным — объяснить в отчёте).
- Маркдаун-ссылки в изменённых файлах валидны (нет битых относительных ссылок на удалённые файлы `backend/`).

## Контекст

- Спека: `.scratch/remove-legacy-backend/spec.md`
- Исследование: `docs/research/backend-legacy-deletion.md`
