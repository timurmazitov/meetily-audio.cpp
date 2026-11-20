# 01: Мягкая миграция на провайдер transcribeCpp

**What to build:** Пользователь с сохранёнными настройками на удалённых движках (провайдеры `localWhisper`, `parakeet`, `audioCpp`) после обновления получает рабочую транскрибацию: приложение трактует любой из этих провайдеров как `transcribeCpp` с моделью по умолчанию из каталога Transcribe.CPP. Запись встречи идёт через transcribe.cpp без каких-либо действий пользователя; старые записи и их транскрипты остаются читаемыми. Схема БД не меняется: legacy-колонки остаются, но не используются; файлы ранее скачанных моделей на диске не удаляются.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Конфигурация транскрибации с провайдером `localWhisper`, `parakeet` или `audioCpp` читается как `transcribeCpp` с дефолтной моделью Transcribe.CPP (тесты на seam хранилища настроек: раунд-трип + нормализация)
- [x] Сохранение конфигурации принимает только допустимый набор провайдеров (transcribeCpp и облачные); спец-обработка API-ключа для `localWhisper` выведена из локального пути (no-op)
- [x] Клиентские дефолты (контекст конфигурации, страница настроек, панели, страницы встреч) используют провайдер transcribeCpp и дефолтную модель Transcribe.CPP; поля движка Whisper убраны из клиентских дефолтов
- [x] Проверка готовности модели перед стартом записи и smoke-путь проходят для нормализованной конфигурации
- [x] Ни одна миграция схемы БД не добавлена; поведение старых встреч не изменилось

## Comments

- Ветка: `impl/tcp-only/01-soft-migration`, влита merge-коммитом `c0ee0a5` в `impl/transcribe-cpp-only-transcription`; конфликтов не было.
- Seam хранилища настроек: `get_transcript_config` нормализует сохранённые `localWhisper`/`parakeet`/`audioCpp` в `transcribeCpp` + `DEFAULT_TRANSCRIBE_CPP_MODEL` без перезаписи строк; `save_transcript_config` принимает только `transcribeCpp` + облачные, неизвестный провайдер — ошибка. Round-trip `whisperApiKey` для локальных — no-op (колонка осталась). Попутно исправлен баг: fallback-вставка ключа облачного провайдера сеяла row с провайдером `parakeet`.
- Rust-дефолты (`api_get_transcript_config`, `validate_transcription_model_ready`, `get_or_init_transcription_engine`, `database/commands.rs`) и клиентские дефолты (ConfigContext, settings page, Sidebar, SummaryModelSettings, meeting-details, useModelConfiguration, useRecordingStart, constants/modelDefaults) переведены на transcribeCpp; whisper/parakeet-дефолты удалены.
- Поле `whisperModel` в payload сохранено (требование контракта Tauri-команды, колонка NOT NULL) — пустой дефолт вместо `large-v3`.
- Тесты (сначала красные): 8 новых случаев в `setting.rs mod tests`, обновлён `migrations_tests.rs` (читается нормализованно, байты строки сохранены), новые `tests/lib/transcription-client-defaults.test.tsx`, `tests/hooks/recording-start-transcribe-cpp.test.tsx`.
- Проверка: `cargo test --lib` — 799 passed / 0 failed / 7 ignored; `bun test tests/` — 478 pass / 9 fail (только известный baseline); `npx tsc --noEmit` — только 1 предсуществующая ошибка.
- Не проверено: живой прогон `pnpm run tauri:dev` (запись с live-транскрибацией) — отнесено к тикету 07.
