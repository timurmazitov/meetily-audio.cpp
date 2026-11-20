# 08: Документация, ADR и лицензии

**What to build:** Новый движок документирован так же тщательно, как audio.cpp: разработчик по BUILDING знает про стейджинг-команду и первый запуск; пользователь RUNNING_LINUX/RUNNING_WINDOWS понимает требования к Vulkan и диагностику. Зафиксирован ADR «transcribe.cpp как четвёртый движок» со ссылкой на ADR 0001 (прежнее возражение про Nemotron-диаризацию не применимо: диаризация остаётся на audio.cpp) и с обоснованием отказа от официальных Rust-биндингов (prebuilt-архив не является CMake install-префиксом). Отражены лицензии весов (MIT для gigaam, Apache-2.0 для parakeet/qwen/whisper-семейств). Зафиксирована таблица соответствий для будущего удаления Whisper и Parakeet.

**Blocked by:** 01, 05, 06, 07

**Status:** resolved

- [x] BUILDING: секция first-run sidecar setup дополнена стейджингом transcribe.cpp; заметки про Vulkan-требования актуальны
- [x] RUNNING_LINUX / RUNNING_WINDOWS: диагностика движка, переменная окружения для кастомного пути библиотеки, поведение фолбэка CPU
- [x] ADR принят: решение, альтернативы (crate-сборка из исходников, официальные биндинги, sidecar-процесс), последствия (0.x пин version+header_hash, стоимость апгрейда)
- [x] Лицензии весов добавлены в THIRD-PARTY/документацию распространения
- [x] Таблица соответствий «удаляемый движок → замена» (Whisper → whisper-large-v3-turbo/large-v3, Parakeet → parakeet-tdt-0.6b-v3/parakeet-ultra) помещена в spec-файл фичи как раздел для будущей задачи удаления
- [x] Ссылки в документации проверены, команды из документации исполняются

## Comments

## Comments

Билет 08 выполнен в worktree-ветке feature/tcp-08-docs. ADR: docs/adr/0006-transcribe-cpp-as-fourth-engine.md. THIRD-PARTY-файла в репозитории нет — таблицы лицензий весов помещены в ADR 0006 (по образцу пункта про лицензии в ADR 0001). Таблица соответствий для удаления Whisper/Parakeet — в spec.md (раздел «Removing Whisper and Parakeet — replacement mapping»). Проверено: относительные ссылки резолвятся, pnpm run stage:transcribe есть в frontend/package.json, шаги стейджинга/проверки есть в build.yml и build-windows.yml; env-имя TRANSCRIBECPP_LIBRARY_PATH, порядок поиска, поля диагностики и caveat про процессную фиксацию маски бэкендов сверены с кодом transcribe_cpp_engine/.
