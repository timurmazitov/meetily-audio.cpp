# audio.cpp: C ABI для интеграции из Rust (исследование первоисточников)

Дата исследования: 2026-11-13. Репозиторий: <https://github.com/0xShug0/audio.cpp>.

**Актуальность версии.** Последний релиз — **v0.9.4** (опубликован 2026-11-05, `tag_name: "v0.9.4"`, не draft/prerelease): <https://api.github.com/repos/0xShug0/audio.cpp/releases/latest>. Новее версии на момент исследования нет. Ветка `main` (commit `eb8281c`) новее релиза: в заголовке на main поднят ABI minor 0.2 → 0.3 (добавлен `audiocpp_log_set`), в релизе этого ещё нет (см. ниже).

> **Update 2026-11-16.** Релиз **v0.9.5** (2026-11-14) вышел и взят как пин: ABI в его заголовке — **0.3.0**, `audiocpp_log_set` теперь в релизе (76 экспортов `audiocpp_*` в `libaudiocpp.so`, статус-коды 0..7 без изменений). FFI-совместимость сохранена, см. update в `docs/adr/0001-audio-cpp-as-third-engine.md`.

---

## 1. C API: публичный заголовок и экспортированные функции

Публичный C-заголовок — **единственный файл** `include/audiocpp.h`: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/include/audiocpp.h>. Реализация фасада — `src/capi/audiocpp.cpp`, карта экспорта — `src/capi/audiocpp.map` (ELF) и `src/capi/audiocpp.symbols` (Mach-O): <https://github.com/0xShug0/audio.cpp/tree/main/src/capi>. Документация ABI: `docs/c_api.md` — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/c_api.md>.

> «`include/audiocpp.h` is a C ABI for embedding audio.cpp **in-process**. It is a facade over `engine::runtime` and adds no behaviour of its own» — docs/c_api.md.

Из заголовка (контракт, дословно):

> «Opaque handles only. No C++ type crosses this boundary. / No exception escapes. Every entry point returns audiocpp_status; the detail for the last failing call on THIS thread is audiocpp_last_error(). / `const char *` and `const float *` outputs are BORROWED… / Handles are not thread-safe individually. Separate handles may be used concurrently from separate threads. / Freeing NULL is a no-op…» — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/include/audiocpp.h>

### Жизненный цикл (init → load → run → cleanup)

Отдельной «инициализации библиотеки» нет; точка входа — создание реестра:

| Этап | Функции |
|---|---|
| Версия | `audiocpp_abi_version()`, `audiocpp_build_version()` |
| Ошибки | `audiocpp_last_error()` (thread-local), `audiocpp_status_string()` |
| Лог (только main, ABI 0.3) | `audiocpp_log_set(callback, user_data)` |
| Реестр | `audiocpp_registry_create(config_path /*NULL = default*/, &reg)`, `audiocpp_registry_free()`, `audiocpp_registry_family_count/family()` |
| Опции (string→string) | `audiocpp_options_create/set/free` |
| Модель | `audiocpp_model_load(registry, model_path, &audiocpp_model_config, options /*= --load-option*/, &model)`, `audiocpp_model_free()` |
| Сессия | `audiocpp_session_create(model, task, mode, &audiocpp_backend_config, options /*= --session-option*/, &session)`, `audiocpp_session_create_with_impl(...)`, `audiocpp_session_prepare()`, `audiocpp_session_run(session, request, &result)`, `audiocpp_session_free()` |
| Запрос | `audiocpp_request_create/free`, `audiocpp_request_set_text(text, language)`, `audiocpp_request_set_text_language()`, `audiocpp_request_set_audio(samples, frames, sample_rate, channels)` — «Interleaved float PCM. Copied into the request», `audiocpp_request_set_option()`, `audiocpp_request_set_option_array()`, `audiocpp_request_set_voice_audio/id`, style-сеттеры, `audiocpp_request_add_artifact()` |
| Результат | `audiocpp_result_free()`, `audiocpp_result_text()`, `audiocpp_result_audio()`, `audiocpp_result_segment_count/segment()` (start/end sample + confidence + text), `audiocpp_result_word_count/word()` (word timestamps), `audiocpp_result_speaker_turn_count/speaker_turn()` (start/end sample, speaker_id, confidence, text), `audiocpp_result_named_audio_*()`, `audiocpp_result_artifact_*()` |
| Стриминг (pull-based) | `audiocpp_stream_policy()` (preferred chunk), `audiocpp_stream_start()`, `audiocpp_stream_push()`, `audiocpp_stream_next_event()`, `audiocpp_stream_finish()`, `audiocpp_stream_reset()`, `audiocpp_event_free/is_final/as_result`, `audiocpp_event_voice_activity_*()` |

