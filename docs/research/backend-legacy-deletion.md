# backend/: нужна ли папка и что сломается при её удалении (исследование первоисточников)

Дата исследования: 2026-11-16. Репозиторий: `samobranka`, рабочая копия `/home/timur/sources/samobranka`.

## Краткий вывод

**Нет, поддерживаемому приложению папка `backend/` не нужна.** Это подтверждено документацией репозитория (`AGENTS.md:11`, `CONTRIBUTING.md:3`, `docs/architecture.md:45`, `frontend/README.md:133`), и в поддерживаемом коде `frontend/` есть ровно две реальные ссылки на пути `backend/...` — обе защищены проверками `exists()` и обе уже сейчас не срабатывают (сабмодуль `backend/whisper.cpp` не инициализирован, каталога `backend/whisper-server-package/` в репозитории нет). Ни сборка (`pnpm run tauri:dev` / `tauri:build`), ни CI, ни runtime-поведение приложения на `backend/` не зависят. Единственные «поломки» при удалении: потеряются dev-only fallback-пути в `whisper_engine.rs` (уже мёртвые), тестовые фикстуры в `audio/import.rs` (уже skip-аются, т.к. файлов нет), и потребуется аккуратное git-удаление сабмодуля + чистка allowlist в `scripts/check-brand.py` и упоминаний в документации/спеках.

---

## 1. Статус `backend/` по документации репозитория

- `AGENTS.md:11` — «`backend/` is an unsupported legacy archive, not a runtime dependency or a place to add new application behavior. Its FastAPI/CORS behavior is not a supported production API.»
- `CONTRIBUTING.md:3` — «Current application work belongs in `frontend/` and its Rust/Tauri core; `backend/` is an unsupported legacy archive.»
- `docs/architecture.md:45` — «The Python/FastAPI, Docker, and standalone whisper-server code under `backend/` is an unsupported legacy archive»; приложение работает через Tauri commands/events, а не через отдельный FastAPI-сервис.
- `frontend/README.md:133` — «Current Samobranka does not require a separate FastAPI service, Docker backend, or manually started whisper-server process. Local transcription is handled by the Rust/Tauri desktop app.»
- Содержимое `backend/` (по листингу каталога): FastAPI-приложение `app/`, `docker-compose.yml`, `Dockerfile.app`, `Dockerfile.server-cpu/gpu/macos`, скрипты `build-docker.sh`, `run-docker.sh`, `start_python_backend.cmd`, `setup-db.sh`, `requirements.txt`, сабмодуль `whisper.cpp`, каталоги `docker/`, `examples/`, `whisper-custom/`. Всё это — standalone Python/whisper-server инфраструктура, не задействованная Tauri-приложением.

## 2. Ссылки на `backend/` в поддерживаемом коде `frontend/`

Полный поиск по `frontend/` (включая `package.json`, `tauri.conf.json`, `src/`, `src-tauri/src/`, `tests/`) даёт всего **две** ссылки на пути вида `backend/...` (остальные совпадения по слову «backend» относятся к Rust/Tauri-бэкенду приложения, а не к папке `backend/`):

### 2.1. Fallback-пути моделей в WhisperEngine — `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:104–146`

```rust
// Development: Use frontend/models or backend directories
// Production: Use system directories (should be overridden by caller)
if cfg!(debug_assertions) {
    // Development mode - try frontend and backend directories
    if current_dir.join("models").exists() {
        current_dir.join("models")
    } else if current_dir.join("../models").exists() {
        ...
    } else if current_dir.join("backend/whisper-server-package/models").exists() {   // строка 127
        current_dir.join("backend/whisper-server-package/models")
    } else if current_dir.join("../backend/whisper-server-package/models").exists() { // строка 129
        current_dir.join("../backend/whisper-server-package/models")
    } else {
        // Create models directory in current directory for development
        current_dir.join("models")
    }
```

Факты:

- Это **dev-only** ветка (guard `cfg!(debug_assertions)`), применяется только когда вызывающий код передал `models_dir = None` — `whisper_engine.rs:99` (`Self::new_with_models_dir(None)`).
- Каждая ветка защищена `.exists()`; если каталога нет, выбирается финальный fallback `current_dir.join("models")` — **ошибки не будет**, просто backend-ветки не сработают.
- Каталога `backend/whisper-server-package/` в репозитории **сейчас нет** (проверено: `ls backend/whisper-server-package` → «No such file or directory»), т.е. эти ветки уже сейчас мёртвые даже без удаления `backend/`.
- Производственный путь: `whisper_init()` в `frontend/src-tauri/src/whisper_engine/commands.rs:43–51` вызывает `get_models_directory()` — глобальный `MODELS_DIR`, устанавливаемый из app-data-dir приложения (`commands.rs:25–34`, лог «Models directory set to: …»). Все тесты движка передают `Some(tempdir)` (`whisper_engine.rs:1066, 1106, …`).

