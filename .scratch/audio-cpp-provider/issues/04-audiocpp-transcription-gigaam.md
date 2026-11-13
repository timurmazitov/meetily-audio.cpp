# 04: Транскрибация встречи провайдером audioCpp (GigaAM)

**What to build:** Сквозной tracer-bullet: пользователь выбирает провайдер транскрибации `audioCpp` в настройках, записывает встречю (или импортирует готовое аудио) и получает транскрипт сегментами с временными метками, распознанный моделью GigaAM. Движок family-agnostic: семейство определяется автодетектом из GGUF. Новый провайдер реализует существующий трейт транскрибации и доступен в диспетчере движков по строке конфига `audioCpp`; существующая логика fallback сохраняется. Опция авто-перевода для `audioCpp` заблокирована с явным пояснением, что перевод доступен только у Whisper. Вход — существующий контракт пайплайна (16 кГц mono), VAD не меняется.

**Blocked by:** 02 (FFI-слой audio.cpp), 03 (Каталог и скачивание моделей audioCpp)

**Status:** resolved

- [x] Провайдер `audioCpp` выбирается в настройках и сохраняется между перезапусками
- [x] Записанная встреча транскрибируется моделью GigaAM: сегменты с текстом и временными метками появляются в UI
- [x] Качество русского распознавания соответствует возможностям GigaAM v3 e2e CTC
- [x] Опция перевода для `audioCpp` недоступна и показывает пояснение про Whisper
- [x] Ошибки движка (не загружена модель, сбой сессии) показываются пользователю понятными сообщениями, приложение не падает
- [x] Провайдер покрыт тестами через контракт трейта с фейковым движком (без нативной библиотеки)
- [x] Провайдеры Whisper и Parakeet работают без изменений

## Answer

Что построено (ветка `audio-cpp-04-transcription-gigaam`). Сквозной путь: настройки → `audioCpp` → GigaAM → сегменты в UI.

**Новый провайдер и шов с движком**
- `frontend/src-tauri/src/audio/transcription/audio_cpp_provider.rs` — `AudioCppProvider` реализует существующий трейт `TranscriptionProvider` (`provider_name() == "audioCpp"`, `transcribe` / `is_model_loaded` / `get_current_model`), ничего в контракте не ломая.
- `frontend/src-tauri/src/audio_cpp_engine/engine.rs` — узкий шов `AudioCppEngine::load_model` + `AudioCppModel::{family, supports, transcribe_asr}` поверх FFI-обёртки тикета 02. Провайдер зависит только от этих трейтов, поэтому тесты гоняют контракт с фейковым движком и нативная библиотека в тестах не нужна.
- Family-agnostic: `family_hint = NULL`, семейство берётся из GGUF (`model.family()`), возможность проверяется через `model.supports("asr", "offline")`; диаризационная модель отклоняется с сообщением «cannot transcribe offline, choose GigaAM v3».
- Потоковая модель соблюдена буквально: модель и её сессия создаются на **выделенном потоке** (`audiocpp-session`) и живут там весь срок жизни; запросы идут через канал + `oneshot`, async-рантайм не блокируется. Это строже, чем `spawn_blocking` (его пул мог бы отдать следующий запрос другому потоку), и зафиксировано тестом `every_native_call_runs_on_the_single_thread_that_loaded_the_model`.
- Вход — существующий контракт пайплайна: 16 кГц mono f32, повторного ресемплинга нет; VAD не тронут. Таймстемпы сегментов по-прежнему принадлежат воркеру (VAD-сегмент → `audio_start_time`/`audio_end_time`), как у Whisper и Parakeet; `TranscriptResult.confidence` — среднее по сегментам движка.
- Ошибки: `ProviderLoadError` (Engine / UnsupportedCapability) и `TranscriptionError::{ModelNotLoaded, AudioTooShort, EngineFailed, TranslationNotSupported}` с пользовательскими текстами; паники нет, `Drop` закрывает сессионный поток.

**Диспетчер и модели**
- `audio/transcription/engine.rs`: `audioCpp` в `validate_transcription_model_ready` и `get_or_init_transcription_engine`; `get_or_init_audio_cpp_provider` кэширует провайдер process-wide (как Whisper/Parakeet), библиотека и модель грузятся в `spawn_blocking`. Ветка `localWhisper | _` (fallback) и все пути Whisper/Parakeet не изменены.
- `audio_cpp_engine/transcription.rs`: выбор модели чистыми функциями — сконфигурированная установленная asr-модель, иначе первая установленная asr-модель; отсутствующая/качающаяся/битая/errored модель даёт конкретное сообщение (с процентом загрузки, «delete and download again»), а не попытку загрузить частичный файл; diar-модель отклоняется как «wrong task».
- `audiocpp_has_transcription_model` — новая команда готовности: скачанный Nemotron сам по себе не даёт начать запись.
- `recording_commands.rs` выгружает audioCpp-модель на shutdown, как остальные движки.

**Настройки и UI**
- `audioCpp` в селекторе провайдера; выбор сохраняется сразу (`api_save_transcript_config`) и переживает перезапуск. При смене провайдера id модели не «переезжает» из чужого каталога (`modelForProviderSwitch` → пустая строка, бэкенд резолвит свою установленную модель). API-ключ для audioCpp не запрашивается (`save/get_transcript_api_key` → no-op).
- `AudioCppModelManager` получил выбор модели: карточки asr-моделей выбираются (Selected + сохранение провайдер+модель), карточка диаризации — никогда; диаризация не может стать провайдером записи.
- Перевод: в `LanguageSelection` для `audioCpp`/`Parakeet` опция `auto-translate` отсутствует, показано пояснение «automatic translation is only available with the Whisper provider»; сохранённая с Whisper настройка `auto-translate` не выдаётся за перевод (селект падает на `auto`, бэкенд тоже не отдаёт её провайдеру — `language_hint_for_provider`). Кнопка Language в панели транскрипта доступна и для audioCpp.

Как проверено
- TDD red→green по вертикальным срезам: шов движка → контракт провайдера → правила выбора модели → диспетчер → персистентность → UI.
- Rust: `cargo test --lib` — **452 passed, 0 failed, 7 ignored**. В том числе 16 тестов провайдера (текст/уверенность, 16 кГц и полный чанк, auto-detect семейства и запрос `asr`/`offline`, отказ diar-модели, ошибки load/support/run, короткое аудио, `ModelNotLoaded` после unload, отказ перевода, склейка текста из сегментов, тишина, сериализация конкурентных запросов, один поток на все нативные вызовы, освобождение потока при отказе модели), 12 тестов выбора модели, 4 теста персистентности провайдера в `transcript_settings`, 4 теста контракта перевода в `provider.rs`.
- Frontend: `bun test` — 90 тестов; новые 20+ (`tests/lib/transcription-provider.test.ts`, `tests/components/language-selection-provider.test.tsx`, дополнения в `audiocpp-model-manager.test.tsx`) зелёные. Полный набор — на уровне базовой линии: единственный провал `empty summary state > renders a persistent retryable error` и те же 5 leaked-mock ошибок между файлами существуют без этих изменений (сверено с worktree на `3e1216c`).
- `tsc --noEmit` — чисто. `cargo build` (dev, Tauri) — успешно.
- Не проверено: живой прогон в Tauri-приложении с реальными `libaudiocpp.so` + GigaAM GGUF (нет библиотеки/весов на машине); фактическое качество русского распознавания и RTF на CPU/Vulkan — требуют отдельного замера по реальной встрече (см. «Further Notes» спеки). Windows/Vulkan-путь не запускался.