Все URL: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/include/audiocpp.h>.

### Как работает «embedded model spec» (детект семейства)

Выбор семейства — **автодетект из самого GGUF**, подсказка не обязательна:

> «`family_hint --family, or NULL to identify the model from its path`» — `audiocpp_model_config` в audiocpp.h. Точнее: новые GGUF содержат спек в метаданных:

> «New GGUFs contain the selected spec in `audiocpp.model_spec.*` metadata, so a standalone GGUF does not depend on a `model_specs` directory or on the binary having that family compiled into its spec» — docs/gguf.md, <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/gguf.md>.

Порядок lookup контракта: «explicit override, local `model_specs/<family>.json`, compiled catalog, then the embedded GGUF spec» (там же). Через C ABI: `audiocpp_model_config.model_spec_override` + `--model-spec-override embedded`. Проверка возможностей: `audiocpp_model_supports(model, "asr"|"diar", "offline"|"streaming")`, перечисление опций семейства — `audiocpp_model_option_count/audiocpp_model_option` (scopes REQUEST/SESSION/LOAD). Все опции — плоские `string→string`, «adding a model family never changes this header» (docs/c_api.md).

### ASR (gigaam_asr) — офлайн

- Семейство `gigaam_asr`, task `"asr"`, mode `"offline"` (model_specs: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/model_specs/gigaam_asr.json>).
- Вход: интерливед f32 PCM любой частоты/канальности через `audiocpp_request_set_audio`; **внутри ресемплируется в 16 кГц моно**: `convert_interleaved_audio_to_mono_linear_resampled(..., 16000)` — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/src/models/gigaam_asr/session.cpp> (строка ~54). Док-ия: «Long recordings are split automatically» — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/gigaam_asr.md>.
- Длинные записи: чанкование request-опциями `audio_chunk_mode` (`auto/quiet_energy/fixed/vad/none`, default `auto`), `audio_chunk_duration_sec` (0.02–25, default 25), `return_timestamps` (word timestamps из эмиссии токенов CTC/RNN-T); session-опция `gigaam_asr.vad_model_path` для `vad`-чанкования. Те же опции доступны через C ABI (`audiocpp_options_set` / `audiocpp_request_set_option`) — не только через CLI.
- Результат: `audiocpp_result_text`, сегменты чанков (`audiocpp_result_segment_*`), слова с таймстампами (`audiocpp_result_word_*`).

### Диаризация (nemotron_3_diar) — офлайн и стриминг

- Семейство `nemotron_3_diar`, task `"diar"`, modes `offline` + `streaming` (model_specs: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/model_specs/nemotron_3_diar.json>). До 8 спикеров «by arrival order».
- Вход: «Input | 16 kHz WAV audio» — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/nemotron_3_diar.md>. Через C API — PCM через `audiocpp_request_set_audio` (офлайн) или `audiocpp_stream_push` (стриминг, session с mode `"streaming"`).
- Повёрнутые реплики: `audiocpp_result_speaker_turn_count/audiocpp_result_speaker_turn` (start/end sample, `speaker_id`, confidence, text). Стриминг — pull-based: «No callback crosses the FFI boundary» (docs/c_api.md).
- Request-опции: `speaker_threshold` (0–1, def 0.5), `speaker_min_frames`, `speaker_pad_frames`, `return_frame_probabilities`. Session-опции: `nemotron_3_diar.latency_profile` (`very_high/low/very_low/ultra_low/asr_la0/3/6/13/custom` — buffer latency от 30.4 с до 0.08 с), `attention` (`auto/flash/eager`), `graph_arena_mb`, `weight_context_mb`, `weight_type`. Всё это — через `audiocpp_session_create(..., options)`.

