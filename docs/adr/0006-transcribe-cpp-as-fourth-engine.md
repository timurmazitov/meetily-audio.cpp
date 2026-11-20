# 0006. transcribe.cpp как четвёртый движок транскрибации через in-process FFI

Добавляем transcribe.cpp ([handy-computer/transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), MIT, ggml-рантайм) четвёртым провайдером транскрибации `transcribeCpp` рядом с Whisper (`localWhisper`), Parakeet (`parakeet`) и audio.cpp (`audioCpp`), за существующим трейтом `TranscriptionProvider`. Мотивация: движки Whisper и Parakeet команда планирует удалить, а качество русской ASR у Whisper заметно хуже современных ggml-моделей (WER fleurs-ru: ~8.4% у GigaAM v3, ~6-7% у qwen3-asr-0.6b, ~5.0% у whisper-large-v3). Один движок на GGUF-моделях должен закрыть и русский, и мультиязычное покрытие: в каталог встроены девять RU-содержащих GGUF-вариантов семейств gigaam-v3 (×4), parakeet-tdt-0.6b-v3, parakeet-ultra, qwen3-asr-0.6b, whisper-large-v3-turbo/large-v3 с выбором кванта (дефолт Q8_0); дефолтная модель провайдера — `parakeet-ultra` (выбрана по мультиязычности/размеру). Интеграция повторяет проверенный паттерн audio.cpp (ADR 0001): in-process FFI через C ABI с рукописным `libloading` subset (~10 символов: инициализация бэкендов, device API, загрузка модели, сессия, run), без потокового API transcribe.cpp — ни одна RU-модель набора не потоковая, используется только батчевый offline-путь (VAD-чанки 16 kHz mono f32 → `transcribe_run`). Нативная библиотека берётся из релизных prebuilt-архивов v0.4.2 (lanes `linux-x86_64-cpu-vulkan`, `linux-aarch64-cpu-vulkan`, `windows-x86_64-cpu-vulkan`, `macos-arm64-metal`; CUDA-lane вне скоупа) скриптом `frontend/scripts/stage-transcribe-native.mjs` с SHA-256-пином. ABI-гейт: рядом с библиотекой лежит `contract.json` релиза; до первого вызова (и до самого dlopen) запиненные в коде `version` (0.4.2) и `header_hash` (f99ad4169374b8f3) сверяются с ним — несовпадение даёт типизированную ошибку несовместимого ABI вместо краша. Сессии transcribe.cpp `Send`, но не `Sync`, и в 0.x допускают один одновременный run на модель → одна выделенная сессионная нить на движок (паттерн session-thread audio-cpp-модуля). Диаризация остаётся на audio.cpp и не затрагивается.

## Considered Options

- **Официальные Rust-биндинги `transcribe-cpp-sys` (сборка из исходников через CMake)**: готовый FFI, но crate собирает библиотеку из исходников, а релизный prebuilt-архив **не является CMake install-префиксом** (нет link-манифеста/config-пакета; на Windows-колесах вообще нет import-.lib), поэтому использовать crate с готовым архивом нельзя, а сборка из исходников потребовала бы на каждой машине CMake, C++-компилятор и `glslc` (компиляция Vulkan-шейдеров ggml). Отклонено: лишает сборки воспроизводимости prebuilt-стейджинга и дублирует ту же цену, что whisper-rs платит за build-from-source.
- **Sidecar-процесс** (по образцу `llama-helper`, IPC): даёт изоляцию падений молодой 0.x-библиотеки, но добавляет протокол, управление процессом и сериализацию PCM между потоками. Отклонено по той же причине, что и для audio.cpp в ADR 0001: прецедент in-process интеграций уже приживился, блокирующие вызовы прячутся в `spawn_blocking`/сессионную нить, а вся работа с либой изолирована в одном модуле-обёртке `transcribe_cpp_engine/` — переезд на sidecar при стабильных крашах останется локальным изменением.
- **dlopen без ABI-гейта** (грузим что нашли): дешевле на один символ меньше, но для 0.x-библиотеки с меняющимся C ABI означает краш вместо ошибки при любом дрейфе версии рядом с пип-стейджингом. Отклонено: `contract.json` уже лежит в архиве релиза, проверка двух полей до dlopen — копейки за предсказуемость (тот же гейт применяет официальный TypeScript-биндинг transcribe.cpp).

