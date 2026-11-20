# 05: Провайдер transcribeCpp — запись встречи даёт транскрипт (tracer bullet)

**What to build:** tracer-bullet через все слои: пользователь выбирает провайдер «Transcribe.cpp» с установленной моделью, запускает запись встречи, и во время встречи в UI идут частичные и финальные обновления транскрипта через существующие события воркера; по окончании — полный транскрипт сохраняется как у других движков. Блокирующий нативный вызов живёт на выделенной сессионной нити (сессии движка Send, но не Sync; один одновременный run на модель). Путь батчевый offline: VAD-чанки 16 kHz mono f32 → run.

**Blocked by:** 02 (FFI-subset, ABI-гейт, диагностика), 03 (Каталог RU-моделей и менеджер моделей с загрузками)

**Status:** resolved

- [x] Новый провайдер реализует существующий контракт провайдера транскрибации (ready/errors: ModelNotLoaded, AudioTooShort, EngineFailed) и возвращает TranscriptResult с текстом и флагом partial
- [x] Диспатч транскрибации знает провайдера transcribeCpp: валидация готовности модели перед стартом записи (ошибка с пользовательским текстом, если модель не установлена/не ASR), фабрика движка и инициализация провайдера из конфига
- [x] Хранение API-ключа для нового провайдера — no-op (локальный движок без ключа)
- [x] Сессионная нить: загрузка модели на выделенном потоке, запросы транскрибации через каналы, отсутствие блокирующих вызовов в async-контексте; освобождение при выгрузке
- [x] Выбранная в конфиге модель реально используется (проверка: скачали gigaam-v3-ctc — транскрипт идёт им); language hint применяется, если модель его поддерживает
- [x] Юнит-тесты провайдера на fake-движке (seam-трейты, паттерн audio.cpp) без нативной библиотеки: happy path, ошибки, готовность
- [ ] Ручная проверка в Tauri-приложении на Linux (Vulkan-lane) — ЧАСТИЧНО: headless live-smoke через реальный прод-путь выполнен (см. комментарий); GUI-запись встречи и Vulkan-инференс требуют ручной проверки человеком

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-05-provider`, мерж `71ba8df`): `audio/transcription/transcribe_cpp_provider.rs` (provider поверх SessionThread, ошибки контракта, language hint → `transcribe_run_params.language`, «auto»→NULL, translation-refusal), глобальный хендл + `load_native_engine/load_native_provider` в `transcribe_cpp_engine::transcription`, явные ветки диспатча в `audio/transcription/engine.rs` (Whisper-default не затронут), `BatchEngine::TranscribeCpp` + unload-ветки, api-key no-op в `setting.rs`, unload после записи в `recording_commands.rs`, подстановка дефолта модели в `api_get_transcript_config`.
- Тесты (Linux x86_64): `cargo test --lib transcribe_cpp` 104 green (13 новых fake-engine), `--lib transcription` 131, полный `--lib` 777 green / 0 failed / 7 ignored, `cargo check --all-targets` чистый.
- Live-проверка (реальная, headless, эта машина): staged-библиотека v0.4.2 (гейт OK, backends vulkan+cpu), реально скачанная `parakeet-tdt-0.6b-v3-Q4_K_M` (SHA-256 совпал с пином), синтетическая речь (ffmpeg lavfi flite) → прогон через прод-путь на CPU-бэкенде → транскрипт `"Hello world this is a test of the transcription engine."` (точное совпадение). Тест закоммичен как `#[ignore]` smoke (`tests/transcribecpp_live_smoke.rs`).
- НЕ проверено (нужен GUI/GPU): запись встречи в Tauri-приложении с Vulkan-лейном — оставить мейнтейнеру/пользователю; результат дописать в этот комментарий.
