# 04: Удаление движка Parakeet (Rust)

**What to build:** Движок Parakeet полностью вырезан из нативной части: модуль движка со всеми командами и событиями `parakeet_*`, его провайдер транскрибации, ветки в перечислении движка рантайма и батч-перечислении, инициализация при старте приложения, спец-обработка в пути конфигурации и проверках готовности, проверка наличия моделей в трее, зависимость ONNX Runtime и её тесты. Фронтенд к этому моменту (тикет 02) к Parakeet не обращается. После тикета сборка не содержит ONNX Runtime, поведение приложения не изменилось: вся транскрибация идёт через transcribe.cpp, диаризация — как раньше.

**Blocked by:** 02 (Экран настроек — только Transcribe.CPP)

**Status:** resolved

- [x] Модуль Parakeet, его команды, события и управляемое состояние удалены; регистрация команд не ссылается на них
- [x] Провайдер и ветки перечислений (движок рантайма, батч-движок), инициализация при старте и выгрузка при остановке — удалены
- [x] Зависимость ONNX Runtime и связанные с ней тесты удалены; осиротевшие транзитивные зависимости проверены
- [x] Tray-проверка доступных моделей учитывает только Transcribe.CPP
- [x] `cargo check` и тесты Rust зелёные; `pnpm run tauri:dev` запускается, запись с live-транскрибацией через transcribe.cpp работает

## Comments

- Ветка: `impl/tcp-only/04-remove-parakeet`, влита merge-коммитом `33baa41` в `impl/transcribe-cpp-only-transcription`.
- Удалены: `parakeet_engine/` (commands/mod/model/parakeet_engine), `parakeet_provider.rs`, `build/onnxruntime.rs`, ветки Parakeet в `engine.rs`/`worker.rs`/`import.rs`/`retranscription.rs`, ONNX-инициализация и команды в `lib.rs`, спец-обработка в `tray.rs`.
- `ort` остался только как транзитивная зависимость `silero_rs` (VAD исполняет модель через ONNX Runtime) — это осознанно, запись в Cargo.toml есть; прямых зависимостей и сборки ONNX нет.
- Проверено (2026-11-21, Linux): `cargo test --lib` — 729 passed / 0 failed / 7 ignored; smoke `transcribecpp_batch_smoke`/`transcribecpp_live_smoke` на реальном рантайме v0.4.2 и модели parakeet-tdt-0.6b-v3-Q4_K_M — транскрипт корректный, провайдер выгружается (`provider().is_none()`).
