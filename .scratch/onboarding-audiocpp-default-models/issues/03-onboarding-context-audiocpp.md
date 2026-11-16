# OnboardingContext: состояние и загрузки на audio.cpp вместо Parakeet

Status: ready-for-agent
Type: task
Blocked by: 01, 02

## Контекст

Спека: `.scratch/onboarding-audiocpp-default-models/spec.md` (решения «Состояние контекста», «Старт загрузок», «События прогресса», «Возобновление»).
Код: `frontend/src/contexts/OnboardingContext.tsx`.
Зависимости: форма payload события прогресса — тикет 01; схема `model_status` — тикет 02 (её TS-зеркало обновляется здесь).

## Задача

1. **Константы**: `PARAKEET_MODEL` заменить на `AUDIOCPP_TRANSCRIPTION_MODEL = 'gigaam-v3-e2e-ctc-f16'` и `AUDIOCPP_DIARIZATION_MODEL = 'nemotron-3-diarization-bf16'` (по образцу существующей константы).
2. **Состояние**: `parakeetDownloaded`/`parakeetProgress`/`parakeetProgressInfo` → `transcriptionModelDownloaded` + `transcriptionModelProgress(Info)` и отдельно `diarizationModelDownloaded` + `diarizationModelProgress(Info)`; сеттеры соответственно. `retryParakeetDownload` удалить (retry — повторный `audiocpp_download_model`, это тикет 04).
3. **Слушатели событий**: с `parakeet-model-download-*` на `audiocpp-model-download-progress|complete|error`; фильтрация по `modelName` — на две константы; payload прогресса включает `downloaded_mb`/`total_mb`/`speed_mbps` (тикет 01) — складывать в progressInfo.
4. **`startBackgroundDownloads`**: опции `includeParakeet` → `includeAudioCppTranscription` / `includeAudioCppDiarization` (summary-часть не менять). Транскрибацию стартовать через `audiocpp_download_model`, диаризацию — тоже `audiocpp_download_model` со вторым именем; перед стартом `invoke('audiocpp_model_init')`.
5. **Верификация/возобновление**: `verifyModelStatus` — вместо `parakeet_init`/`parakeet_has_available_models` использовать `audiocpp_model_init` + `audiocpp_has_transcription_model` (ASR-гейт; диаризацию проверять отдельно через `audiocpp_get_available_models` по имени nemotron, без жёсткого гейта). `checkActiveDownloads` — через `audiocpp_get_available_models`, детект по статусу `Downloading`.
6. **TS-зеркало схемы** (по тикету 02): интерфейс `OnboardingStatus.model_status` — `{ gigaam, diarization, summary, selected_summary_model? }`; `loadOnboardingStatus` читает новые поля.
7. **Сохранение**: `saveOnboardingStatus` пишет новые поля; `completeOnboarding` — выставлять обе модели downloaded, вызывать `audiocpp_model_init` + `audiocpp_has_transcription_model` вместо parakeet-проверок.

## Тесты (TDD)

Поведенческая логика контекста (разрешение статусов, детект Downloading) выносится в чистый хелпер `frontend/src/lib/onboarding-audiocpp-status.ts` (вход — сырые статусы/события, выход — состояния карточек). Тесты — по образцу `frontend/tests/lib/onboarding-summary-model.test.mjs` (node + typescript transpile, `node --test`/`node` прямым запуском .mjs). Покрыть: прогресс-событие по имени gigaam обновляет только транскрибацию; complete по nemotron — только диаризацию; чужое имя модели игнорируется; детект активной загрузки из списка моделей.

## Заметки

- Потребитель контекста (`DownloadProgressStep`) обновляется в тикете 04 — здесь сохранить API контекста согласованным со спекой (оба набора состояний прогресса доступны).
- Проверить тип `ModelInfo`/статуса для `audiocpp_get_available_models` на фронтенде (зеркало Rust `ModelInfo`): статус — строка либо объект (`{ Downloading: { progress } }` сериализуется как `{"Downloading":{"progress":N}}` в serde externally-tagged; сверить с `frontend/src/lib/audiocpp.ts`, там уже есть типы).
