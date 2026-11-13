# 01: Prefactor — общий механизм скачивания моделей

**What to build:** Из существующего bespoke-загрузчика whisper_engine извлекается переиспользуемый механизм скачивания моделей (прогресс, отмена, валидация скачанного файла), который не привязан к конкретному движку. Поведение существующих провайдеров Whisper и Parakeet не меняется — их загрузка продолжает работать как раньше. Новый механизм — фундамент для каталога моделей audioCpp (тикет 03).

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Существующие загрузки моделей Whisper и Parakeet работают без изменений в поведении (прогресс, отмена, ошибки сети)
- [x] Механизм скачивания доступен как переиспользуемый компонент, не знающий о конкретных движках
- [x] Валидация скачанного файла (magic bytes / целостность) вынесена в общий механизм
- [x] Существующие тесты загрузки проходят; новая логика покрыта тестами по образцу существующих

## Answer

Добавлен модуль `frontend/src-tauri/src/model_downloader.rs` — engine-agnostic механизм скачивания:

- `download_file(url, path, cancellation, progress_hook)` — HTTP-стриминг с `reqwest`, прогрессом 0..100 (троттлинг не чаще 1% или раз в 2 с), кооперативной отменой через `CancellationToken` и User-Agent `Meetily/<version>`.
- `validate_model_magic(path)` — проверка заголовка GGML/GGUF (все варианты версий/endianness), вынесена из `WhisperEngine::validate_model_file`.
- `validate_downloaded_model(path, min_size)` — magic + минимальный размер; заменил bespoke-логику в `finish_download`.
- `DownloadCancelled` / `is_download_cancelled` — общий тип ошибки отмены (Whisper ре-экспортирует их для обратной совместимости).

`whisper_engine` переведён на общий механизм; `download_model_with_owner` по-прежнему обновляет состояние моделей и вызывает пользовательский callback через хук. Поведение Whisper не изменилось: все 20 тестов `whisper_engine` (включая отмену, частичные загрузки, user-agent, undersized-файлы) зелёные.

Верификация: `cargo test --lib model_downloader` — 10/10 passed; `cargo test --lib whisper_engine` — 20/20 passed; `cargo test --lib parakeet_engine` — 10/10 passed (Parakeet не затрагивался, служит регрессионным контролем).
