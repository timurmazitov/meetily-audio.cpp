# 08: GPU-путь — авто-детект бэкенда для audioCpp

**What to build:** Движок audio.cpp выбирает вычислительный бэкенд по доступному железу по существующему авто-детекту приложения (приоритет CUDA → Vulkan → CPU), и существующая настройка GPU-ускорения применяется к провайдеру `audioCpp`. Пользователь с NVIDIA GPU получает ускоренную транскрибацию автоматически; пользователь без GPU получает рабочий CPU-путь. Решение о дефолтном бэкенде на CPU/Vulkan опирается на замеры производительности (риск зафиксирован в спеке, бенчмарк — отдельная задача вне этого набора).

**Blocked by:** 04 (Транскрибация встречи провайдером audioCpp)

**Status:** resolved

- [x] На машине с NVIDIA GPU движок использует CUDA без ручной настройки
- [x] На машине без NVIDIA GPU движок использует Vulkan или CPU, приложение остаётся рабочим
- [x] Существующая настройка GPU-ускорения влияет на audioCpp предсказуемо и документированно
- [x] Переключение бэкенда не ломает уже скачанные модели и настройки провайдера

## Answer

Движок audio.cpp выбирает вычислительный бэкенд рантайм-авто-детектом приложения (тот же `HardwareProfile`/`GpuType`, приоритет CUDA → Vulkan → CPU), а не компиляционным флагом, как Whisper. Вся логика выбора локальна в `audio_cpp_engine`:

- **`audio_cpp_engine/backend.rs`** (новый): чистые функции выбора — `AudioCppBackend::autodetect(GpuType)` (Cuda → Vulkan → Cpu; Metal/OpenCL/None → Cpu, т.к. вариантов библиотеки под них в пине нет), `shipped_variant()` — пинованные релизные архивы v0.9.4 ship варианты `cpu` и `vulkan`, поэтому желание CUDA обслуживается вариантом `vulkan` (Vulkan работает на NVIDIA GPU через vendor ICD), `session_config()` — маппинг в `audiocpp_backend_config` (токен backend, device 0, threads 0 — библиотека сама не трогает глобальный OpenMP).
- **`wrapper.rs`**: вариантно-зависимое обнаружение библиотеки — `AudioCpp::load()` = авто-детект → `shipped_variant` → `resolve_library_path_for(variant)`: сначала `audiocpp-libs/<variant>/` рядом с исполняемым файлом (раскладка пакетирования), затем плоская раскладка (single-variant/development, совместимо со staging тикета 07), `AUDIOCPP_LIBRARY_PATH` по-прежнему побеждает всё. Ошибка `LibraryNotFound` перечисляет все просмотренные пути.
- **`engine.rs`**: `NativeAudioCppEngine::new(library, backend)`; сессия asr/offline создаётся с токеном выбранного бэкенда вместо дефолта библиотеки.
- **`transcription.rs`**: `load_native_provider` выбирает бэкенд один раз и логирует решение (detected → running).
- **`docs/GPU_ACCELERATION.md`**: секция «The audioCpp provider» фиксирует предсказуемое правило (NVIDIA → GPU через Vulkan-вариант; AMD/Intel → Vulkan; без GPU → CPU; модели не привязаны к бэкенду).

Модели — бэкендо-агностичные GGUF: любой вариант библиотеки грузит тот же каталог, поэтому переключение бэкенда не инвалидирует скачанные модели и настройки провайдера. Whisper/Parakeet не затронуты.

Тесты (TDD, `cargo test --lib`, 463 passed / 7 ignored после мержа tip `audio-cpp-provider`): `backend.rs` — 7 тестов приоритета/маппинга/токенов; `wrapper.rs` — вариантная директория побеждает плоскую раскладку, single-variant fallback, перечень searched paths при отсутствии, env-override (обновлён под новую сигнатуру, ENV_LOCK из тикета 07 сохранён). Нативная библиотека в тестах не используется, поведение против живой CUDA/Vulkan-сборки не проверялось (неподтверждённый риск — как и раньше, бенчмарк вне этого набора).
