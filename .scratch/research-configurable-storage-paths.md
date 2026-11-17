# Исследование: конфигурируемые пути хранения данных (app data dir + папка записей)

Type: research
Status: resolved

> Куда сохранён файл: репозиторий хранит исследовательские заметки двумя способами —
> однофайловые `research-*.md` прямо в `.scratch/` (прецедент: `.scratch/research-tauri-autoupdate.md`)
> и фичи-спеки в `.scratch/<feature-slug>/spec.md` (см. `docs/agents/issue-tracker.md`).
> Так как это исследование, а не спека фичи, использован первый вариант: `.scratch/research-configurable-storage-paths.md`.

Дата: 2026-11-17. Все ссылки на код проверены чтением файлов.

---

## 1. Текущее состояние: где и как хранятся данные

Идентификатор приложения — `ai.samobranka.app`, `productName: "Samobranka"`
(`frontend/src-tauri/tauri.conf.json:3-4`). Поэтому на Windows
`app.path().app_data_dir()` = `%APPDATA%\ai.samobranka.app\` (Roaming-профиль пользователя) —
именно тот путь, который пользователь хочет сделать настраиваемым. Права доступа (capabilities)
встроены прямо в `tauri.conf.json` (отдельной папки `capabilities/` нет), scope fs-плагина —
только `$APPDATA/*` (`tauri.conf.json:47-89`).

Все пути резолвятся в рантайме через `app.path().app_data_dir()` (Tauri path API), а не
хардкодятся — это соответствует гайдлайну AGENTS.md («Use Tauri path APIs for application
files; do not hardcode platform-specific paths»).

### 1.1. База данных (SQLite)

- `frontend/src-tauri/src/database/manager.rs:44-67`: `DatabaseManager::new_from_app_handle`
  берёт `app_handle.path().app_data_dir()`, создаёт его при отсутствии и использует
  `<app_data_dir>/meeting_minutes.sqlite`; рядом ищется легаси-БД `meeting_minutes.db`
  (авто-миграция копированием, `manager.rs:19-31`), а также защитно чистятся WAL/SHM-файлы
  (`manager.rs:65-67, 86-97`).
- Миграции: `sqlx::migrate!("./migrations")` (`manager.rs:35`), файлы —
  `frontend/src-tauri/migrations/`.
- Инициализация при старте: `frontend/src-tauri/src/lib.rs:711-715` вызывает
  `database::setup::initialize_database_on_startup` (`frontend/src-tauri/src/database/setup.rs:8-26`);
  при первом запуске событие `first-launch-detected` откладывает инициализацию, иначе БД
  открывается сразу (`app.manage(AppState { db_manager })`).
- Команды: `get_database_directory` (`database/commands.rs:227-234`),
  `open_database_folder` (`database/commands.rs:238-278`), импорт легаси-БД
  (`database/commands.rs:145-173`).

### 1.2. Модели (Whisper, Parakeet, audio.cpp, summary)

Все четыре движка используют общий корень `<app_data_dir>/models`:

- Whisper: `whisper_engine/commands.rs:13-37` — `set_models_directory()` пишет в глобальный
  `static MODELS_DIR: Mutex<Option<PathBuf>>`; fallback без инициализации —
  `dirs::data_dir()/Samobranka/models` (`whisper_engine/whisper_engine.rs:112-140`).
- Parakeet: `parakeet_engine/commands.rs:13-37` — зеркально; fallback —
  `parakeet_engine/parakeet_engine.rs:291`.
- audio.cpp: `audio_cpp_engine/commands.rs:95-127` (корень `<app_data_dir>/models`, подпапка
  `audiocpp` — `audio_cpp_engine/models.rs:82-114`).
- Summary (BuiltInAI/llama.cpp): `summary/summary_engine/commands.rs:53` и
  `commands.rs:359-365` — `<app_data_dir>/models/summary`; fallback —
  `summary/summary_engine/model_manager.rs:145-151`.
- Утилита `open_models_folder` — `whisper_engine/commands.rs:530-558`.

Все `set_models_directory` вызываются в `setup` до инициализации движков
(`lib.rs:659-687`). Это означает, что путь к моделям фиксируется один раз на старте
в глобальных static-переменных — смена «на лету» без перезапуска/переустановки глобалов
невозможна без доработки.

### 1.3. Записи встреч (recordings)

- Базовая папка: `audio/recording_preferences.rs:233-267` —
  `get_default_recordings_folder()`: Windows — `%USERPROFILE%\Music\samobranka-recordings`
  (fallback — Documents), macOS — `~/Movies/samobranka-recordings`, Linux —
  `~/Documents/samobranka-recordings`.
- Структура встречи: `audio/audio_processing.rs:35-58` — `create_meeting_folder()` создаёт
  `<base>/<MeetingName>_<YYYY-MM-DD_HH-MM>/` с подпапкой `.checkpoints` (при auto-save).
  Итоговые файлы: `audio.mp4`, `transcripts.json`, `metadata.json`
  (`recording_saver.rs:252-253, 269-291`).
- **Важно**: активный сейвер `audio/recording_saver.rs:226` вызывает
  `get_default_recordings_folder()` напрямую, **игнорируя** сохранённый в преференсах
  `save_folder`. Сохранённый `save_folder` сегодня нигде в активном коде не используется
  (поиск `save_folder` по `src-tauri/src` даёт только `recording_preferences.rs` и мёртвый
  `recording_saver_old.rs:327,350,363`). Тот же дефолт использует импорт аудио:
  `audio/import.rs:338`. Это готовность «наполовину»: хранилище и UI для кастомной папки
  записей уже есть, а исполнительный путь его не читает.
- Путь встречи в БД: миграция `migrations/20251006000000_add_audio_sync_fields.sql:8` добавляет
  `meetings.folder_path TEXT`. Фронтенд кладёт туда абсолютный путь из события
  `recording-stopped` (`frontend/src/hooks/useRecordingStop.ts:122-131, 278-294`) через
  `storageService.ts:14,42-47`. Воспроизведение идёт по абсолютному пути:
  команда `read_audio_file` — `lib.rs:399-404` (`std::fs::read` произвольного пути);
  `get_meeting_folder_path` — `audio/recording_commands.rs:1446-1455`;
  `useTranscriptRecovery.ts:126-203` (в т.ч. `cleanup_checkpoints`).
- tray-остановка записи передаёт `save_path = <app_data_dir>/recording-<ts>.wav`
  (`tray.rs:67-77, 165-175`; `RecordingArgs.save_path` — `recording_commands.rs:152`) —
  похоже на легаси-артефакт, активный сейвер папку строит сам.

### 1.4. Прочее в app_data_dir и рядом

- Логи: `lib.rs:74-82` — `<app_data_dir>/logs` (release) / `logs-dev` (debug);
  подключение в setup — `lib.rs:586-594`.
- Doodle: `doodle.rs:151-153` — `<app_data_dir>/doodle` (asset protocol scope
  `$APPDATA/doodle/*` — `tauri.conf.json:30-33`).
- Tauri store: `recording_preferences.json` читается/пишется через `tauri_plugin_store`
  (`recording_preferences.rs:286-321, 344-409`); фронтенд дополнительно использует
  `Store.load('preferences.json')` (`RecordingSettings.tsx:67-69, 115-117`).
  По умолчанию файлы плагина store лежат в `app_data_dir` (поведение
  tauri-plugin-store v2; внешний факт, в коде репозитория путь не переопределяется).
- **Вне** app_data_dir (и легко забываемые при переезде):
  - Настройки уведомлений: `notifications/settings.rs:115-127` —
    `dirs::config_dir()/samobranka/notifications.json`.
  - Пользовательские шаблоны саммари: `summary/templates/loader.rs:26-32` —
    `dirs::data_dir()/Samobranka/templates`.
- llama-helper: бинарник бандлится как `externalBin` (`tauri.conf.json` →
  `binaries/llama-helper`, стейджинг — `docs/BUILDING.md:33-53`), ищется рядом с exe или
  через `SAMOBRANKA_LLAMA_HELPER` (`summary/summary_engine/sidecar.rs:90-130`).
  `SidecarManager::new(_app_data_dir)` **игнорирует** app_data_dir (`sidecar.rs:78`) —
  путь к модели передаётся аргументом, так что sidecar не привязан к месту хранения данных.

### 1.5. Документация

- `docs/architecture.md:78` прямо предупреждает: директории моделей зависят от конфигурации,
  «use the current initialization and loading code as the source of truth rather than
  hardcoding storage paths».
- В `docs/BUILDING.md` утверждений о расположении пользовательских данных нет (только
  стейджинг sidecar/LLVM, см. `BUILDING.md:33-61`).

---

## 2. Существующая архитектура настроек (end-to-end)

Существует **два** параллельных механизма:

**A. SQLite-таблица `settings` / `transcript_settings`** — модельные/провайдерные настройки.
Пример потока: `frontend/src/app/settings/page.tsx:35-52` → `invoke('api_get_transcript_config')`
→ репозиторий `database/repositories/setting.rs:33-69` (`get_model_config` /
`save_model_config`, upsert с фиксированным `id='1'`). Схема —
`migrations/20250916100000_initial_schema.sql:51-75`.

**B. tauri-plugin-store (JSON-файлы)** — «преференсы»:
- Rust: `RecordingPreferences` (`recording_preferences.rs:14-50`) с командами
  `get_recording_preferences` / `set_recording_preferences` (`recording_preferences.rs:412-429`),
  регистрация плагина — `lib.rs:569`, новые команды добавляются в
  `invoke_handler` (`lib.rs:740+`).
- Фронтенд: `RecordingSettings.tsx:40-61` грузит через `invoke`, меняет локальный стейт,
  сохраняет `invoke('set_recording_preferences', { preferences })` (`:125-148`).
- Фронтенд-стор напрямую: `RecordingSettings.tsx:67-69` (`Store.load('preferences.json')`).
- Дисплейные пути: `ConfigContext.tsx:413-459` (`loadPreferences`) — три invoke-параллели:
  `get_database_directory`, `whisper_get_models_directory`, `get_default_recordings_folder_path`
  → `storageLocations` → `PreferenceSettings.tsx`.

Существующая страница настроек: `frontend/src/app/settings/page.tsx:17-23` — вкладки
General / Recordings / Transcription / Summary / Beta. Вкладка **General уже существует**
и рендерит `PreferenceSettings`.

---

## 3. Секция «Data Storage Locations» — уже существует

`frontend/src/components/PreferenceSettings.tsx:124-183`:

- Заголовок «Data Storage Locations» и подзаголовок «View and access where Samobranka stores
  your data» (`:126-129`) — секция **только для просмотра**, без выбора папок.
- Карточки Database (`:133-145`) и Whisper Models (`:148-160`) **закомментированы**.
- Живая карточка «Meeting Recordings» (`:163-175`) показывает `storageLocations.recordings`
  и кнопку «Open Folder» (`invoke('open_recordings_folder')`).
- Плашка-примечание: «Database and models are stored together in your application data
  directory for unified management» (`:178-182`).

Дефект отображения: `storageLocations.recordings` заполняется из
`get_default_recordings_folder_path` (`ConfigContext.tsx:442`), который возвращает
**дефолт**, а не фактический `save_folder` из преференсов
(`recording_preferences.rs:432-435`). То есть показанный путь может не совпадать с реальным
(тем более что сейвер вообще игнорирует `save_folder`, см. §1.3).

Выбор папки пользователем в UI отсутствует: команда-заглушка
`select_recording_folder` возвращает `Ok(None)` с предупреждением
(`recording_preferences.rs:477-486`); реальный диалог выбора папки не реализован
(в Rust диалог используется только для выбора **файла** легаси-БД —
`database/commands.rs:25-44` через `tauri_plugin_dialog::DialogExt`; плагин зарегистрирован
в `lib.rs:571`; во фронтенде `@tauri-apps/plugin-dialog` не используется, permission
`dialog:*` в capabilities не объявлен — но для Rust-стороны capabilities не нужны).

---

## 4. Ограничения и риски

1. **Chicken-and-egg для app data dir.** Файлы настроек (`recording_preferences.json`,
   `preferences.json`), БД, логи, модели и doodle живут внутри `app_data_dir`. Если сам
   путь к app data dir хранить внутри app data dir — переезд невозможен (при первом запуске
   на новом месте файла нет; при чтении настроек ещё неизвестно, откуда читать). Нужен
   двухфазный bootstrap: «bootstrap-файл» в фиксированном месте (дефолтный app_data_dir
   или `dirs::config_dir()`), который указывает на реальное место.
2. **БД открыта на всю сессию.** `AppState { db_manager }` управляется в setup
   (`database/setup.rs:22-24`, `database/commands.rs:164, 188`), pool закрывается только
   в `cleanup()` при штатном выключении (`manager.rs:189-207`). Перенос БД возможен только
   при закрытом pool (через `cleanup()` + повторный `new_from_app_handle`), иначе WAL
   и открытые соединения дадут рассинхрон.
3. **Абсолютные пути в БД.** `meetings.folder_path` — абсолютный путь
   (`useRecordingStop.ts:127-131`; миграция `20251006000000_...sql:8`). Смена папки записей
   «только для новых» ломает старые записи, только если старую папку удалят; сама БД
   при этом остаётся валидной, т.к. пути в ней абсолютные и не зависят от настройки.
   Миграции путей в БД (rewrite `folder_path`) нужны только при физическом переносе файлов.
4. **Модели уже скачаны.** Переезд app data dir должен либо копировать гигабайты моделей,
   либо оставлять их на месте (разнести «app data» и «models root» — но сейчас они
   жёстко связаны, §1.2). Все четыре `set_models_directory` фиксируют путь в static на
   старте (`lib.rs:659-687`), смена требует перезапуска приложения.
5. **Файлы вне app_data_dir.** `notifications.json` (`notifications/settings.rs:115-127`) и
   пользовательские шаблоны (`summary/templates/loader.rs:26-32`) используют
   `dirs::config_dir()` / `dirs::data_dir()` напрямую — при переезде «всего» они останутся
   в старых местах (нужно либо принять, либо мигрировать отдельно).
6. **fs-scope.** В capabilities scope ограничен `$APPDATA/*` (`tauri.conf.json:80-88`).
   Сегодня фронтенд читает аудио через Rust-команду `read_audio_file` (`lib.rs:399-404`),
   поэтому записи вне `$APPDATA` работают. Если захотеть читать файлы через fs-плагин из JS
   (например, при новом пикере), scope придётся расширять динамически
   (`fs:scope` с пользовательскими путями) — либо продолжать идти через Rust-команды.
7. **Дефолт записей игнорируется кодом.** `save_folder` хранится и показывается, но
   `recording_saver.rs:226` и `import.rs:338` используют `get_default_recordings_folder()`.
   Перед добавлением новой настройки нужно сначала починить/подключить существующий
   механизм, иначе получится второй мёртвый путь.
8. **Tray-артефакт.** `tray.rs:77, 175` кладёт `recording-*.wav` прямо в app_data_dir —
   при переезде такие файлы останутся в старом месте (мусор/потеря).
9. **Первый запуск.** `check_first_launch` / `first-launch-detected`
   (`database/commands.rs:16-21`, `database/setup.rs:10-26`) завязаны на
   `app_data_dir` — логика первого запуска должна учитывать новый путь (и не считать
   «первым запуском» ситуацию после переезда с копированием БД).

## 5. Предлагаемый дизайн

### 5.1. Папка записей (низкий риск, делать первой)

- Источником истины остаётся существующий `RecordingPreferences.save_folder`
  (`recording_preferences.json` в app_data_dir — стор плагина уже настроен,
  `lib.rs:569`). Новый store не нужен.
- Починить исполнение: `recording_saver.rs:226` и `import.rs:338` должны читать
  `load_recording_preferences().save_folder` (с fallback на `get_default_recordings_folder()`
  при ошибке десериализации — уже заложено в `recording_preferences.rs:313-321`).
- UI: в существующей карточке «Meeting Recordings» (`PreferenceSettings.tsx:163-175`)
  добавить кнопку «Choose…», вызывающую новую Rust-команду
  `select_recording_folder` (реализовать через `app.dialog().directory().blocking_pick_folder()`,
  плагин уже зарегистрирован, `lib.rs:571`; заменить заглушку
  `recording_preferences.rs:477-486`), затем `set_recording_preferences`.
- Семантика — «применяется к новым записям»: старые папки лежат на старых абсолютных
  путях в БД и продолжают работать (воспроизведение по `folder_path`, §1.3). Опционально —
  отдельная кнопка «Move existing recordings…» с прогрессом (копирование +
  `UPDATE meetings SET folder_path`), но это отдельный тикет.
- Валидация: директория существует/создаётся (`ensure_recordings_directory`,
  `recording_preferences.rs:270-276` уже вызывается при сохранении, `:406`), тест на
  запись (записать и удалить временный файл), запрет выбирать корень диска/системные пути —
  минимально. Проверка свободного места — только как предупреждение, не блокер.
- Заодно починить `ConfigContext.tsx:442`: показывать фактический `save_folder`
  (взять из `get_recording_preferences`), а не дефолт.

### 5.2. Папка app data (высокий риск, отдельной фичей)

- **Bootstrap-файл.** Путь к app data dir хранить вне самого app data dir. Вариант:
  `<default_app_data_dir>/storage_location.json` (или `dirs::config_dir()/samobranka/`).
  Рекомендация — `dirs::config_dir()/samobranka/app-data-dir.json` (config dir на Windows —
  тот же Roaming, но отдельный от данных; иначе при «чистой» ситуации дефолтный app_data_dir
  не существует и bootstrap читать неоткуда). Альтернатива — переменная окружения
  `SAMOBRANKA_DATA_DIR` (как уже сделано для `SAMOBRANKA_LLAMA_HELPER`, `sidecar.rs:92-101`,
  и `SAMOBRANKA_DEBUG_LOG`, `lib.rs:582-585`) + bootstrap-файл.
- **Централизованный resolver.** Ввести `storage::app_data_dir(handle) -> PathBuf`,
  который: (1) читает bootstrap; (2) если нет — возвращает дефолт; (3) проксирует все
  текущие прямые вызовы `app.path().app_data_dir()` (перечень в §6). Это единственная
  точка, через которую проходит смена пути.
- **Применение — только через перезапуск.** При сохранении нового пути: валидировать
  (существует/создать, writable-тест), записать bootstrap-файл, показать диалог
  «Переместить существующие данные?» с вариантами:
  - *Перенести* (по умолчанию при пустом новом месте): корректно закрыть БД
    (`DatabaseManager::cleanup()`, `manager.rs:189-207`), скопировать БД (+WAL checkpoint),
    `models/`, `logs/`, `doodle/`, стор-файлы; затем пометить в bootstrap новый путь.
    Для больших моделей — прогресс-бар и оценка свободного места (сравнить размер
    `models/` с free space целевого диска).
  - *Начать заново*: записать bootstrap, не копировать.
  - *Отмена*.
  Перезапуск приложения после переноса (или хотя бы повторная инициализация БД/моделей);
  смена глобальных `MODELS_DIR` без перезапуска ненадёжна (static-переменные + уже
  загруженные движки).
- **Что переносится / что нет**: в scope — БД, модели, логи, стор-файлы, doodle.
  Вне scope (принять осознанно) — `notifications.json` и шаблоны
  (§4.5); либо отдельная мини-миграция. tray-артефакты `recording-*.wav` в корне старого
  app_data_dir — переносить опционально/игнорировать.
- **llama-helper**: не привязан к app_data_dir (`sidecar.rs:78`), модели передаются
  аргументом — дополнительных действий не требуется.
- **Защита от дурака**: запретить выбирать (а) текущий app data dir, (b) папку внутри
  себя самой рекурсивно, (c) не-writable пути, (d) сетевые/съёмные диски — предупреждением
  (WAL SQLite на сетевых ФС — риск).

---

## 6. Конкретный список изменений

### Папка записей (фаза 1)

| Файл | Изменение |
| --- | --- |
| `frontend/src-tauri/src/audio/recording_saver.rs:226` | Брать `save_folder` из `load_recording_preferences` вместо `get_default_recordings_folder()` (fallback на дефолт). |
| `frontend/src-tauri/src/audio/import.rs:338` | То же для импорта аудио. |
| `frontend/src-tauri/src/audio/recording_preferences.rs:477-486` | Реализовать `select_recording_folder` через `app.dialog().directory().blocking_pick_folder()`. |
| `frontend/src-tauri/src/audio/recording_preferences.rs` | Добавить команду-валидатор папки (exists/writable) при необходимости. |
| `frontend/src-tauri/src/lib.rs:740+` | Зарегистрировать новую команду в `invoke_handler`. |
| `frontend/src/components/PreferenceSettings.tsx:163-175` | Кнопка «Choose…» + показ выбранного пути в карточке «Meeting Recordings». |
| `frontend/src/contexts/ConfigContext.tsx:439-449` | `recordings` брать из `get_recording_preferences().save_folder` (исправить текущий дефолтный путь). |
| `frontend/src/components/RecordingSettings.tsx:183-198` | (Опционально) та же кнопка выбора в «Save Location». |
| Тесты: `frontend/tests/` (runner — см. конфигурацию в `frontend/tests`, не `pnpm test`) | e2e: смена папки → новая запись появляется в новой папке, старая читается. |

### Папка app data (фаза 2)

| Файл | Изменение |
| --- | --- |
| Новый `frontend/src-tauri/src/storage.rs` | `app_data_dir(handle)` resolver + чтение/запись bootstrap-файла (`config_dir/samobranka/app-data-dir.json` или env `SAMOBRANKA_DATA_DIR`). |
| `database/manager.rs:44-67, 121-130, 137-147` | `new_from_app_handle`, `is_first_launch`, `import_legacy_database` → через resolver. |
| `database/commands.rs:88-93, 228-250` | `check_default_legacy_database`, `get_database_directory`, `open_database_folder` → через resolver. |
| `database/setup.rs` | Учесть bootstrap при определении первого запуска. |
| `whisper_engine/commands.rs:16`, `parakeet_engine/commands.rs:16`, `audio_cpp_engine/commands.rs:106`, `summary/summary_engine/commands.rs:53,363`, `summary/service.rs:483`, `lib.rs:586, 659-687` (models/logs init) | Все `app.path().app_data_dir()` → resolver. |
| `doodle.rs:151-153`, `tray.rs:67,165` | Через resolver (tray — заодно решить судьбу легаси-`save_path`). |
| `lib.rs` | Новые команды: `get_app_data_directory`, `set_app_data_directory` (валидация + запись bootstrap + диалог переноса), `relocate_app_data` (копирование: БД через `cleanup()`+copy, `models/`, `logs/`, сторы). Регистрация в `invoke_handler`. |
| `frontend/src/components/PreferenceSettings.tsx` | Вернуть (раскомментировать/дописать) карточки Database и Models (`:133-160`) и добавить карточку «Application Data» с выбором папки, статусом переноса и предупреждением о перезапуске. |
| `frontend/src-tauri/tauri.conf.json` (capabilities) | При необходимости — динамическое расширение `fs:scope` (только если пойдём через fs-плагин из JS; рекомендуется обойтись Rust-командами). |
| `docs/architecture.md`, AGENTS.md при необходимости | Зафиксировать новый resolver как единую точку резолва путей. |

---

## 7. Открытые вопросы / риски

> **Update (2026-11-17): вопросы 1–8 закрыты решениями (см. ADR
> `docs/adr/0003-configurable-storage-locations.md`).** Резюме: никакого переноса
> файлов никогда (чистый лист; исключение — копирование мелких JSON-сторов настроек);
> bootstrap в `config_dir/samobranka/` + env `SAMOBRANKA_DATA_DIR`; модели не
> переносятся, корень моделей не разделяется; перезапуск обязателен; настройка
> `save_folder` — одна, на вкладке General, применяется к новым записям; сетевые
> диски для app data запрещены, для записей — предупреждение; логи не копируются
> (вопрос 8 снят самим решением «без переноса»). Ниже — исходная формулировка
> вопросов для истории.

1. **Bootstrap-локация**: ~~нужно решение~~ решено — `config_dir/samobranka/` + env.
2. **Перенос моделей**: ~~копировать гигабайты или предлагать «оставить models на месте»~~
   Решено: не переносить ничего (чистый лист); корень моделей не разделяется.
3. **WAL/живые соединения**: снят решением «без переноса» — БД не копируется, переноса
   pool не требуется; соответственно фаза 2 при смене папки просто стартует с пустой БД.
4. **Перезапуск**: ~~приемлемо ли принудительный рестарт~~ Решено: перезапуск обязателен.
5. **Обновления/автоапдейтер**: updater не зависит от app_data_dir, но `createUpdaterArtifacts`
   и кэш обновлений стоит проверить отдельно (не исследовано здесь).
6. **Двойник настройки**: ~~одна или две~~ Решено: одна настройка (`save_folder`),
   отображается только на вкладке General; контролы выбора папки с вкладки Recordings
   удалены.
7. **Съёмные/сетевые диски**: ~~запретить или предупреждать~~ Решено: app data dir —
   сетевые запрещены (блок), съёмные с предупреждением; папка записей — разрешены с
   предупреждением.
8. **Приватность логов**: снят решением «без переноса» — логи не копируются.
