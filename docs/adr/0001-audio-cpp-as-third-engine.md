# audio.cpp как третий движок транскрибации через in-process FFI

Добавляем audio.cpp (0xShug0/audio.cpp, Apache-2.0) третьим провайдером транскрибации рядом с Whisper (whisper-rs) и Parakeet (ONNX), за существующим трейтом `TranscriptionProvider` со значением конфига `audioCpp`. Интеграция — in-process FFI через C ABI (`libaudiocpp.so` / `audiocpp.dll` из релизных lib-архивов, v0.9.4, ABI 0.2.0), с рукописным `extern "C"` биндингом (официальных Rust-биндингов нет). Первые модели: GigaAM v3 e2e CTC (русская ASR, семейство `gigaam_asr`) и Nemotron-3-Diarization BF16 (семейство `nemotron_3_diar`, до 8 спикеров); движок пишется family-agnostic с авто-детектом семейства из embedded spec в GGUF. Модели скачиваются из HuggingFace через существующий UI управления моделями. Платформы первой итерации: Linux и Windows; macOS — отдельным шагом.

## Considered Options

- **Sidecar-процесс** (по образцу `llama-helper`, JSON-over-stdio): даёт изоляцию падений молодой нативной либы, но добавляет IPC, управление процессом и сериализацию PCM. Отклонено: прецедент whisper-rs — in-process, ABI audio.cpp спроектирован для встраивания (opaque handles, out-of-order free безопасен), блокирующие вызовы прячутся в `spawn_blocking`, а вся работа с либой изолирована в одном модуле-обёртке — переезд на sidecar при стабильных крашах останется локальным изменением.
- **transcribe.cpp** (MIT, официальный Rust-биндинг): не поддерживает Nemotron-3-Diarization (открытый issue #170), GGUF Nemotron упакован именно под audio.cpp. Отклонено — исследование: `docs/research/audio-cpp-vs-transcribe-cpp.md`.
- **Полная замена whisper.cpp**: отклонено — audio.cpp становится третьим провайдером, Whisper и Parakeet сохраняются.

## Consequences

- GigaAM не поддерживает перевод: для провайдера `audioCpp` опция авто-перевода блокируется с пояснением; перевод остаётся фичей Whisper-провайдера (явный отказ от тихого fallback на вторую модель).
- Диаризация встраивается атрибуцией на уровне сегментов: после транскрибации повороты спикеров накладываются на сегменты по таймстемпам, у сегмента появляется nullable-поле `speaker` (миграция БД).
- Хендлы audio.cpp не потокобезопасны: одна сессия — один поток (соответствует существующему worker-pool); `audiocpp_session_run` только через `spawn_blocking`.
- Windows: аудио.cpp собирается MSVC — бандлим `msvcp140*`/`vcruntime140*`/`vcomp140` DLL рядом с приложением.
- Версионирование: пин на релизных lib-архивах, гейт по `audiocpp_abi_version()` (major must match, minor только добавляет entry points).
- Лицензии весов отделены от кода движка: GigaAM — MIT, Nemotron-3-Diarization — OpenMDW-1.1 (учитывать при распространении).
