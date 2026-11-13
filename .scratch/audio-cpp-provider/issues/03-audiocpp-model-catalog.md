# 03: Каталог и скачивание моделей audioCpp

**What to build:** Пользователь в настройках видит менеджер моделей аудио.cpp со списком поддерживаемых моделей: GigaAM v3 e2e CTC (для транскрибации) и Nemotron-3-Diarization BF16 (для диаризации). Модели скачиваются из HuggingFace через общий механизм из тикета 01 — с прогрессом, отменой и удалением; скачанный файл валидируется (magic GGUF). Источник весов — репозитории организации audio-cpp (для GigaAM НЕ используется пакет handy-computer, собранный под transcribe.cpp). Метаданные каталога хранят семейство и задачу каждой модели. Транскрибация ещё недоступна — только управление моделями.

**Blocked by:** 01 (Prefactor — общий механизм скачивания моделей)

**Status:** resolved

- [x] В настройках отображается менеджер моделей audioCpp с описанием каждой модели (назначение, размер)
- [x] GigaAM v3 e2e CTC скачивается из audio-cpp/audio.cpp-gguf с прогрессом и отменой
- [x] Nemotron-3-Diarization BF16 скачивается из audio-cpp/Nemotron-3-Diarization-GGUF
- [x] Невалидный/битый файл отклоняется валидацией, пользователь видит понятную ошибку
- [x] Скачанные модели можно удалить; состояние (скачана/нет) переживает перезапуск
- [x] Каталог моделей содержит метаданные семейства и задачи, независимые от конкретного движка

## Answer

Что построено (ветка `audio-cpp-03-model-catalog`, модуль `frontend/src-tauri/src/audio_cpp_engine/` + UI в `frontend/`):

- **`catalog.rs`** — статический каталог моделей, не знающий о движке: `ModelSpec` (stable `name`, `display_name`, `family`, `task`, `quantization`, `filename`, `size_mb`, точный `exact_bytes`, описание, pinned `source_url`) и сериализуемый `ModelInfo` для UI. `ModelTask::{Asr, Diar}` отдаёт строку задачи в терминах audio.cpp (`asr`/`diar`) и человекочитаемую метку. Каталог:
  - `gigaam-v3-e2e-ctc-f16` — family `gigaam_asr`, task `asr`, F16, 442 520 704 байт, из `audio-cpp/audio.cpp-gguf` (`GigaAM-ASR-GGUF/gigaam-v3-e2e-ctc-f16.gguf`, ревизия `73ce4162…`). Пакет handy-computer/transcribe.cpp не используется — это зафиксировано тестом на URL.
  - `nemotron-3-diarization-bf16` — family `nemotron_3_diar`, task `diar`, BF16, 198 720 928 байт, из `audio-cpp/Nemotron-3-Diarization-GGUF` (`nemotron-3-diarization-bf16.gguf`, ревизия `647d39fe…`). Q8_0 намеренно отсутствует (не parity-safe).
- **`models.rs`** — `AudioCppModelManager`, владеющий только файлами: `discover_models` (статус читается с диска каждый раз: `Missing` / `Corrupted { file_size, expected_min_size }` при неверном размере или битом заголовке / `Available` при точном размере и валидном GGUF-заголовке; идущая загрузка перекрывает статус `Downloading { progress }`), `download_model` (через общий `crate::model_downloader::download_file` из тикета 01 — прогресс 0→100, кооперативная отмена), `cancel_download` (как у Whisper/Parakeet: `Cancelled`/`Pending` с 5-секундным ожиданием очистки), `delete_model` (удаление отсутствующего файла — не ошибка; удаление во время загрузки запрещено), `is_model_available`. Валидация скачанного файла — magic GGUF **плюс** точный размер из каталога (`validate_artifact`): обрезанный/подменённый файл отклоняется, частичный файл удаляется, состояние возвращается в `Missing`. Имя файла проверяется на выход из каталога моделей. Артефакты лежат в `<app_data>/models/audiocpp/`.
- **`commands.rs`** — Tauri-команды `audiocpp_model_init`, `audiocpp_get_available_models`, `audiocpp_has_available_models`, `audiocpp_get_models_directory`, `audiocpp_download_model` (события `audiocpp-model-download-progress|complete|error`), `audiocpp_cancel_download`, `audiocpp_delete_model`, `open_audiocpp_models_folder`. Менеджер инициализируется на старте приложения и идемпотентно из UI. Ни одна команда не загружает нативную библиотеку — управление моделями работает без `libaudiocpp`.
- **Frontend**: `src/lib/audiocpp.ts` (чистые типы, имена команд/событий, хелперы статусов — без импорта Tauri), `src/lib/audiocppClient.ts` (Tauri-реализация за интерфейсом `AudioCppModelClient`), `src/components/AudioCppModelManager.tsx` (список моделей с назначением, задачей, квантизацией и размером; Download / Cancel / Delete / Re-download; прогресс-бар с троттлингом; состояние переживает перезапуск, т.к. источник истины — диск). В `TranscriptSettings` каталог доступен в секции «audioCpp models (GigaAM + Nemotron)». Выбор провайдера `audioCpp` для транскрибации сознательно не добавлялся — это тикет 04.

Как проверено:

- TDD red→green по срезам (каталог → discovery/валидация → download/cancel/delete → команды → UI).
- Rust: `cargo test --lib` — **415 passed, 0 failed, 7 ignored** (в т.ч. 41 тест `audio_cpp_engine::`, из них 8 на каталог и 16 на менеджер моделей: точный размер, битый заголовок, обрезанная загрузка с очисткой, дубликат загрузки, отмена с удалением частичного файла, Pending-отмена без активной загрузки, запрет удаления во время загрузки, сохранение состояния при «перезапуске» (новый менеджер над тем же каталогом), защита от path traversal в имени файла).
- Живая проверка против HuggingFace (временные `#[ignore]`-тесты, затем удалены): обе закреплённые ревизии скачиваются и проходят валидацию — GigaAM 442 520 704 байта `Available`, Nemotron 198 720 928 байт `Available` (102 отчёта прогресса, последний 100).
- Frontend: `bun test` — новые 16 тестов (`tests/lib/audiocpp.test.ts` — имена команд/событий и хелперы статусов; `tests/components/audiocpp-model-manager.test.tsx` — список с задачей/размером, прогресс, ошибка→Retry, отмена, удаление, состояние после перезапуска, Corrupted→Delete/Re-download) зелёные. Полный набор репозитория — на уровне базовой линии: единственный провал `empty summary state > renders a persistent retryable error` существует и без этих изменений.
- `tsc --noEmit` по `frontend/` — чисто.
- Не проверено: живой прогон в Tauri-приложении (UI-путь проверен только компонентными тестами), реальное удаление/загрузка через GUI на Windows (пути и команды общие с Linux).
