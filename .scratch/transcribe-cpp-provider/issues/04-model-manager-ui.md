# 04: Выбор провайдера Transcribe.cpp и UI менеджера моделей

**What to build:** В настройках транскрибации пользователь выбирает провайдер «Transcribe.cpp». Под выбором появляется менеджер моделей нового движка: список 9 моделей с квантами, статусами, кнопками скачать/отменить/удалить/открыть папку, прогрессом загрузки и выбором активной модели (сохранение автоматически, по образцу менеджера audio.cpp). Выбор провайдера сохраняет конфиг с дефолтной моделью parakeet-ultra; опция авто-перевода для нового провайдера заблокирована с пояснением (как у audio.cpp).

**Blocked by:** 03 (Каталог RU-моделей и менеджер моделей с загрузками)

**Status:** resolved

- [x] Провайдер «Transcribe.cpp» доступен в селекторе настроек транскрибации; сохранение конфига использует провайдер-значение transcribeCpp
- [x] Trio фронтенда (типы+карта команд+клиентский интерфейс / Tauri-клиент / компонент менеджера) повторяет структуру audio.cpp-менеджера; клиент покрывается подменой invoke в тестах
- [x] Менеджер моделей работает при недоступной нативной библиотеке (список и загрузка доступны)
- [x] Union-тип провайдера расширен во всех потребителях: настройки транскрибации, выбор языка, импорт аудио, диалог ретранскрибации
- [x] TS-дефолты моделей содержат запись нового провайдера (keep-in-sync с Rust-конфигом)
- [x] Для transcribeCpp опция перевода отображается как недоступная с пользовательским пояснением
- [x] Компонентные/unit-тесты менеджера на фейковом клиенте: рендер статусов, выбор модели с автосохранением, обработка событий прогресса

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-04-ui`, мерж `2c106bf`): trio `src/lib/transcribeCpp.ts` / `transcribeCppClient.ts` / `components/TranscribeCppModelManager.tsx` (ASR-only — без task/diagnostics-сложности audio.cpp); ветка провайдера в TranscriptSettings; union расширен в LanguageSelection, useTranscriptionModels, ImportAudioDialog, RetranscribeDialog, TranscriptPanel, transcription-provider, transcription-model-readiness (PROVIDER_COMMANDS: `transcribecpp_model_init` / `transcribecpp_has_transcription_model` / `transcribecpp_get_available_models`). i18n en+ru: providerTranscribeCpp, models.transcribeCpp.*, languageSelection.{transcribeCppTitle,transcribeCppBody}, importAudio.language.transcribeCppUnsupported, retranscribe.languageTranscribeCppHint. Тесты: +38 новых, bun test — совпадение с baseline (9 pre-existing flakes), tsc чистый (кроме pre-existing TS7053 в тесте билета 01). DI-шов saveSelection для детерминизма автосохранения в тестах.
