# 02: Экран настроек — только Transcribe.CPP

**What to build:** В настройках транскрибации больше нет выбора движка: пользователь видит сразу таблицу моделей Transcribe.CPP с переключателем режима вычисления (auto/vulkan/cpu) и неизменную секцию диаризации (только модель Nemotron). Из интерфейса полностью исчезают: менеджеры моделей Whisper и Parakeet, опции `localWhisper`/`parakeet`/`audioCpp` в каком-либо выборе движка, функция перевода транскрипта (auto-translate) и связанные правила выбора языка (язык — автоопределение в объёме, который реально поддерживает transcribe.cpp), слушатели parakeet-событий загрузки и соответствующие ключи i18n. Диалоги ретранскрибации и импорта аудио, онбординг и проверка «есть ли модель» в трее предлагают только Transcribe.CPP. Rust-команды удаляемых движков ещё существуют, но из UI недостижимы.

**Blocked by:** 01 (Мягкая миграция на провайдер transcribeCpp)

**Status:** resolved

- [x] Селектор движка транскрибации отсутствует; видны таблица моделей Transcribe.CPP и режим вычисления (обновлённый компонентный тест секции настроек)
- [x] Секция диаризации показывает только модель диаризации audio.cpp; ASR-блок GigaAM отсутствует (обновлённый тест секции диаризации)
- [x] В диалогах ретранскрибации и импорта аудио выбор модели — только таблица Transcribe.CPP
- [x] Онбординг предлагает скачивание модели Transcribe.CPP (обновлённый тест шага онбординга)
- [x] Удалены клиентские модули/компоненты Whisper и Parakeet, их вызовы Tauri и события; тип-объединение провайдера локальной транскрибации сведён к transcribeCpp; функции выбора модели при смене движка и правил перевода удалены
- [x] Перевод недоступен нигде в UI; выбор языка соответствует возможностям transcribe.cpp; тест провайдера языка обновлён
- [x] Ключи i18n моделей Whisper/Parakeet и связанные строки выбора языка удалены; тест i18n-покрытия зелёный
- [x] Все тесты фронтенда зелёные

## Comments

- Ветка: `impl/tcp-only/02-settings-ui`, влита merge-коммитом `f3c7265` в `impl/transcribe-cpp-only-transcription`; конфликтов не было. 43 файла, −4085 строк.
- TranscriptSettings: селектор движка и ветки Whisper/Parakeet/audioCpp-ASR удалены; таблица Transcribe.CPP + режим вычисления (auto/vulkan/cpu) рендерятся сразу. Секция диаризации — только Nemotron из audio.cpp.
- audio.cpp на фронте приведён к `AudioCppModelTask = 'diar'`; менеджер фильтрует `task === 'diar'` и игнорирует чужие (GigaAM) события до их удаления из Rust-каталога (тикеты 03+).
- Диалоги ретранскрибации/импорта: только модели Transcribe.CPP, всегда `language: null` (автоопределение); `constants/languages.ts` удалён; `useTranscriptionModels` читает только `transcribecpp_get_available_models`.
- LanguageSelection: `auto-translate` и машинерия перевода удалены; устаревшее сохранённое предпочтение `auto-translate` откатывается к автоопределению.
- Онбординг: карточка транскрибации отслеживает каталожную дефолтную модель Transcribe.CPP через `transcribecpp-model-download-*`; гейт — `transcribecpp_has_transcription_model`; карточка диаризации осталась audio.cpp (Rust-конфиг онбординга переведён тикетом 03).
- Удалены `whisper.ts`, `parakeet.ts`, оба менеджера моделей, `ModelDownloadProgress`, parakeet/whisper-слушатели в `DownloadProgressToast`/`useModalState`; мёртвые i18n-ключи убраны из en+ru (parity/usage зелёные). `whisperModel` в save-payload сохранён (контракт Tauri, колонка NOT NULL).
- Проверка (сверено оркестратором на merge-коммите): `bun test tests/` — 9 fail, ровно baseline main (6 meeting-rename-export, 2 language/window-title, 1 order-dependent flake — не относятся к спеке); новых падений нет. `npx tsc --noEmit` — 1 предсуществующая ошибка. Rust не менялся.
- Не проверено: живые экраны в Tauri-приложении — отнесено к тикету 07.