### Ошибки

Коды `audiocpp_status` (OK, INVALID_ARGUMENT, UNSUPPORTED_FAMILY, LOAD_FAILED, RUNTIME, OUT_OF_MEMORY, OUT_OF_RANGE, NOT_AVAILABLE); детали — `audiocpp_last_error()`, thread-local: `thread_local std::string g_last_error;` — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/src/capi/audiocpp.cpp> (строка 58).

---

## 2. Потоки / асинхронность

- Все вызовы **блокирующие**: «`audiocpp_session_run()` prepares the session implicitly» и возвращает готовый результат; фоновых задач/_future в ABI нет (docs/c_api.md).
- Thread-safety: «Handles are not thread-safe individually. Separate handles may be used concurrently from separate threads» (заголовок и docs/c_api.md). Лог-мьютекс внутри есть (`std::mutex g_log_mutex`), но это не делает хендлы потокобезопасными.
- Стриминг — **pull-based**, колбэки через FFI не пересекаются: «Pull-based, mirroring IStreamingVoiceTaskSession. No callback crosses the FFI boundary» (заголовок). Единственный колбэк в ABI — `audiocpp_log_set` (main/ABI 0.3; в релизе v0.9.4 его нет).
- SSE — только у HTTP-сервера (`/v1/audio/transcriptions/live`), к C ABI не относится: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/nemotron_3_diar.md>.
- **Rust Send/Sync**: хендлы opaque; безопасная модель — обернуть в struct и использовать с одного выделенного потока (реализация `Send` вручную, `Sync` — только с внешней синхронизацией). Thread-local `last_error` корректно работает при переносе вызовов между потоками, если не гоняться за одним хендлом с нескольких.
- Важно: «The library does **not** call `omp_set_num_threads()`… Set `threads` in `audiocpp_backend_config`» (docs/c_api.md) — число потоков контролируется вызывающей стороной.

---

## 3. Сборка / линковка (проверено скачиванием релизных архивов)

