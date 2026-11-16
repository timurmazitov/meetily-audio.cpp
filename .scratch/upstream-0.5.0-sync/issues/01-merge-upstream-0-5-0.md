# Влить upstream-фиксы сборки: тег v0.5.0 и upstream/main

Status: resolved
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

## Answer

Status: resolved. Влито двумя merge-коммитами на ветке `impl/01-merge` от базы
`588e898` (`feat/upstream-0.5.0-sync`).

### Merge-коммиты

| # | Хеш | Что влито |
| --- | --- | --- |
| 1 | `45d5175` | `v0.5.0` (тег `9e50dc2`) — CI-фиксы сборки, без конфликтов |
| 2 | `ba19819` | `upstream/main` (`2166bc6`) — три пост-теговых коммита, два конфликта |

Remote `upstream` уже был настроен (`https://github.com/Zackriya-Solutions/meetily.git`);
сделан `git fetch upstream --tags`. Тег `v0.5.0` присутствует, `upstream/main` = `2166bc6`.

Первый merge: 10 файлов, ровно те, что в `git diff e991463 v0.5.0`
(`.github/workflows/{build,build-devtest,build-linux,build-macos,build-windows,release}.yml`,
`frontend/{build,dev}-gpu.{sh,bat}`). Конфликтов не было, как и предсказано аудитом.

### Разрешение конфликтов

**`frontend/src-tauri/tauri.conf.json`** — принята сторона форка целиком
(`shortDescription` / `longDescription` про Samobranka). Формулировки upstream
(«AI meeting note taker», «Meetily Community Edition») отклонены. Остальные поля
(`publisher: samobranka.ai`, `homepage: https://samobranka.ai`, `productName`,
`identifier: ai.samobranka.app`) не были в конфликте и остались форковыми.
Проверено: `productName=Samobranka`, `identifier=ai.samobranka.app`, `plugins` ключа нет
(updater отключён по ADR 0002) — JSON валиден.

**`README.md`** — по правилу «бренд побеждает, структура берётся»:

- сохранён форковый раздел установки с единственным источником ссылок —
  `https://github.com/timurmazitov/samobranka/releases/latest`;
- из upstream взята **структура**: Windows и macOS объединены в одну таблицу
  («Windows and macOS»), вместо прежних двух отдельных нумерованных списков;
- из upstream взято **примечание про glibc**: «Requires x86_64 Linux with glibc 2.35 or
  newer (.deb preferred, otherwise AppImage).»;
- **не перенесено**: строка `curl -fsSL https://raw.githubusercontent.com/Zackriya-Solutions/meetily/main/scripts/install-linux.sh | sh`,
  весь блок `<details>` с опциями установщика (`--appimage`, `--uninstall`, `--help`)
  и upstream-ссылки на конкретные релизные ассеты `v0.5.0`.

Итог: в README ноль вхождений `meetily` и ноль ссылок на `Zackriya-Solutions`.

**`scripts/install-linux.sh`** — merge принёс его как новый файл (304 строки). Удалён и
зафиксирован в сообщении merge-коммита: `REPO="Zackriya-Solutions/meetily"`, чужие пути
`~/.local/share/com.meetily.ai`, `~/.config/meetily`, `~/Documents/meetily-recordings`,
чужой бинарь `meetily.AppImage`. В дереве отсутствует (`git ls-tree -r HEAD | grep -c install-linux.sh`
→ `0`). Перебрендированный установщик — вне этой спеки.

### Верификация (выполнено реально)

1. **brand-lint** — `python3 scripts/check-brand.py` → `OK: no old-brand occurrences outside
   allowed zones`, exit `0`.
   *Примечание о способе прогона:* в git-worktree `.git` — это **файл** (`gitdir: …`), а
   `check-brand.py` ищет по `os.walk` и исключает `.git` только как **каталог**. Поэтому
   прямой прогон в worktree даёт ложное срабатывание
   `FAIL: 1 old-brand occurrence(s): .git:1: gitdir: /home/…/.git/worktrees/wt-01-merge`.
   Это артефакт worktree, не содержимого репозитория. Гейт прогнан на точном дереве коммита:
   `git archive HEAD | tar -x -C /tmp/opencode/brandcheck-01` и уже там
   `python3 scripts/check-brand.py` → OK. В обычном (не-worktree) клоне скрипт отработает
   напрямую. Файл `docs/research/upstream-0.5.0.md` уже в allowlist (`588e898`), поэтому
   ожидаемого падения из тикета 02 не возникло.