## Consequences

- Движок young (0.x): апгрейд версии transcribe.cpp = скоординированное изменение **пина версии + header_hash** (`transcribe_cpp_engine/error.rs`), **SHA-256 всех lane** (`frontend/scripts/stage-transcribe-native.mjs`) и **FFI-subset** в одном месте модуля-обёртки; стоимость апгрейда предсказуема именно потому, что все три меняются вместе. При каждом апгрейде прогонять Vulkan-проверку вручную (live-smoke `tests/transcribecpp_live_smoke.rs`, `#[ignore]`).
- Пин contract.json (`version=0.4.2`, `header_hash=f99ad4169374b8f3`) фиксирует ABI до первых вызовов; несовместимая библиотека отклоняется ошибкой `IncompatibleAbi`, код из неё не выполняется.
- Только offline-ASR: потоковый API, Sortformer-диаризация, LangID и VAD-роли transcribe.cpp не используются; перевод через новый провайдер не поддерживается (авто-перевод остаётся фичей Whisper и теряется при переходе с Whisper — см. таблицу соответствий в `.scratch/transcribe-cpp-provider/spec.md`).
- Windows: в отличие от audio.cpp, MSVC-редистристуемые DLL и `vulkan-1.dll` не бандлятся (upstream чинит колёса `delvewheel repair --exclude vulkan-1.dll` и никогда не вендорит MSVC-рантайм) — ожидаем от системы VC++ 2015-2022 Redistributable и Vulkan-загрузчик из драйверов.
- Режимы вычислений «Авто / Vulkan / CPU»: библиотека фиксирует маску допустимых бэкендов при первой регистрации на процесс, поэтому смена режима после первой загрузки движка применяется только после перезапуска; при недоступном Vulkan — авто-фолбэк на CPU с пользовательским предупреждением.
- Существующие провайдеры не меняются: значения `localWhisper`, `parakeet`, `audioCpp` неизменны, фактическое удаление Whisper и Parakeet — отдельная будущая задача (таблица соответствий — в спеке фичи).

## Лицензии распространяемых компонентов

Отдельного THIRD-PARTY-файла в репозитории нет (только `LICENSE.md` приложения, MIT); лицензии весов моделей, как и в ADR 0001 для audio.cpp, фиксируются здесь:

| Компонент | Источник | Лицензия |
| --- | --- | --- |
| transcribe.cpp (код движка, рантайм из релизных архивов) | handy-computer/transcribe.cpp | MIT (архив несёт `licenses/LICENSE`) |
| gigaam-v3-ctc / e2e-ctc / rnnt / e2e-rnnt | ai-sage/GigaAM-v3 (конвертация handy-computer) | MIT |
| parakeet-tdt-0.6b-v3 | nvidia/parakeet-tdt-0.6b-v3 | Apache-2.0 |
| parakeet-ultra | moondream/parakeet-ultra | Apache-2.0 |
| qwen3-asr-0.6b | Qwen/Qwen3-ASR-0.6B | Apache-2.0 |
| whisper-large-v3-turbo, whisper-large-v3 | openai/whisper-large-* (GGUF-конвертации handy-computer) | Apache-2.0 |

Модели скачиваются с Hugging Face `handy-computer/*-gguf` (неизменяемые resolve-URL с точным размером и SHA-256 в каталоге); каждый репозиторий несёт собственные `LICENSE`/`NOTICE`.

## Relation to ADR 0001

[ADR 0001](0001-audio-cpp-as-third-engine.md) ранее отклонил transcribe.cpp как раз из-за отсутствия поддержки Nemotron-3-Diarization (открытый issue #170; GGUF Nemotron упакован именно под audio.cpp). То возражение к этому решению не применимо: диаризация остаётся на audio.cpp (ADR 0004), а transcribe.cpp добавляется как **движок распознавания речи** с моделями, которых у audio.cpp нет. Записи не конфликтуют: audio.cpp остаётся третьим движком и единственным исполнителем диаризации.