Релизные lib-архивы (`audio-v0.9.4-lib-<platform>-<backend>.tar.gz/.zip`, <https://github.com/0xShug0/audio.cpp/releases/tag/v0.9.4>):

**Состав архива** (проверено `tar tzf` / `unzip -l`):
- `include/audiocpp.h`, `LICENSE`, `libs/`.
- Linux: `libs/libaudiocpp.so.0.1.0` (+ symlink `.so.0`, `.so`), SONAME `libaudiocpp.so.0`.
- Windows: `libs/audiocpp.dll`, **`audiocpp.lib`/`audiocpp.exp`** + **бандл MSVC-рантайма**: `msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll` (OpenMP).

**Реальные зависимости** (проверено `readelf -d` / `objdump -p` на скачанных бинарниках):

| Платформа | Vulkan-сборка | CPU-сборка |
|---|---|---|
| Linux | `libvulkan.so.1`, `libgomp.so.1`, `libstdc++.so.6`, `libgcc_s.so.1`, `libm`, `libc` | то же без `libvulkan` |
| Windows | `vulkan-1.dll`, `MSVCP140.dll`, `VCRUNTIME140(_1).dll`, `VCOMP140.DLL`, `ADVAPI32`, `KERNEL32`, ucrt | — |

- Тулчейн Windows — **MSVC, не MinGW**: «Visual Studio Build Tools 2022 or newer … MSVC x64 compiler … MSVC OpenMP components» — README (<https://raw.githubusercontent.com/0xShug0/audio.cpp/main/README.md>, раздел Build). `Makefile.windows` — лишь обёртка над `scripts/build_windows.ps1`.
- «GPU drivers and the Vulkan loader must be installed on the host» — docs/c_api.md. Для Vulkan на CPU без AVX/AVX2 есть отдельные `*-portable` пакеты.
- Символы: «`libaudiocpp.so` exports exactly the … entry points in `audiocpp.h` and nothing else» через линкерные allowlist'ы `audiocpp.map`/`audiocpp.symbols`; на Windows — `.def`-подход и `CJSON_HIDE_SYMBOLS` (docs/c_api.md). Проверено: `nm -D --defined-only` на релизном `.so` = **75 экспортов, 0 не-`audiocpp_*`**.
- **Исключения через ABI не проходят**: каждая точка входа обёрнута в `try/catch(...)`: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/src/capi/audiocpp.cpp> (`catch (const std::bad_alloc&…)`, `catch (const std::exception&…)`, `catch (...)`), наружу — только `audiocpp_status`. C++-рантайм при этом требуется (libstdc++/MSVCP), но C++-типы и исключения границу не пересекают; заголовок компилируется чистым C (тест `tests/capi/path_test.c` собирается C-компилятором, docs/c_api.md).
- Backend-плагины: «Dynamic backend plugins are discovered beside the C API library on Linux, macOS, and Windows» (docs/c_api.md) — ggml-бэкенды в lib-архиве слинкованы статически (отдельных `libggml-*.so` в архиве нет), но соседние директории библиотеки всё равно участвуют в поиске.
- Версия ABI в релизе v0.9.4: `AUDIOCPP_ABI_VERSION 0.2.0` (75 функций; `audiocpp_log_set` и `sycl` в строке документации — только на main, ABI 0.3.0). Правило совместимости: major must match, minor только добавляет entry points, patch не гейтить (docs/c_api.md).

---

## 4. Загрузка моделей: GigaAM и Nemotron через одну точку входа

Да, обе — через `audiocpp_model_load` с авто-детектом семейства из embedded spec (`family_hint = NULL`). Различия — только в task/mode и опциях:

| | gigaam_asr | nemotron_3_diar |
|---|---|---|
| task/mode | `asr`/`offline` | `diar`/`offline` + `streaming` |
| Чанкинг long-form | request-опции `audio_chunk_mode`, `audio_chunk_duration_sec` — **доступны через C ABI** (не только CLI) | — |
| Латентность стриминга | — | session-опция `nemotron_3_diar.latency_profile` (`very_high`…`ultra_low`/`asr_laN`/`custom`) — **доступна через C ABI** |
| Веса | GGUF F16/F32 (<https://huggingface.co/ai-sage/GigaAM-v3> ревизии) | GGUF BF16/Q8_0 (<https://huggingface.co/audio-cpp/Nemotron-3-Diarization-GGUF>) |
| Лицензия весов | MIT | OpenMDW-1.1 (checkpoint) |

Источники: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/gigaam_asr.md>, <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/nemotron_3_diar.md>, model_specs/*.json.

Пер-семейные опции НЕ захардкожены в ABI — они приходят через string→string options, а интроспекция (`audiocpp_model_option_count/option`) позволяет биндингу перечислить их в рантайме (docs/c_api.md).

---

## 5. Ожидания по производительности (RTF)

Документированных **CPU/Vulkan RTF именно для `gigaam_asr` и `nemotron_3_diar` нет**. Что есть в первоисточниках:

- `nemotron_3_diar`, CUDA RTX 4070 Ti SUPER: «costs about 35% more time (900 s of audio in 16.8 s instead of 12.5 s…)» → RTF ≈ 0.019 (eager) / ≈ 0.014 (flash attention, default `auto`) — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/nemotron_3_diar.md>.
- `nemotron_asr` (другая модель!), CUDA, из docs/reports/asr_test.md: offline RTF 0.0066 (327.6 s за 2.173 s), one-shot streaming RTF 0.0298 — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/reports/asr_test.md>.
- Раздел производительности README измерен только «on Ubuntu with the CUDA backend on an NVIDIA GeForce RTX 5090» и покрывает в основном TTS: «Absolute RTF depends on the GPU and system setup» — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/README.md>.
- Общая оговорка по бэкендам: «CUDA is the optimized path… CPU, Vulkan, Metal, and HIP are intended for portability and testing… performance and model coverage may be lower» (README, раздел серверного конфига).

Вывод: для CPU/Vulkan-бенчей этих двух семейств нужен собственный замер (CLI умеет `--metrics` — «prints compact offline wall time, audio duration, RTF, realtime speed», README).

---

## 6. Лицензия и линковка в дистрибутивный бинарник

- Лицензия проекта — **Apache-2.0** (Copyright 2026 ShugoAI LLC): <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/LICENSE>.
- **NOTICE-файла в корне репозитория нет** (в дереве main есть только `tests/mulacover/NOTICE` — тестовый ассет). Следовательно, пункт 4(d) Apache-2.0 не активируется, но остаются:
  - 4(a): выдать получателям копию лицензии;
  - 4(c): сохранить уведомления об авторских правах/патентах/атрибуции в исходных формах;
  - при модификации — 4(b) (prominent notices об изменённых файлах).
- Лицензии **весов моделей — отдельные**: GigaAM — MIT, checkpoint Nemotron 3 Diarization — OpenMDW-1.1 (см. docs/models/*). Встраивание весов в дистрибутив регулируется ими, а не Apache-2.0 audio.cpp.
- В линковке (динамическая через `libaudiocpp.so`/`audiocpp.dll`) нет copyleft-препятствий; внутри библиотеки уже слинкованы ggml (MIT), cJSON (MIT), sentencepiece (Apache) и др. — их уведомления покрываются тем, что лицензия библиотеки идёт в комплекте архива (`LICENSE` в каждом lib-архиве).

---

## 7. Rust-биндинги

- В репозитории **нет** `bindings/` и никаких Rust-файлов (полный список дерева `main`: <https://api.github.com/repos/0xShug0/audio.cpp/git/trees/main?recursive=1>).
- На crates.io/community пакетов для audio.cpp не найдено (поиск по crates.io на 2026-11-13). Заголовок ABI-стабилен и «opaque handles only», поэтому биндинг тривиален вручную через `bindgen`/`cbindgen`-стиль: ABI — плоский C, 75 символов, `extern "C"` без исключений.
- Контракт ABI прямо проектировался под GC-языки: «This is deliberate. Callers from garbage-collected runtimes cannot control the order finalizers run in…» (docs/c_api.md) — out-of-order free безопасен (хендлы держат родителей).

---

## Потенциальные showstoppers для FFI из Rust

1. **ABI-версия релиза (0.2.0) отстаёт от main (0.3.0)**: `audiocpp_log_set` (перехват логов из Rust) доступен только на main; в v0.9.4 логи идут в stderr. Гейтить по `audiocpp_abi_version()`.
2. **Хендлы не потокобезопасны по отдельности** — один session/model/request обслуживать с одного потока; параллелизм — только через разные сессии.
3. **Блокирующие вызовы**: длинный ASR/diar на CPU блокирует поток; из async-рантайма звать через `spawn_blocking`/выделенный поток.
4. **Windows = MSVC**: `audiocpp.dll` требует MSVC-рантайм (бандлится в `libs/` архива; vcomp140 = OpenMP). Rust-приложение на `-gnu` тулчейне должно поставлять эти DLL рядом; на `-msvc` — согласовать с Visual C++ Redist. lib-архив для импорта — `audiocpp.lib` (MSVC), для GNU-линковки понадобится import library из `.def`/или линковка через `dllimport`.
5. **Входной PCM для `nemotron_3_diar` — 16 кГц** (документировано); `gigaam_asr` ресемплит в 16 кГц сам. Ресемплинг в аудио-пайплайне приложения (Samobranka уже имеет) дублировать не нужно для GigaAM, но для диаризации лучше отдавать 16 кГц.
6. Документированных CPU/Vulkan RTF-чисел нет — риск производительности на CPU подтверждается только замером; CUDA — «optimized path», CPU/Vulkan «intended for portability and testing» (README).