2. **fork-only шаги живы** — staging рантайма audio.cpp присутствует в
   `build.yml` (строки 289/303) и `build-windows.yml` (701/714), включая проверку файлов
   `audiocpp.dll`, … , `audio.cpp-LICENSE.txt`. `SAMOBRANKA_*` секреты/переменные на месте:
   `build.yml` 2, `build-macos.yml` 4, `build-linux.yml` 2, `build-windows.yml` 2,
   `build-devtest.yml` 2. Вхождений `MEETILY_RSA` / `MEETILY_DOODLE` / `MEETILY_DEBUG` /
   `MEETILY_RELEASE` — `0` (grep по `precise`-шаблону вернул пусто), артефактов `name: meetily-*` — `0`.
3. **upstream-фиксы пришли** —
   `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL` есть в `frontend/build-gpu.sh`, `frontend/dev-gpu.sh`,
   `frontend/build-gpu.bat`, `frontend/dev-gpu.bat` и во всех пяти build-workflow;
   пин `dtolnay/rust-toolchain@1.99.1` — в `build.yml`, `build-devtest.yml`, `build-linux.yml`,
   `build-macos.yml`, `build-windows.yml` (вхождений `@stable` не осталось);
   `validate-release` — `runs-on: ubuntu-24.04` с комментарием про minisign и
   `sudo apt-get install -y minisign` в шагах.
4. **`scripts/install-linux.sh` в дереве отсутствует** — см. выше, `0` совпадений.
5. **`git diff --stat 588e898..HEAD`** — 11 файлов, диф не выходит за CI/скрипты/README:
   `.github/workflows/{build,build-devtest,build-linux,build-macos,build-windows,release}.yml`,
   `README.md`, `frontend/{build,dev}-gpu.{sh,bat}`. Rust/TS/БД/миграции не тронуты.
6. Дополнительно: все 8 workflow-файлов успешно парсятся `yaml.safe_load` (PyYAML).

### Что осталось НЕПРОВЕРЕНО

- **Реальный прогон CI / `Release` workflow.** Локально невозможно. Не подтверждено, что
  macOS-сборка действительно проходит на OpenSSL 4, что пин 1.99.1 устраняет падения
  `tauri-build`/`ort-sys` на всех раннерах, и что `validate-release` на `ubuntu-24.04`
  находит `minisign`. Требуется первый прогон на GitHub-раннерах.
- **`cargo check` / `cargo test --lib` и проверка типов фронтенда не запускались** — диф
  в этих тикетах не трогает Rust/TS (только YAML, shell/bat и README), поэтому прогон счёл
  избыточным; отдельный агент / CI должен подтвердить.
- **`git archive`-прогон brand-lint**, а не прямой прогон в worktree (см. примечание выше);
  эквивалентность содержимого обеспечена тем, что архив собран из `HEAD`.
- **Локальная незакоммиченная правка README владельца** из спек («удаление встраиваемых
  изображений») в этом worktree отсутствовала — переносить было нечего; если она ещё живёт
  в основном репозитории, её нужно сверить с результатом вручную.

### Что важно следующему агенту

- Тикет 02 (brand-lint allowlist) **не нужен** для прохождения гейта: `docs/research/upstream-0.5.0.md`
  уже в `EXCLUDED_FILES` на `588e898`. Если 02 всё же правит allowlist под пути — сверь,
  чтобы `EXCLUDED_FILES` продолжал матчить по полному пути.
- `scripts/install-linux.sh` отсутствует намеренно: не возвращать его merge-ом и не
  добавлять без собственного `REPO`/путей/имён ассетов Samobranka.
- Пин `1.99.1` снимать только вместе с обновлением `tauri-build` и `ort` (см. комментарий в
  workflow и `## Заметки`).
