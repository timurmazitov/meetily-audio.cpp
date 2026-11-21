# 06: Документация и глоссарий под единственный движок

**What to build:** Документация описывает итоговое состояние продукта: единственный локальный движок транскрибации transcribe.cpp плюс независимая диаризация моделью Nemotron из audio.cpp. README, архитектура (было четыре движка), руководство сборки и GPU (только рантайм-Vulkan transcribe.cpp вместо компилируемых GPU-сборок whisper-rs), руководство по запуску на Linux и глоссарий (термины «движок транскрибации»/«провайдер транскрибации» перечисляют только transcribeCpp) приведены в соответствие. ADR, описывающие Whisper/Parakeet и audio.cpp как движки транскрибации, помечены устаревшими полностью или в части ASR; добавлен ADR о консолидации транскрибации на transcribe.cpp. В репозитории не остаётся инструкций, команд и ссылок на удалённые движки и GPU-сборки.

**Blocked by:** 03 (audio.cpp — только диаризация), 04 (Удаление движка Parakeet), 05 (Удаление движка Whisper и компилируемых GPU-путей)

**Status:** resolved

- [x] README, архитектура, сборка, GPU, запуск на Linux описывают единственный движок + независимую диаризацию; команды и пути команд проверены
- [x] Глоссарий обновлён: движок/провайдер транскрибации без Whisper/Parakeet/audioCpp-ASR
- [x] ADR 0001 и 0006 получили статус-пометки; новый ADR о консолидации написан; ADR 0004 не изменён
- [x] Поиск по репозиторию не находит рабочих ссылок на удалённые движки вне исторических ADR и чейнджлога

## Comments

- Ветка: `impl/tcp-only/06-docs`, влита merge-коммитом `5070bc7` в `impl/transcribe-cpp-only-transcription` (21 файл).
- Обновлены: README, architecture, BUILDING (удален компилируемый GPU-раздел, ~290 строк), GPU_ACCELERATION (перезаписан: рантайм-режимы), RUNNING_LINUX/RUNNING_WINDOWS, building_in_linux, .github/workflows/ACCELERATION_GUIDE + README_DEVTEST + WORKFLOWS_OVERVIEW, AGENTS.md, frontend/README, GLOSSARY; ADR 0001/0006 — баннеры устаревания, новый ADR 0007 (консолидация), ADR 0004 не тронут (`git diff` пуст).
- Grep-аудит (whisper-rs/whisper_engine/parakeet/ONNX/dev-gpu/build-gpu/auto-detect-gpu/TAURI_GPU_FEATURE/coreml/hipblas/openblas): оставшиеся попадания легитимны — тексты/баннеры ADR, датированные research-заметки (получили исторические баннеры), имена GGUF-моделей (каталог transcribe.cpp), точный комментарий про транзитивный ONNX в Cargo.toml.
- Попутно исправлены ложные утверждения документации: «Vulkan SDK ставится build-windows.yml», устаревшие утверждения апдейтера (проверено по tauri.conf.json), атрибция portable-ggml hook (llama-helper), комментарий whisper-rs-sys в force-portable-ggml.cmake.
- Команды в документации сверены с frontend/package.json и workflow-файлами; линк-аудит изменённого markdown — 0 битых ссылок.
- Дожим: мёртвые шаги «Cache Windows ONNX Runtime» убраны из build.yml/build-windows.yml/build-devtest.yml в `401b5cc` (файлы вне scope тикета 06).
- Не проверено: GUI-уровневые утверждения документов проверялись по исходникам компонентов/i18n, не прогоном приложения.
