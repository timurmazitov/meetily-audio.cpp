# 02: FFI-subset, ABI-гейт по contract.json и команда диагностики

**What to build:** Приложение может загрузить нативную библиотеку transcribe.cpp и рассказать о ней пользователю: отдельная команда диагностики возвращает доступна ли библиотека, её путь, версию, header_hash и список доступных бэкендов (по образцу диагностики audio.cpp; никогда не падает — вместо ошибки отдаёт unavailable с текстом проблемы). До первого вызова библиотека проверяется по contract.json рядом с собой: запиненные в коде версия 0.4.2 и header_hash должны совпасть, иначе — понятная ошибка «несовместимый ABI». Поверхности FFI — минимальный subset C API: инициализация модулей бэкендов из каталога рядом с библиотекой, перечисление устройств, загрузка модели из GGUF, создание сессии, запуск run по 16 kHz mono f32, чтение thread-local ошибки.

**Blocked by:** 01 (Стейджинг нативной библиотеки transcribe.cpp)

**Status:** resolved

- [x] Загрузка библиотеки через динамическую загрузку с разрешением пути: env-переменная → каталог lane рядом с exe → плоско рядом с exe
- [x] ABI-гейт: несовпадение version/header_hash (или отсутствие contract.json) даёт типизированную ошибку несовместимого ABI до любых нативных вызовов; юнит-тесты на parsing contract'а с подложными файлами
- [x] Отсутствие всех нужных символов даёт типизированную ошибку с именем отсутствующего символа
- [x] Safe-обёртки над нативными хендлами с детерминированным освобождением; хендлы не объявлены разделяемыми между потоками (тест-запрет Sync, по образцу audio.cpp)
- [x] Команда диагностики зарегистрирована в invoke_handler и возвращает структуру: available, library_path, version, header_hash, backends, error
- [x] Без установленной библиотеки команда возвращает available=false с пользовательским текстом ошибки; UI-зависимые команды это не затрагивает
- [x] Модуль изолирует всю небезопасную работу с библиотекой (единственное место с unsafe FFI для движка)

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-02-ffi`, мерж `9ae752b`): файлы `error.rs` (pins `TRANSCRIBECPP_PINNED_VERSION`/`TRANSCRIBECPP_PINNED_HEADER_HASH` — единый источник), `ffi.rs` (23 символа, sym!-макро → `SymbolMissing{symbol}`), `wrapper.rs` (disclosure env→`transcribe-libs/<lane>/`→flat; `AbiContract` из contract.json, гейт до вызовов), `backend.rs` (`ComputeMode` Auto/Vulkan/Cpu, `select_device`, `fell_back_to_cpu` для билета 06), `engine.rs` (seam-трейты `TranscribeCppEngine`/`TranscribeCppModel`), `session_thread.rs` (паттерн audio.cpp), `diagnostics.rs` (`transcribecpp_diagnostics`, никогда Err). `cargo test --lib transcribe_cpp` — 88 green; live-smoke против реальной staged-библиотеки: гейт OK, 0.4.2/`f99ad4169374b8f3`, Vulkan+Cpu модули, CPU-устройства ≥1. Важно для 05: все клоны `SessionHandle` должны быть dropped до `shutdown()` (иначе join висит) — задокументировано в коде.
- Замеченный при разработке deadlock в тесте сессионного треда исправлен (commit `1e48740`): shutdown вызывался при живом клоне handle.