### 2.2. Тестовые фикстуры в `frontend/src-tauri/src/audio/import.rs:1136, 1153, 1165`

Тесты внутри `#[cfg(test)]`-модуля (`import.rs:1089`) используют `../../backend/whisper.cpp/samples/jfk.wav` и `jfk.mp3`:

```rust
let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.wav");
if test_path.exists() {                       // guard: тест — no-op без файла
    let result = extract_duration_from_metadata(test_path);
    ...
}
```

Факты:

- Каждый тест обёрнут в `if test_path.exists() { ... }` — при отсутствии файла тест проходит как пустой (не `#[ignore]`, а именно условное тело), поэтому `cargo test` **не падает**.
- Сабмодуль `backend/whisper.cpp` в данной копии **не инициализирован**: `git submodule status` → `-d682e150… backend/whisper.cpp` (префикс `-` = не инициализирован; каталог `backend/whisper.cpp` пуст — `ls` пустой). Значит, фикстуры **уже сейчас отсутствуют**, и эти тесты уже сейчас выполняются как no-op. Удаление `backend/` ничего здесь не меняет.

### 2.3. Похожие, но НЕ связанные с репозиторной папкой `backend/` места

- `frontend/src-tauri/src/database/manager.rs:59–61` — «Legacy backend DB path» = `app_data_dir.join("meeting_minutes.db")`. Это **пользовательский каталог данных приложения** (миграция БД старых установок), а не папка репозитория. На удаление `backend/` не влияет.
- `frontend/src-tauri/src/database/commands.rs:72–76` — детект legacy-БД в **выбранном пользователем** каталоге (`path.join("backend").join("meeting_minutes.db")`) — импорт старых установок через UI (`frontend/src/components/DatabaseImport/LegacyDatabaseImport.tsx:45,138`). Работает с пользовательскими путями, не с репозиторием.

## 3. CI и сборка

- `.github/workflows/` (`build.yml`, `build-linux.yml`, `build-macos.yml`, `build-windows.yml`, `build-test.yml`, `build-devtest.yml`, `pr-main-check.yml`, `release.yml`): `grep -rn "backend" .github/workflows/*.yml` не нашёл ни одного упоминания папки `backend/` (единственное совпадение — слово «backend» в `.github/workflows/ACCELERATION_GUIDE.md:142` про compute-backend OpenBLAS). CI не собирает и не использует `backend/`.
- `frontend/package.json` и `frontend/src-tauri/tauri.conf.json`: упоминаний `backend` нет (`grep` пуст). Скрипты сборки (`pnpm run tauri:dev` / `tauri:build`, см. `AGENTS.md` → Development Commands) не обращаются к `backend/`.
- Корневые/служебные скрипты `scripts/`: единственная ссылка на папку — allowlist в `scripts/check-brand.py:19–20` (`EXCLUDED_DIRS` содержит `"backend", # legacy archive, intentionally untouched`). Никакой build/test-скрипт в `scripts/` на `backend/` не ссылается (`grep` по `scripts/*.sh`, `scripts/*.py`).
- `llama-helper/` (sidecar для сборки): упоминаний `backend` нет.

## 4. Git-сабмодуль `backend/whisper.cpp`

- `.gitmodules` (весь файл, 4 строки): сабмодуль `backend/whisper.cpp`, url `https://github.com/Zackriya-Solutions/whisper.cpp`, branch `develop`.
- В текущей копии сабмодуль **не инициализирован** (`git submodule status` → префикс `-`), каталог пуст, в `.git/modules` записей нет (`ls .git/modules` пуст), в `.git/config` секции `submodule` нет. Поэтому orphan-`gitdir` в `.git/modules` после удаления не останется.
- Правильное удаление: `git rm backend/whisper.cpp` (снимет запись из индекса), затем удалить секцию из `.gitmodules` (или весь файл — других сабмодулей нет), затем `git rm -r backend/` / коммит. При простом `rm -rf backend/` без `git rm` git будет показывать `deleted: backend/...` для всех отслеживаемых файлов и `deleted: backend/whisper.cpp` для сабмодуля — само по себе репозиторий не сломается, но индекс останется «грязным» до коммита.
- Размер `backend/` в рабочей копии — ~920K (без инициализированного сабмодуля); в git-истории объекты останутся в любом случае.

