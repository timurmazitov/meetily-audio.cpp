# 01: Стейджинг нативной библиотеки transcribe.cpp

**What to build:** Разработчик одной командой пакета из `frontend/` готовит нативную библиотеку transcribe.cpp v0.4.2 к запуску `tauri:dev`, а CI-сборки делают то же самое автоматически. Скрипт скачивает релизный архив нужного lane с GitHub Releases (linux-x86_64 / linux-aarch64 / windows-x86_64 — cpu-vulkan, macos-arm64 — metal), проверяет SHA-256 по запиненному значению и раскладывает содержимое (основная библиотека + модули бэкендов + contract.json + лицензии) в gitignored каталог рядом с src-tauri. Приложение в собранном виде находит библиотеку рядом с исполняемым файлом через ресурсы Tauri (для Windows — platform-conf, по образцу рантайма audio.cpp).

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Команда пакета в `frontend/` скачивает корректный lane по платформе/архитектуре и проверяет SHA-256; при расхождении — ошибка и удаление частичной загрузки
- [x] Повторный запуск скрипта не перекачивает корректно staged-артефакт (идемпотность)
- [x] Каталог стейджинга в .gitignore
- [x] После стейджинга и `pnpm run tauri:dev` на Linux библиотека лежит рядом с бинарником/в каталоге ресурсов и видима для runtime-дисквери (env-переменная → каталог lane рядом с exe → плоско рядом с exe) — проверено в билете 02 (runtime-дисквери + диагностика); bundling-конфиг Windows добавлен
- [x] Шаг стейджинга добавлен в CI-workflow сборки (Linux и Windows) до bundling
- [x] Windows-специфика: архив НЕ содержит MSVC redist DLL (delvewheel --exclude vulkan-1.dll, не вендорит vcruntime/msvcp) — задокументировано, что требуются от системы (VC++ redist + vulkan-1.dll)

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-01-staging`, мерж в `feature/transcribe-cpp-provider`):
  `frontend/scripts/stage-transcribe-native.mjs` + npm-скрипт `pnpm run stage:transcribe`.
  SHA-256 всех четырёх lanes пересчитаны реальным скачиванием — совпали с пинами (linux-x86_64 = `4fe87db8…15046`, сверка со спекой прошла).
  Стейдж плоский: `frontend/src-tauri/transcribecpp-runtime/` → `libtranscribe.so`, `libggml*.so` (vulkan + 14 ISA-cpu модулей), `contract.json`, `licenses/`, штамп `.staged`.
  `contract.json` релиза: `{"version":"0.4.2","header_hash":"f99ad4169374b8f3","backends":["vulkan","cpu"],"lane":"cpu-vulkan"}`.
  Идемпотентность, фолбэк битого штампа, ошибка несовпадения SHA без частичных файлов — проверено.
  CI: шаг в build.yml (гейт ubuntu) и build-windows.yml. Windows: бандлятся только DLL архива; системные зависимости задокументированы.
