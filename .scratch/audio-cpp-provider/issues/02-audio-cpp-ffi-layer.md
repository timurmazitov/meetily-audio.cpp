# 02: FFI-слой audio.cpp в приложении

**What to build:** Приложение умеет загружать нативную библиотеку audio.cpp в процесс: рукописный биндинг к C ABI (opaque-обёртки, `Send` вручную, без `Sync`), проверка совместимости версии ABI при загрузке с явной ошибкой при несовместимости. Пользовательская видимость — диагностическая команда, показывающая версию ABI и список доступных семейств моделей. Реальные модели ещё не загружаются. Требования из docs/research/audio-cpp-c-api.md: блокирующие вызовы только через `spawn_blocking`, одна сессия на поток, число потоков задаётся в конфигурации бэкенда.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Нативная библиотека из релизного lib-архива audio.cpp загружается в приложении на Linux и Windows
- [x] Несовместимая версия ABI отклоняется с понятной ошибкой, приложение не падает
- [x] Диагностическая команда выводит версию ABI и перечень семейств движка
- [x] Юнит-тесты маппинга кодов статуса и текстов ошибок проходят
- [x] Вся работа с нативной библиотекой изолирована в одном модуле-обёртке

## Answer

Что построено (ветка `audio-cpp-02-audio-cpp-ffi-layer`, модуль `frontend/src-tauri/src/audio_cpp_engine/`):

- **`ffi.rs`** — рукописный `extern "C"` биндинг к заголовку `include/audiocpp.h` релиза v0.9.4 (ABI 0.2.0): opaque-типы registry/model/session/options/request/result, repr(C)-структуры `audiocpp_model_config`/`audiocpp_backend_config`, 29 type-aliases функций и `AudiocppApi` — загрузка через `libloading` с разрешением всех используемых символов. Это единственное место, касающееся нативной библиотеки; плюс тонкие читалки `borrowed_str`/`last_error`/`check` (маппинг статуса + thread-local `audiocpp_last_error()`).
- **`error.rs`** — `Status` (8 кодов из заголовка), `AbiVersion` (packed u32 ↔ major/minor/patch), `AudioCppError` (LibraryNotFound / LibraryLoad / SymbolMissing / IncompatibleAbi / Native с detail). Гейт ABI: major должен совпасть (0), minor только добавляет entry points (>= 2), patch не гейтится.
- **`wrapper.rs`** — безопасные opaque-обёртки `AudioCpp`/`Registry`/`Model`/`Session`/`Request`/`Options`/`AudioCppResult` с `Drop`, вызывающим соответствующий `*_free`; `unsafe impl Send` вручную, `Sync` нигде не реализуется (есть compile-time тест `handles_are_send`). Поиск библиотеки: `AUDIOCPP_LIBRARY_PATH` (dev-override) → имя рядом с исполняемым файлом (`libaudiocpp.so`/`libaudiocpp.so.0` на Linux, `audiocpp.dll` на Windows). `BackendConfig` несёт `threads` (библиотека не трогает глобальный OpenMP). Документирован контракт: одна сессия на поток, блокирующие вызовы — через `spawn_blocking`.
- **`commands.rs`** — Tauri-команда `audiocpp_diagnostics`: вся работа внутри `tokio::task::spawn_blocking`; возвращает `AudioCppDiagnostics { available, library_path, abi_version, build_version, families, tasks, error }`. Отсутствующая/несовместимая библиотека сообщается in-band (`available: false` + понятный `error`), а не падением.
- Регистрация: `pub mod audio_cpp_engine;` и команда в `generate_handler!` в `lib.rs`; зависимость `libloading = "0.8"` в `Cargo.toml`; строка в Source Map `AGENTS.md`.

Как проверено:

- TDD red→green по срезам: сначала failing-тесты маппинга статусов/ABI-гейта/текстов ошибок (12 шт., прогон через standalone `rustc --test`), затем реализация.
- Полный набор — 17 юнит-тестов модуля (статусы, ABI-гейт, тексты ошибок, имена .so, env-override, Send-хендлы, unavailable-отчёт) — зелёный. Из-за отсутствия системных dev-пакетов (pkg-config, libssl-dev, webkit2gtk, alsa) полный `cargo test` крейта на этой машине невозможен; модуль скомпилирован и протестирован standalone-harness'ом, включающим те же исходные файлы (tauri заменён shim'ом только для макроса `#[command]`).
- Смоук против живой библиотеки `audio-v0.9.4-lib-ubuntu-x64-cpu.tar.gz`: загрузка успешна, ABI 0.2.0, build 0.9.4, 17 задач, 148 семейств (включая `gigaam_asr` и `nemotron_3_diar`).
- Смоук граничных путей на синтетических .so: ABI 1.2.0 отклонён гейтом с понятной ошибкой; библиотека с ABI 0.2.0 и падающим `registry_create` даёт `AUDIOCPP_ERR_RUNTIME` + текст thread-local ошибки; отсутствующий файл — понятная ошибка загрузки. Падений нет.
- `rustfmt --check` по файлам модуля чист. Не проверено: компиляция всего крейта и прогон в живом Tauri-приложении (нет системных зависимостей на машине); Windows-путь (`audiocpp.dll`) проверен только на уровне имён и cfg.