## 5. Документация и спеки

- Упоминания `backend/` в документации (`grep -rn "backend/" docs/ README.md CONTRIBUTING.md AGENTS.md`): только `AGENTS.md:11`, `CONTRIBUTING.md:3`, `docs/architecture.md:45` — все три называют `backend/` неподдерживаемым архивом (см. раздел 1). **Инструкций, которые требуют существования `backend/`, нет**: `docs/BUILDING.md` и `docs/GPU_ACCELERATION.md` ссылаются только на `frontend/`, `llama-helper/`, `.tools/`; слово «backend» там означает compute/audio-бэкенды whisper-rs/audio.cpp.
- `.scratch/`-спеки прямо предписывают не трогать `backend/` на этапе ребрендинга/миграций: `.scratch/rebrand-samobranka/spec.md:52,58,69`, `.scratch/audio-cpp-provider/spec.md:73`, `.scratch/upstream-0.5.0-sync/issues/02-brand-lint-path-allowlist.md:155`. Это артефакты планирования, а не runtime-зависимости; при удалении стоит актуализировать (или оставить как исторический факт — спеки описывают прошедшую работу).

---

## Что произойдёт при удалении `backend/`

### Не сломается ничего (проверено по коду)

1. **Сборка приложения** (`pnpm run tauri:dev` / `tauri:build`): никаких ссылок на `backend/` в `package.json`, `tauri.conf.json`, build-скриптах, `Cargo.toml`/`build.rs` нет.
2. **CI** (все workflow в `.github/workflows/`): не обращаются к `backend/`.
3. **Runtime**: единственные runtime-ссылки — dev-only fallback в `whisper_engine.rs:127–130` под `cfg!(debug_assertions)` с `.exists()`-guard; они и сейчас не срабатывают (каталога `backend/whisper-server-package/` нет). В проде движок всегда получает `models_dir` из app-data-dir (`whisper_engine/commands.rs:25–51`).
4. **Тесты `cargo test`**: три теста в `audio/import.rs` (строки 1136/1153/1165) обёрнуты в `if test_path.exists()`; фикстур `jfk.wav`/`jfk.mp3` в репозитории сейчас нет (сабмодуль не инициализирован), т.е. они уже выполняются как no-op и после удаления будут вести себя так же. Падений не будет.
5. **Миграция legacy-БД**: пути в `database/manager.rs:59–61` и `database/commands.rs:72–76` относятся к пользовательским каталогам данных, не к репозиторию.

### Что нужно поправить заранее / вместе с удалением

1. **Git-гигиена сабмодуля**: удалить через `git rm backend/whisper.cpp` + убрать секцию из `.gitmodules` (это единственный сабмодуль — файл можно удалить целиком), затем `git rm -r backend/`. Простой `rm -rf backend/` оставит в `git status` массовые `deleted:` и запись сабмодуля; сиротского `gitdir` в `.git/modules` в данной копии не будет (сабмодуль не инициализирован), но в клонах, где он инициализирован, стоит проверить `git clean` сабмодульных остатков.
2. **`frontend/src-tauri/src/whisper_engine/whisper_engine.rs:119–130`**: удалить/заменить мёртвые dev-fallback-ветки `backend/whisper-server-package/models`, чтобы код не ссылался на несуществующие пути (косметика, не обязательна для работоспособности).
3. **`frontend/src-tauri/src/audio/import.rs:1130–1170`**: перенести тестовые фикстуры (например, сгенерировать маленький WAV в `testdata/`) или оставить тесты условными, зафиксировав, что они осознанно no-op; сейчас они зависят от стороннего сабмодуля, который никто не инициализирует.
4. **`scripts/check-brand.py:19–20`**: убрать `"backend"` из `EXCLUDED_DIRS`, иначе brand-lint перестанет иметь смысл для каталога, которого нет (и, наоборот, если его оставить — исключение станет недостижимым, что тоже безвредно, но устареет как комментарий).
5. **Документация**: обновить формулировки в `AGENTS.md:11`, `CONTRIBUTING.md:3`, `docs/architecture.md:45` (убрать «archive»-статус, заменить на «удалён как legacy»), при желании — упоминания в `.scratch/*`-спеках (как исторический контекст можно оставить).
6. **Не трогать**: `database/manager.rs` и `database/commands.rs` — их «backend» пути относятся к пользовательским данным старых установок и должны остаться для миграции БД.
