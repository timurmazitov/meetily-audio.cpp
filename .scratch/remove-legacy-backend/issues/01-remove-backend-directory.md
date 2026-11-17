Status: resolved

# 01: Удалить папку backend/ и сабмодуль whisper.cpp

## Задача

Полностью удалить `backend/` из репозитория, включая git-сабмодуль `backend/whisper.cpp` (единственный сабмодуль репо; `.gitmodules`: url `https://github.com/Zackriya-Solutions/whisper.cpp`, branch `develop`).

## Как

- `git rm backend/whisper.cpp` (чистит индекс от сабмодуля).
- Удалить секцию `[submodule "backend/whisper.cpp"]` из `.gitmodules`; если других секций нет — удалить сам файл.
- Удалить остальное содержимое `backend/` (FastAPI `app/`, Docker-файлы и скрипты, `whisper-custom/`, `examples/`, документация архива, скрипты запуска).
- Проверить, что под `.git/modules/` не осталось сиротского gitdir сабмодуля (в текущей копии `.git/modules` чист — удаление сабмодуля не должно ничего оставить; если gitdir остался, удалить его как часть этой задачи).
- Обычный коммит. Историю не переписывать.

## Верификация

- `git submodule status` — пусто (нет записей).
- `git ls-files backend/` — пусто.
- `.gitmodules` отсутствует или не содержит секций.
- `git status` после коммита чист (кроме не относящихся к задаче untracked-файлов).

## Контекст

- Спека: `.scratch/remove-legacy-backend/spec.md`
- Исследование: `docs/research/backend-legacy-deletion.md`
- Эту задачу блокирует ничего; она блокирует тикеты 03 и 04 (нельзя убирать исключение brand-lint и упоминания в доках, пока папка на месте).

## Out of scope

Любые правки кода вне удаления каталога/сабмодуля/`.gitmodules` — это тикеты 02–04.

## Resolution

Закрыто в рамках ветки `chore/remove-legacy-backend` (2026-11-17). См. git log main..HEAD и спеку `.scratch/remove-legacy-backend/spec.md`.
