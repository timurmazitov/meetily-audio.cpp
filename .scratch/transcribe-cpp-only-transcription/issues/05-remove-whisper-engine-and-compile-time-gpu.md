# 05: Удаление движка Whisper (Rust) и компилируемых GPU-путей сборки

**What to build:** Движок Whisper полностью вырезан из нативной части: модуль движка (включая параллельный процессор и его команды), провайдер транскрибации, ветки в перечислении движка рантайма и батч-перечислении, инициализация при старте, дефолты и каталог моделей Whisper в конфигурации, переводческий путь провайдера (поддерживал только Whisper). Вместе с ним вырезаются компилируемые GPU-пути, существовавшие только ради него: зависимости whisper-rs во всех платформенных блоках, Cargo-фичи metal/coreml/cuda/vulkan/hipblas/openblas/openmp, проверки GPU-фич в сборочном скрипте, GPU-варианты dev/build-скриптов и соответствующие шаги CI (установка Vulkan SDK ради whisper-rs, компилируемые флаги). Рантайм-Vulkan-инфраструктура transcribe.cpp (staging рантайма, «дорожки» библиотек, Vulkan SDK в каталоге репозитория, режимы auto/vulkan/cpu) сохраняется без изменений. Фронтенд к этому моменту (тикет 02) к Whisper не обращается.

**Blocked by:** 02 (Экран настроек — только Transcribe.CPP)

**Status:** resolved

- [x] Модуль Whisper (с параллельным процессором), команды `whisper_*`, события и управляемое состояние удалены
- [x] Провайдер Whisper, его переводческий путь и ветки перечислений (движок рантайма, батч-движок), инициализация и выгрузка — удалены; дефолты конфигурации без Whisper
- [x] whisper-rs и все компилируемые GPU-фичи удалены из манифеста; сборочный скрипт без GPU-проверок фич
- [x] GPU-варианты dev/build-скриптов удалены; обычная сборка и `tauri:dev` работают; staging рантайма transcribe.cpp не затронут
- [x] CI-пайплайны собраны без whisper-специфичных GPU-шагов; шаги staging рантайма transcribe.cpp сохранены
- [x] `cargo check` и тесты Rust зелёные

## Comments

- Ветка: `impl/tcp-only/05-remove-whisper`, влита merge-коммитом `624a58d` в `impl/transcribe-cpp-only-transcription` (слиты обе удалённые ветки 04+05).
- Удалены: `whisper_engine/` (включая parallel_processor/parallel_commands/system_monitor/acceleration/_stderr_suppressor), `whisper_provider.rs`, ветки Whisper в engine/worker/import/retranscription/common/config/lib.rs; whisper-rs и компилируемые GPU-фичи (metal/coreml/cuda/vulkan/hipblas/openblas/openmp) из Cargo.toml; GPU-проверки в build.rs; GPU/dev-build скрипты (build-gpu.*, build.bat/ps1, dev-gpu.*, auto-detect-gpu.js, tauri-auto.js) и скрипты из package.json; whisper-GPU-шаги из CI (vulkan-sdk установка ради whisper-rs, компилируемые флаги) при сохранённых шагах `stage-transcribe-native.mjs` (build-windows.yml:210/215, комментарии про рантайм-Vulkan без вендоринга vulkan-1.dll).
- `keep in sync`: рантайм-Vulkan transcribe.cpp не тронут (режимы auto/vulkan/cpu, lanes, `.tools/`-инфраструктура).
- Проверено (2026-11-21, Linux, cargo 1.100): `cargo test --lib` — 729 passed / 0 failed / 7 ignored; live/batch smoke с staged-runtimes v0.4.2 — ok; `pnpm run tauri:dev` — запуск подтверждён (см. тикет 07).
- Не проверено: сборка Windows/macOS CI-пайплайнов (нет доступа к раннерам), CUDA/Metal-пути.
