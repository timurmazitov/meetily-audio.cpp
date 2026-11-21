# Сравнение audio.cpp vs transcribe.cpp как замены whisper.cpp

> **Историческое исследование** (см. также [ADR 0007](../adr/0007-consolidation-on-transcribe-cpp.md)): на момент записи в приложении ещё были движки Whisper/Parakeet; сейчас локальная транскрибация консолидирована на transcribe.cpp.

Дата исследования: 2026-11-13. Источники: README и исходники репозиториев на GitHub, карточки моделей на Hugging Face, issue-трекеры. Всё ниже — только из первоисточников.

Задача приложения (Samobranka): запись/транскрибация встреч, Rust/Tauri-ядро, нужны две модели:

1. **GigaAM v3 e2e CTC** — русскоязычная ASR (SberDevices / ai-sage): <https://huggingface.co/handy-computer/gigaam-v3-e2e-ctc-gguf>
2. **Nemotron-3-Diarization** — диаризация до 8 спикеров (NVIDIA): <https://huggingface.co/audio-cpp/Nemotron-3-Diarization-GGUF>

---

## 1. audio.cpp — <https://github.com/0xShug0/audio.cpp>

### Что это

- «All-in-one, pure C++ inference engine for audio models, powered by ggml» — фреймворк на ggml/GGUF с CLI (`audiocpp_cli`), HTTP-сервером (`audiocpp_server`) и встроенным WebUI (описание репозитория: <https://api.github.com/repos/0xShug0/audio.cpp>).
- Охват: 143 семейства моделей / 250+ вариантов — TTS, ASR, VAD, диаризация, форсед-алайнмент, сепарация, музыка и пр. (README, раздел Supported Models: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/README.md>).
- Лицензия: **Apache 2.0** (файл LICENSE: «Copyright 2026 ShugoAI LLC, Licensed under the Apache License, Version 2.0» — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/LICENSE>). GitHub API почему-то показывает license «Other/NOASSERTION» (<https://api.github.com/repos/0xShug0/audio.cpp>) — но текст LICENSE однозначно Apache 2.0.
- Активность: создан 2026-06-23, последний push 2026-11-13, 4207 звёзд, 14 открытых issues (<https://api.github.com/repos/0xShug0/audio.cpp>). Последний релиз **v0.9.4 от 2026-11-05** с готовыми бинарниками Windows/Linux/macOS (CPU/CUDA/Vulkan/HIP/Metal) и отдельными **C API библиотеками** (`audio-v0.9.4-lib-*.tar.gz`) — <https://api.github.com/repos/0xShug0/audio.cpp/releases/latest>. Релизы выходят очень часто (0.9.0 → 0.9.4 за ~5 недель, новости в README).

### Поддержка GigaAM

**Поддерживается (включая именно e2e CTC).** Подтверждение на уровне «документация + пакеты + пример CLI»:

- В таблице Supported Models семейство `gigaam_asr`: задача ASR, языки ru/kk/ky/uz/en, варианты «GigaAM v3 CTC, GigaAM v3 RNN-T, **GigaAM v3 E2E CTC**, GigaAM v3 E2E RNN-T, GigaAM Multilingual CTC, GigaAM Multilingual Large CTC», формат GGUF F16/F32 (README: <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/README.md>).
- Отдельная документация `docs/models/gigaam_asr.md`: таблица вариантов с именами GGUF-файлов (`gigaam-v3-e2e-ctc-f16.gguf` и др.), инструкция конвертации (`tests/gigaam_asr/convert_gguf.py`), рабочий пример CLI:
  `audiocpp_cli --task asr --family gigaam_asr --model .../gigaam-v3-ctc-f16.gguf --backend cuda --audio speech.wav --text-out transcript.txt` — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/gigaam_asr.md>.
- Готовые GGUF-пакеты лежат в организации audio-cpp: `audio-cpp/audio.cpp-gguf/GigaAM-ASR-GGUF/` содержит в т.ч. `gigaam-v3-e2e-ctc-f16.gguf` (442 520 704 байт) — <https://huggingface.co/api/models/audio-cpp/audio.cpp-gguf/tree/main/GigaAM-ASR-GGUF>.
- Варианты GigaAM **v2** в README/доках не упоминаются — только v3 и Multilingual (**не подтверждено** для v2; судя по списку — не поддерживается). Варианты enc+dec / phone отдельно не перечислены.
- Ограничения порта (из docs/models/gigaam_asr.md): long-form через чанкование (quiet-energy / VAD / fixed, до 25 с на чанк), word timestamps есть, перевод и спикер-лейблы не поддерживаются.

**Важный нюанс совместимости:** конкретный GGUF из задания (`handy-computer/gigaam-v3-e2e-ctc-gguf`) собран **для transcribe.cpp** — в его карточке `library_name: transcribe.cpp` и метаданные `transcribe_cpp:` (<https://huggingface.co/handy-computer/gigaam-v3-e2e-ctc-gguf/raw/main/README.md>). audio.cpp использует собственную упаковку GGUF с «embedded model spec» (упоминается в docs/models/nemotron_3_diar.md и gigaam_asr.md). Загрузится ли handy-computer GGUF в audio.cpp напрямую — **не подтверждено**; гарантированный путь для audio.cpp — их собственный пакет `GigaAM-ASR-GGUF/gigaam-v3-e2e-ctc-f16.gguf` или конвертация их скриптом.

### Поддержка Nemotron-3-Diarization

**Поддерживается — это «родной» пакет именно для audio.cpp.** Подтверждение максимального уровня (документация + официальный GGUF-репозиторий + примеры CLI/server + валидация):

- Семейство `nemotron_3_diar` в таблице Supported Models: «NVIDIA Nemotron 3 Diarization with eight-speaker arrival-order diarization», GGUF BF16, Batch, Stream (README).
- Карточка модели из задания (`audio-cpp/Nemotron-3-Diarization-GGUF`) буквально называется «Use With audio.cpp»: «GGUF weights for native inference with audio.cpp. Tested on CUDA, Vulkan, and Metal(Mac) backends», файлы `nemotron-3-diarization-bf16.gguf` (189.5 MiB) и `nemotron-3-diarization-q8_0.gguf` (101.7 MiB), в файле «embedded model spec» — <https://huggingface.co/audio-cpp/Nemotron-3-Diarization-GGUF/raw/main/README.md>.
- Документация `docs/models/nemotron_3_diar.md`: задача `diar`, режимы offline/streaming/batch, вход 16 кГц WAV, вывод — speaker turns (`--turns-out`); рабочие примеры CLI (offline и streaming с latency-профилями) и серверного батчинга; конвертер `tests/nemotron_3_diar/convert_gguf.py` из `.nemo`; валидация против NeMo Python и NVIDIA NeMo-Speech.cpp с численными отклонениями — <https://raw.githubusercontent.com/0xShug0/audio.cpp/main/docs/models/nemotron_3_diar.md>.
- Задача модели — **чистая диаризация** (who spoke when, до 8 спикеров по порядку появления, повороты спикеров с таймстемпами), а не транскрипция: «It returns speaker turns and timestamps, not transcribed words» (карточка модели). Отдельно есть интеграция с ASR: семейство `nemotron_asr` (Nemotron 3.5 ASR) умеет speaker-tagged transcription, читая `speaker_probabilities` от `nemotron_3_diar` (docs/models/nemotron_3_diar.md, раздел Request Options; docs/asr.md).
- Известные ограничения (карточка модели + docs): Q8_0 «not exact-parity-safe» — может менять границы поворотов и их число (рекомендуют BF16); модель чувствительна к мелким изменениям входа на длинных стримах.

### Сборка и платформы

- ggml vendored; бэкенды: CPU, CUDA, HIP/ROCm, Vulkan, Metal, WebGPU (WASM), SYCL (README, бейджи «Supported backends»).
- Сборка: CMake + helper-скрипты (`scripts/build_linux.sh --backend cuda|vulkan|hip|cpu`, PowerShell для Windows, `scripts/build_metal.sh` для macOS); GCC 13+ / VS 2022 / Xcode; поддерживаются composite builds только нужных семейств (`--model-set custom --models gigaam_asr,nemotron_3_diar` и т.п.) — README, раздел Build yourself.
- Готовые prebuilt-бинарники и C API libs на странице релизов (см. выше) — можно не собирать.
- Платформы: Windows x64, Ubuntu x64, macOS arm64/x64 (README, Prebuilt Binaries).

---

## 2. transcribe.cpp — <https://github.com/handy-computer/transcribe.cpp>

### Что это

- «C/C++ speech-to-text inference library» на GGUF/ggml: библиотека с **единым C API (single header `include/transcribe.h`)**, CLI (`transcribe-cli`), конвертером и квантизатором; официальные биндинги **Python, TypeScript/JavaScript, Rust, Swift/ObjC** (README, разделы Project layout и Bindings: <https://raw.githubusercontent.com/handy-computer/transcribe.cpp/main/README.md>).
- Охват: 16 семейств ASR / 60+ вариантов (Parakeet, Canary, Qwen3-ASR, Whisper, Moonshine, GigaAM-v3, Nemotron 3.5 ASR и др.), плюс отдельно VAD (Silero), langID (VoxLingua107) и одна модель диаризации (Sortformer). Каждая опубликованная модель «numerically verified and WER-tested against its reference implementation» (README).
- Лицензия: **MIT** (<https://api.github.com/repos/handy-computer/transcribe.cpp>, `license.spdx_id: MIT`).
- Активность: создан 2026-04-07, последний push 2026-11-11, 2168 звёзд, 67 открытых issues. Последний релиз **v0.4.2 от 2026-11-12** с нативными пакетами (linux/macos/windows, cpu/vulkan/cuda/metal), Python wheel, xcframework для Apple — <https://api.github.com/repos/handy-computer/transcribe.cpp/releases/latest>.

### Поддержка GigaAM

**Поддерживается — включая именно e2e CTC, это «родной» пакет для transcribe.cpp.** Подтверждение максимального уровня:

- В таблице Supported models семейство GigaAM-v3: варианты `gigaam-v3-ctc`, **`gigaam-v3-e2e-ctc`**, `gigaam-v3-e2e-rnnt`, `gigaam-v3-rnnt`, capability «token timestamps» (README).
- Семейная документация `docs/models/gigaam.md`: все 4 варианта v3 (2×2: CTC/RNN-T × e2e/charwise) с WER на FLEURS ru (e2e-ctc Q8_0 — 5.53%), отдельный раздел про GigaAM Multilingual Large CTC (конвертация своим скриптом), quick start с `transcribe-cli -m .../gigaam-v3-e2e-rnnt-Q8_0.gguf samples/jfk.wav` — <https://raw.githubusercontent.com/handy-computer/transcribe.cpp/main/docs/models/gigaam.md>.
- Карточка конкретной модели из задания: `library_name: transcribe.cpp`, валидация против референса (FLEURS ru WER: F32 5.50%, Q8_0 5.53%), таблица квантов F32/F16/Q8_0/Q6_K/Q5_K_M/Q4_K_M и рабочий пример `build/bin/transcribe-cli -m gigaam-v3-e2e-ctc-Q8_0.gguf input.wav` — <https://huggingface.co/handy-computer/gigaam-v3-e2e-ctc-gguf/raw/main/README.md>.
- Ограничения (docs/models/gigaam.md): только офлайн (streaming: false), без VAD и диаризации внутри семейства, окно ~25 с (длинный аудио режется на перекрывающиеся окна с последующей склейкой).
- GigaAM **v2** нигде не упоминается (**не поддерживается** по первоисточникам).

### Поддержка Nemotron-3-Diarization

**НЕ поддерживается.** Подтверждено тремя независимыми первоисточниками:

- В каталоге моделей transcribe.cpp единственная standalone-модель диаризации — «Streaming Sortformer Diarizer 4spk v2.1»; Nemotron 3 Diarization отсутствует в списке семейств (README; листинг docs/models: <https://api.github.com/repos/handy-computer/transcribe.cpp/contents/docs/models> — есть `diar_streaming_sortformer_4spk-v2.1.md`, `nemotron-3.5-asr-streaming-0.6b.md` (это ASR, другая модель), и нет ничего про Nemotron Diarization).
- Открытый issue **#170 «Add Nemotron 3 Diarization»** (создан 2026-09-24, статус open на момент исследования): пользователь просит добавить именно <https://huggingface.co/nvidia/Nemotron-3-Diarization>, «It is a lighter, more capable version of Sortformer» — <https://github.com/handy-computer/transcribe.cpp/issues/170>.
- GGUF из задания упакован для audio.cpp («embedded model spec», «Use With audio.cpp») — см. карточку выше. Даже форматно это не пакет transcribe.cpp.
- Что есть взамен: диаризация через другие модели — Sortformer 4spk v2.1 (standalone, потоковая), MOSS-Transcribe-Diarize и Multitalker Parakeet (ASR со спикер-лейблами) (README). Это **не** Nemotron-3-Diarization.

### Сборка и платформы

- Vendored ggml (`ggml/` + downstream-патчи), vendored miniz; C++17 (README, Project layout).
- Бэкенды: CPU (tinyBLAS по умолчанию, опционально OpenBLAS «~10-15x» для хост-декодера), Metal (авто на Apple Silicon), Vulkan (`-DTRANSCRIBE_VULKAN=ON`), CUDA (`-DTRANSCRIBE_CUDA=ON`), HIP/ROCm 6.1+ (`-DTRANSCRIBE_HIP=ON`) — README, раздел Build.
- Сборка: `cmake -B build && cmake --build build`; тесты `ctest` (плюс real-model тесты через `-DTRANSCRIBE_BUILD_REAL_MODEL_TESTS=ON` и env с путём к GGUF) — README.
- Платформы: Linux x86_64/aarch64, macOS (вкл. xcframework), Windows x86_64 (гайд docs/build-windows.md) — по релизам и README.
- Модели: готовые GGUF под каждую поддерживаемую модель в организации <https://huggingface.co/handy-computer>; конвертеры (Python/uv, NeMo) и квантизатор `transcribe-quantize` в репо.

### Качество/зрелость (по первоисточникам)

- Системный подход к валидации: машиночитаемый каталог (`catalog/`, `catalog.db` в релизах), WER-пайплайн, DER/JER для диаризации, FLEURS parity для langID, документированные процедуры портирования (docs/model-family-testing.md) — README.
- Известные проблемы из issue-трекера: CUDA медленнее Vulkan на некоторых конфигурациях (issue #124); баг стриминга Nemotron 3.5 ASR с «замерзанием» committed text (issue #215, есть PR #218 с фиксом); OpenVINO/NPU-путь экспериментален и ломается на speech-моделях (issue #242).

---

## 3. Сравнение как замены whisper.cpp для Samobranka (Tauri/Rust)

| Критерий | audio.cpp | transcribe.cpp |
|---|---|---|
| Интеграция с Rust | C ABI (Apache-2.0), готовые `lib`-архивы в релизах; FFI напрямую, Rust-биндингов нет (не подтверждено/нет) | Официальный Rust-биндинг (`bindings/rust/transcribe-cpp`), единый C header; MIT |
| CLI-вариант | `audiocpp_cli` (аср/диар/VAD и пр.) | `transcribe-cli` (только STT-задачи) |
| Server-вариант | `audiocpp_server` (HTTP + WebUI, SSE-батчи) | нет |
| Охват нужных задач | ASR + диаризация + VAD + align в одном движке | ASR + VAD + langID; диаризация только Sortformer |
| GPU | CUDA, Vulkan, HIP, Metal, WebGPU, SYCL | CUDA, Vulkan, HIP, Metal |
| Лицензия кода | Apache 2.0 | MIT |
| Зрелость | Очень быстрые релизы, 143 семейства, широкое комьюнити; проект молодой (с июня 2026) | Узкий фокус (STT), строгая верификация каждой модели, меньше поверхность; тоже молодой (с апреля 2026) |

Риски audio.cpp: большая и быстро меняющаяся кодовая база; формат GGUF-пакетов свой — привязка к их конвертерам/репозиториям; лицензия Apache 2.0 (для встраивания ок, но менее пермиссивна, чем MIT). Риски transcribe.cpp: нет Nemotron-3-Diarization (issue #170 открыт) — потребуется либо Sortformer, либо второй движок для диаризации.

---

## Выводы

### Таблица поддержки целевых GGUF-моделей

| Проект | GigaAM v3 e2e CTC (handy-computer/gigaam-v3-e2e-ctc-gguf) | Nemotron-3-Diarization (audio-cpp/Nemotron-3-Diarization-GGUF) |
|---|---|---|
| **audio.cpp** | ✅ Да (архитектура `gigaam_asr` + e2e_ctc; пример CLI, доки, F16/F32-пакет в audio-cpp/audio.cpp-gguf). ⚠️ Именно handy-computer-файл собран под transcribe.cpp — совместимость с audio.cpp **не подтверждена**; для audio.cpp берётся их собственный GGUF той же модели. | ✅ Да — это родной пакет проекта (доки, примеры CLI/server/streaming, валидация против NeMo). Q8 не parity-safe, рекомендован BF16. |
| **transcribe.cpp** | ✅ Да — это родной пакет проекта (`library_name: transcribe.cpp`, WER-валидация, пример `transcribe-cli`). | ❌ Нет. Модель отсутствует в каталоге; issue #170 «Add Nemotron 3 Diarization» открыт; GGUF упакован под audio.cpp. Альтернатива внутри проекта — Sortformer 4spk v2.1 / MOSS-Transcribe-Diarize. |

### Вердикт

- **audio.cpp: обе модели — «да»** (но для GigaAM использовать пакет из `audio-cpp/audio.cpp-gguf`, а не handy-computer). Подтверждение: примеры в репозитории + официальные GGUF-пакеты + документация по обеим семьям.
- **transcribe.cpp: GigaAM — «да», Nemotron-3-Diarization — «нет»**. Подтверждение: каталог моделей + открытый issue #170 + карточка модели.
- **Рекомендация для Samobranka:** если нужны именно эти две модели в одном движке — **audio.cpp** (семейства `gigaam_asr` + `nemotron_3_diar`, Apache-2.0, C ABI с готовыми lib-пакетами, CLI/HTTP-сервер для вызова из Tauri). Если приоритет — минимальная интеграция через официальный Rust-биндинг и MIT-лицензия, и диаризацию можно заменить на Sortformer — **transcribe.cpp** (GigaAM v3 e2e CTC работает из коробки), а Nemotron-3-Diarization ждать по issue #170 или запускать отдельно.

### Не подтверждено первоисточниками

- Перекрёстная совместимость handy-computer GGUF (GigaAM) с audio.cpp.
- Поддержка GigaAM v2 (enc/dec/phone-варианты) в обоих проектах — в списках только v3 и Multilingual.
- Наличие Rust-биндингов у audio.cpp (в релизах — C API libs, биндингов в README не заявлено).
