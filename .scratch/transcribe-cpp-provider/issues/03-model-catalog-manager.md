# 03: Каталог RU-моделей и менеджер моделей с загрузками

**What to build:** Пользователь (пока через команды, UI — в билете 04) получает список из 9 вариантов моделей transcribe.cpp с поддержкой русского языка — gigaam-v3-ctc, gigaam-v3-e2e-ctc, gigaam-v3-rnnt, gigaam-v3-e2e-rnnt, parakeet-tdt-0.6b-v3, parakeet-ultra, qwen3-asr-0.6b, whisper-large-v3-turbo, whisper-large-v3 — каждый с квантами (F16…Q4_K_M, дефолт Q8_0), размером и описанием. Модели скачиваются с Hugging Face по неизменяемым pinned-commit URL репозиториев handy-computer в отдельную поддиректорию каталога моделей приложения, с прогрессом, отменой и удалением. Проверка целостности: GGUF-магия + точный размер + SHA-256 из каталога. Всё это работает БЕЗ установленной нативной библиотеки (по образцу менеджера моделей audio.cpp).

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Каталог содержит 9 вариантов со всеми квантами, размерами, семействами и запиненными SHA-256 от Hugging Face (значения фиксируются из HF на этапе реализации)
- [x] Директория моделей — отдельная поддиректория в каталоге моделей приложения; каталог и команды инициализации зарегистрированы (set_models_directory + init в setup)
- [x] Классификация состояния с диска: Missing / Available / Corrupted (размер или SHA-256 не совпали) / Downloading / Error
- [x] Загрузка идёт через существующий общий downloader с прогрессом и кооперативной отменой; события прогресса/завершения/ошибки с собственными именами
- [x] Отмена загрузки оставляет корректное состояние (частичный файл не считается Available; повторная загрузка possible)
- [x] Защита от path-escape по имени файла каталога
- [x] Юнит-тесты без сети и без нативной библиотеки: классификация диска на временных файлах, валидация магии/размера/SHA-256, selection-функции (configured-wins с фолбэком; exact без фолбэка для batch)
- [x] Дефолтная модель провайдера — parakeet-ultra Q8_0 (константа в Rust-конфиге и в TS-дефолтах провайдеров)

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-03-catalog`, мерж `326ae37`): модуль `transcribe_cpp_engine` (catalog/models/commands/transcription). 45 записей (9 вариантов × кванты F16/Q8_0/Q6_K/Q5_K_M/Q4_K_M), размеры/SHA-256/pinned-commit из сверки с HF API (native-facts §4). Валидация: GGUF-магия + точный размер + SHA-256 (sha2). События `transcribecpp-model-download-{progress,complete,error}`, payload-форма как у audio.cpp. `cargo test transcribe_cpp` — 48 green. Модель по умолчанию `parakeet-ultra-Q8_0` в Rust `config.rs` и TS `modelDefaults.ts`.
- Примечание: обнаружен pre-existing flake в `audio_cpp_engine::wrapper::a_missing_variant_reports_every_searched_path` (~50% при параллельной мутации env-переменных, изолированно зелёный) — не регрессия этого билета.
