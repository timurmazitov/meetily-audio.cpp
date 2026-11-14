# 07: Windows-упаковка — нативные библиотеки и MSVC-рантайм в установщике

**What to build:** Установщик Windows содержит всё необходимое для работы провайдера `audioCpp`: нативную библиотеку audio.cpp и бандл MSVC-рантайма (msvcp140, vcruntime140, vcomp140). Приложение запускается и транскрибирует через audioCpp на чистой Windows-машине без установленного Visual C++ Redistributable. Сборка нативных компонентов интегрирована в существующий CI-пайплайн Windows; staging выполняется по образцу существующего sidecar-паттерна. Лицензия Apache-2.0 движка включается в дистрибутив.

**Blocked by:** 04 (Транскрибация встречи провайдером audioCpp)

**Status:** resolved

- [ ] Установщик Windows собирается в CI с нативной библиотекой audio.cpp и MSVC-рантаймом
- [ ] На чистой Windows-машине (без VC++ Redist) приложение устанавливается, запускается и транскрибирует через audioCpp
- [ ] Лицензия Apache-2.0 audio.cpp присутствует в дистрибутиве
- [ ] Linux-сборка продолжает работать без регрессий

## Answer

Что построено (ветка `audio-cpp-07-windows-packaging`):

- **`frontend/src-tauri/scripts/stage-audiocpp-runtime.ps1`** — скрипт стейджинга по образцу sidecar-паттерна: скачивает пинованный lib-архив релиза audio.cpp `audio-v0.9.4-lib-windows-x64-vulkan.zip` (соответствует `--features vulkan` сборки Windows; SHA256 `cf2fabd8630050491a92c002d7bdfd29eca722c31fb3b8cb0ba9b9c5e6e9ff3d` захардкожен в workflow и проверяется перед распаковкой), извлекает в `frontend/src-tauri/audiocpp-runtime/`: `audiocpp.dll`, бандл MSVC-рантайма (`msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll`) и лицензию `audio.cpp-LICENSE.txt` (Apache-2.0 из архива). Каталог добавлен в `.gitignore` (генерируется CI, по аналогии с `frontend/src-tauri/binaries/`).
- **`frontend/src-tauri/tauri.windows.conf.json`** — маппинг `"audiocpp-runtime": "."` в `bundle.resources` (тот же механизм, что у `binaries/onnxruntime`): все файлы попадают в корень установки рядом с исполняемым файлом. Обнаружение библиотеки (`audio_cpp_engine/wrapper.rs`, тикет 02) уже ищет `audiocpp.dll` рядом с `current_exe()` — лейаут бандлера совместим без изменений рантайм-кода; рядом лежащие MSVC DLL подхватывает загрузчик Windows, поэтому приложение работает без установленного Visual C++ Redistributable.
- **CI**: шаги `Stage audio.cpp runtime for bundling` + `Verify audio.cpp runtime staging` добавлены в `.github/workflows/build-windows.yml` и в Windows-ногу `.github/workflows/build.yml` (release-пайплайн, откуда их подхватывает `release.yml`); verify-шаг падает, если не хватает `audiocpp.dll`/`msvcp140.dll`/`vcruntime140.dll`/`vcomp140.dll`/лицензии.
- **`wrapper.rs`** — новый тест `library_next_to_executable_is_discovered` (библиотека рядом с исполняемым файлом находится без env-override; env-мутирующие тесты сериализованы общим мьютексом).
- **`docs/BUILDING.md`** — документирован стейджинг рантайма audio.cpp в дистрибутивных Windows-сборках и dev-обход через `AUDIOCPP_LIBRARY_PATH`.

Как проверено (Linux-хост; валидация Windows-сборки недоступна):

- YAML-синтаксис `build-windows.yml` и `build.yml` — парсится (`yaml.safe_load`); JSON `tauri.windows.conf.json` валиден.
- Скрипт стейджинга прогнан по-настоящему на PowerShell 7.4.6 (в `/tmp/opencode/pwsh`, не инсталлировался в систему): скачивание → SHA256-верификация → распаковка → стейджинг всех 11 файлов, лицензия — Apache-2.0 «Copyright 2026 ShugoAI LLC». Негативный путь (неверный хеш) завершается exit 1. Заодно найден и исправлен баг с глубиной `Resolve-Path` (`$PSScriptRoot/../../..`).
- 17 юнит-тестов модуля (включая новый тест обнаружения рядом с exe) зелёные через standalone-харнесс; `rustfmt --check` чист. Смоук `AudioCpp::load_from` против живой `libaudiocpp.so` (ubuntu-vulkan v0.9.4): ABI 0.2.0, build 0.9.4 — регрессий загрузки нет.
- Linux-сборка не затронута (изменения только Windows-workflow'ов, windows-конфига Tauri, gitignore, скрипта и теста).

Не проверено (требуется реальная Windows-машина / прогон CI на `windows-latest`):

- [ ] Фактическая сборка установщика MSI/NSIS с застейдженным рантаймом и установка/запуск на чистой Windows без VC++ Redist: неизменённость маппинга `bundle.resources` "." для NSIS/MSI, фактическое разрешение зависимостей `audiocpp.dll` (включая `vulkan-1.dll` от Vulkan-сборки, который бандлится отдельным существующим механизмом) и транскрибация через audioCpp на чистой машине.
- [ ] Подпись DLL/установщика DigiCert-пайплайном поверх новых файлов и прогон `signCommand`.
- [ ] Поведение `Invoke-WebRequest`/каширования в `windows-latest` раннере (на Linux всё отработало, на Windows проверялся только парсер скрипта — `Parser::ParseFile` OK).
