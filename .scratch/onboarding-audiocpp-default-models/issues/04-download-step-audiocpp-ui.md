# Шаг «Getting things ready»: карточки GigaAM + Nemotron, гейт Continue, retry

Status: resolved
Type: task
Blocked by: 03

## Контекст

Спека: `.scratch/onboarding-audiocpp-default-models/spec.md` (решения «Шаг Getting things ready», «Гейт Continue», «Размеры на карточках», «Повторная попытка»).
Код: `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx`.
Зависимости: API OnboardingContext — тикет 03; payload прогресса с MB — тикет 01.

## Задача

1. **Карточки**: вместо одной «Transcription Engine» (Parakeet) — три:
   - «Transcription Engine» — GigaAM v3 e2e CTC, размер из события/каталога (не `~670 MB` захардкожено);
   - «Speaker Diarization» — Nemotron-3 Diarization, фоновая (не блокирует Continue), иконка `Users` из lucide-react;
   - «Summary Engine» — без изменений (существующий поток summary).
2. **Гейт Continue**: `disabled`, пока не `transcriptionModelDownloaded`; в `handleContinue` верификация через `audiocpp_model_init` + `audiocpp_has_transcription_model` (вместо parakeet-команд). Провал диаризации/summary не блокирует — тост «Downloads will continue in the background» уже существует, оставить логику.
3. **Retry**: «Try Again» для транскрибации и диаризации — повторный `invoke('audiocpp_download_model', { modelName })` (retry-команды нет; скачивание и есть вход). Кнопку retry показывать для карточек транскрибации и диаризации; для summary — существующий `handleRetrySummaryDownload`.
4. **Прогресс**: карточки рисуют `downloadedMb`/`totalMb`/`speedMbps` из состояний контекста (тикет 03); начальный `totalMb` — из payload событий (тикет 01). Инфо-блок «You can continue while this finishes» — показывать, когда транскрибация скачана, а диаризация или summary ещё нет.
5. **Текст**: заголовок/описание шага без упоминания Parakeet; описание — «You can start using Samobranka after downloading the Transcription Engine.» остаётся.

## Тесты (TDD)

Чистые хелперы шага — в `frontend/src/lib/onboarding-audiocpp-status.ts` (если чего-то не хватает — дополнить; основа создана в тикете 03):
- разрешённость Continue: gigaam completed → enabled; gigaam error/cancelled → disabled; диаризация любая → не влияет;
- подписи размеров карточек из payload события;
- выбор retry-обработчика по карточке.

Тесты по образцу `frontend/tests/lib/onboarding-summary-model.test.mjs` (node .mjs с transpile TS). Если шаг компонентный — проверить существующие component-тесты (`frontend/tests/components/`) и следовать их раннеру; не добавлять новый фреймворк.

## Заметки

- События фильтруются по `modelName` — слушатели в контексте (тикет 03), шаг только читает состояния.
- Размеры моделей: GigaAM F16 и Nemotron BF16 — из `ModelSpec.size_mb`; фронтенд получает их через payload события (тикет 01), fallback — подождать первого события, «Waiting...» как сейчас.

## Result

Implemented on `feat/onboarding-audiocpp-default-models`; review fixes in `feat/onboarding-audiocpp-review-fixes`. Integration tip after review: `1e03c73`.
