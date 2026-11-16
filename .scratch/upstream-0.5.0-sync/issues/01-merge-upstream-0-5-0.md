# Влить upstream-фиксы сборки: тег v0.5.0 и upstream/main

Status: ready-for-agent
Type: task
Blocked by: —

## Контекст

Спека: `.scratch/upstream-0.5.0-sync/spec.md`, разделы «Solution» и «Implementation Decisions» (правило «бренд побеждает, структура берётся»).

Аудит: `docs/research/upstream-0.5.0.md`, разделы 4 и 6. Ключевой факт: база форка — родитель `e991463` (upstream-merge `release-v0.5.0`), и `e991463` является предком тега `v0.5.0`. Поэтому `git diff e991463 v0.5.0` — это **10 файлов CI/сборочных скриптов**, и только их форк не содержит. Весь содержательный релиз (119 файлов) уже влит и перебрендирован.

Чего не хватает: `4cf4836` (llama-helper без OpenSSL), `21de5b5` (пин Rust 1.99.1), `e03a15b` (`validate-release` на ubuntu-24.04). Плюс три пост-теговых коммита в `upstream/main` — структура README и примечание про glibc.

## Задача

1. Убедиться, что remote `upstream` = `https://github.com/Zackriya-Solutions/meetily.git` существует; если нет — добавить. Сделать `git fetch upstream --tags`.
2. Влить **двумя отдельными merge-коммитами**, в этом порядке:
   - `git merge v0.5.0` — ожидается без конфликтов;
   - `git merge upstream/main` — ожидаются ровно два конфликта: `README.md` и `frontend/src-tauri/tauri.conf.json`.
3. Разрешить конфликты по правилу из спеки:
   - **`tauri.conf.json`** — принять сторону форка целиком (`shortDescription` / `longDescription` про Samobranka). Формулировки upstream («note taker», «Meetily Community Edition») не принимать.
   - **`README.md`** — сохранить раздел установки Samobranka со ссылками на `https://github.com/timurmazitov/samobranka`; из upstream взять **структуру** (Windows и macOS одной таблицей) и примечание про glibc 2.35 для Linux. Строку `curl ... install-linux.sh | sh` и upstream-ссылки на релизы **не переносить**.
4. **`scripts/install-linux.sh` не должен оказаться в дереве.** Он адресует upstream-репозиторий, чужие пути данных (`com.meetily.ai`, `~/.config/meetily`, `meetily-recordings`) и чужой бинарь. Если merge принёс его как новый файл — удалить и зафиксировать решение в сообщении merge-коммита.

## Тесты (TDD)

Тестового шва у этой работы нет — это слияние, а не код. Верификация обязательна и выполняется вручную, результат приложить к `## Answer`:

- `python3 scripts/check-brand.py` → OK (если падает на `docs/research/upstream-0.5.0.md` — это ожидаемо, тикет 02 это чинит; тогда приложить вывод и указать, что остальные вхождения чистые).
- Проверить, что **fork-only шаги живы**: staging рантайма audio.cpp и `SAMOBRANKA_*` секреты присутствуют в слитых workflow.
- Проверить, что **upstream-фиксы пришли**: `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL` в `frontend/build-gpu.sh` и `frontend/dev-gpu.sh`; пин тулчейна во всех build-workflow; `validate-release` на образе с `minisign`.
- Проверить, что в дереве нет `scripts/install-linux.sh`.
- `git diff --stat <base>..HEAD` — убедиться, что диф не выходит за пределы CI/скриптов и README.

## Заметки

- Пин тулчейна оправдан локом форка (`tauri-build 2.6.1`, `ort 2.0.0-rc.10`). Снимать его — только вместе с обновлением этих зависимостей; в этом тикете не снимать.
- Коммиты после тега — только README и `install-linux.sh`; содержательного кода в них нет, поэтому риск конфликта в Rust/TS равен нулю.
- Не вливать `origin/devtest`: на момент аудита он побайтово равен `upstream/main`.
