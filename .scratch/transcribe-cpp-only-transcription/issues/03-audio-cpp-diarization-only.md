# 03: audio.cpp — только диаризация

**What to build:** Модуль audio.cpp имеет одну ответственность — диаризация. Модель GigaAM и ASR-задача убраны из статического каталога: каталог содержит только модель диаризации Nemotron. ASR-провайдер транскрибации audio.cpp удалён из рантайма транскрибации: исчезли его ветки в перечислении движка транскрибации и в батч-перечислении, путь инициализации/выгрузки при остановке записи и пакетной обработки, ASR-режим менеджера загрузок моделей на фронтенде. Диаризация работает как раньше: независимый этап поверх готового транскрипта (ADR 0004), панель Speakers, справочник людей и команды атрибуции спикеров не меняются. Ни один пользовательский путь не может выбрать audio.cpp как движок распознавания речи.

**Blocked by:** 02 (Экран настроек — только Transcribe.CPP)

**Status:** resolved

- [x] Каталог audio.cpp содержит только Nemotron-диаризацию; ASR-задание и GigaAM отсутствуют; каталожные Rust-тесты обновлены
- [x] ASR-провайдер транскрибации audio.cpp удалён; в батч-перечислении вариантов транскрибации, кроме TranscribeCpp, не осталось (неизвестный провайдер маппится на TranscribeCpp)
- [x] Путь выгрузки движка при остановке записи и в пакетной обработке не содержит audioCpp
- [x] Менеджер моделей audio.cpp на фронтенде поддерживает только режим диаризации; его тесты обновлены
- [x] Диаризация сохранённой встречи работает как раньше (демонстрация на существующей встрече); тесты диаризации зелёные
- [x] `cargo check` и тесты Rust зелёные

## Comments

- Ветка: `impl/tcp-only/03-audiocpp-diar-only`, влита merge-коммитом `87713e2` в `impl/transcribe-cpp-only-transcription`; конфликтов не было.
- Каталог: GigaAM убран, `ModelTask::Asr` удалён (enum схлопнут в `Diar`), новый тест `catalog_holds_only_the_nemotron_diarization_model`, тест сериализации переведён на `task == "diar"`.
- ASR-провайдер удалён: `audio_cpp_provider.rs` (819 строк), `audio_cpp_engine/transcription.rs` (470 строк), ветки в `validate_transcription_model_ready`/`get_or_init_transcription_engine`, `BatchEngine::AudioCpp`, выгрузка при стопе записи и в батчах, ветки сегментации в `retranscription.rs`/`import.rs`.
- Разделение общего FFI-трейта (ловушка): у `AudioCppEngine`/`AudioCppModel` сохранён диаризационный срез (`load_model`, `family`, `supports`, `diarize`), удалён ASR-срез (`transcribe_asr`, `TranscriptionOutput`, `ASR_TASK`); `load_native_engine` перенесён в `engine.rs`. Мёртвые ASR-символы FFI (`result_text`, `result_segment*`) убраны.
- `audiocpp_has_transcription_model` удалён; `const AUDIO_CPP_PROVIDER` сознательно оставлен (мягкая миграция и тесты репозитория настроек ещё называют legacy-конфиги).
- Rust-конфиг онбординга: `transcribeCpp` + `DEFAULT_TRANSCRIBE_CPP_MODEL`; тест-гард модели онбординга переведён на каталог transcribe.cpp.
- Фронт: `tests/lib/live-diarization.test.ts` переведён на diar-only фикстуры; защитные тесты чужих ASR-записей в менеджере сохранены. Диаризация (Nemotron, панель Speakers, справочник людей, команды атрибуции) не тронута.
- Проверка: `cargo check --tests` зелёный (варнинги как в baseline); `cargo test --lib` — 766 passed / 0 failed / 7 ignored; `bun test tests/` — 453 pass / 9 fail (ровно baseline, сверено stash-прогоном на `f3c7265`).
- Не проверено: живая диаризация реальной встречи в Tauri-приложении (нет экрана/устройств) — покрыто 51 юнит-тестом диаризации; отнесено к тикету 07.
