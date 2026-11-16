# Upstream Meetily v0.5.0: что изменилось относительно v0.4.1

Источники: клон `/tmp/opencode/meetily-upstream` (теги `v0.4.1`, `v0.5.0`), release notes
<https://github.com/Zackriya-Solutions/meetily/releases/tag/v0.5.0>, PR-описания через GitHub API.
Форк уже содержит merge-коммит `e991463` (PR #907).

## 1. Состав релиза

- `git rev-list --count v0.4.1..v0.5.0` = **84 коммита** (18 merge, 66 обычных), тег `9e50dc2`,
  2026-11-13 19:46 +0530. Префиксы: 40 `fix`, 13 `feat`, 4 `docs`, 3 `ci`, 2 `chore`.
- PR: #805, #755, #834, #822, #874, #893, #882, #905, #904, #909, #907 (+ пост-релизные #915, #916).
- Объём: 119 файлов, +8510/−2846.
- **Релиз про Linux-аудио и надёжность записи**: 44 из 84 коммитов упоминают
  linux/pulse/alsa/appimage/deb, 31 трогает `frontend/src-tauri/src/audio/`. Заголовок PR #907:
  «Release v0.5.0: Linux support and recording reliability».

## 2. Группы изменений

### 2.1 Захват и микширование (ядро)

Новые `audio/capture/pulse_linux.rs` (535 строк) и `pulse_enumerator.rs` (532); в
`frontend/src-tauri/Cargo.toml@v0.5.0` добавлен `libpulse-binding = "2.30.1"`.

- `e01b48e` — нативный system audio через PulseAudio/PipeWire, уход от `~/.asoundrc` с «monitor»
  (PR #822). `082560f` — реальный default-sink сервера вместо эвристики.
- `3f3e53a` — сериализация alsa-lib: `snd_device_name_hint`/`snd_config_update_r` мутируют
  process-global кэш, параллельный вызов портил heap → SIGABRT через 30–35 минут записи (два
  coredump'а). `57b3092`, `4e18765`, `b2e8846` — постоянный поток `pulse-enum` вместо
  открытия/закрытия соединений каждые 2 с.
- `6236e36`, `0a91e70`, `89cedd7`, `6088552` — `AudioDevice.{label,is_default}`, идентичность
  отделена от подписи, одинаковые описания получают номера (PR #874). `7f7168c`, `5033d5e` —
  резолв по точному ID (`find_sink_by_name`/`find_source_by_name`). `3755459` — backlog PipeWire:
  pending-события дренятся пачками, sleep только при простое.
- `70b829f` (последний перед тегом) — микшер терял ~10 % микрофона: отставший поток заливался
  тишиной через 100 мс wall-clock, сэмплы потом отбрасывались. Теперь тишина только при
  опережении на 1 с **по сэмплам**; при мёртвом устройстве микс идёт с задержкой 1 с.

### 2.2 Жизненный цикл записи

- `7005086`, `eea7212`, `8e16dc0` — неблокирующий mainloop, join воркеров на stop, отмена
  незавершённых стартов, follow OS default каждые 2 с, подготовка замены потока до переключения
  сэмплов, выравнивание по позиции сэмплов через разрывы.
- `491f6a1` — старт отклоняется, если микрофон не создался. `ff55ca1` — единственный владелец
  shutdown, вводит `StopOutcome { Stopped, AlreadyStopping, NotActive }` (`recording_commands.rs`).
  `fafb8f4` — постанализ после автостопа; миграция legacy-выборов по ролям.
- `ee4d607`, `8f62458` — unresolved legacy-устройство не блокирует Start на macOS/Windows (fallback
  на default с уведомлением); на Linux блокирует, т.к. legacy-ID не совпадут с Pulse-именами.
- `6870780`, `1e08344`, `a4dbeea` — уведомление при неудачном старте system audio; восстановление
  микрофона после потери порта; отказ от частичных Pulse-инвентарей.

### 2.3 Логирование (PR #755, merge `24f4533`)

- `beec9b8` — единый `tauri-plugin-log` вместо `env_logger`+`tracing`; удалены
  `audio/async_logger.rs` (131) и `audio/batch_processor.rs` (216). Dev → stdout +
  `<app-data>/logs-dev`, release → `<app-data>/logs`; 10 МБ/файл, ротация KeepSome(5|10);
  release-уровень `Warn` (Info для `app_lib`), включается `MEETILY_DEBUG_LOG=1` (`lib.rs`).
- `defefec`, `2451b9a`, `ee3c626` — редакция секретов (токены, URL-креды, тела ответов, текст
  транскрипта, stdout детей); новый `src/utils.rs` (`bounded_log_line` — 4096 символов,
  `log_snippet`, `url_origin_for_log`). Вместо per-chunk логов — `WorkerLogStats` в
  `audio/transcription/worker.rs`.

### 2.4 Движки транскрипции

- `d16c04f` (PR #805) — Parakeet v3 с `huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx`
  (пин `8f23f0c0…`) вместо self-hosted `meetily.towardsgeneralintelligence.com`;
  `user-agent: Meetily/<version>`. `PARAKEET_V3_ARTIFACTS` не менялся (файлы байт-идентичны).
- `parakeet_engine/` — только удаление trace/debug логов. `whisper_engine.rs` — −162 строки
  (удалено «умное» логирование); параметры инференса (`no_timestamps`, `no_speech_thold 0.55`,
  `max_len 200`) те же. `provider.rs`, `whisper_provider.rs`, `parakeet_provider.rs` **не
  менялись** — диф только в `worker.rs`.

### 2.5 Summary/LLM, БД, онбординг

- Summary: только `tracing` → `log` и тексты сообщений (`summary/{llm_client,processor,service,
  commands}.rs`). Провайдеры, ключи настроек, промпты не менялись (в диффе нет строк с
  `"openai"|"gemini"|"claude"|"groq"|"openrouter"|"custom_openai"|"ollama"`).
- БД: **схема не менялась** — набор `migrations/` идентичен (10 файлов, дифф имён пуст). В
  `database/repositories/*.rs` только `tracing` → `log` и сокращение текстов ошибок.
- Онбординг: `src/onboarding.rs` **не менялся**; `ModelStatus` по-прежнему содержит `parakeet`.

### 2.6 Frontend UI, CI, документация

- `doodle.rs` (новый, 505 строк) + `Logo.tsx` + `SidebarProvider.tsx`: сезонная иконка, качается
  только если на этапе компиляции задан `MEETILY_DOODLE_URL`. `doodle_sync` — единственная новая
  Tauri-команда релиза. `app/feedback/page.tsx` (новый); `FeedbackLink.tsx` заменяет удалённый
  `Info.tsx` (`9800a05`, `96015ea`, `72120bb`, `3db38ac`: минимум окна, `preventOverflow`,
  1100×700). `4a41233` — `open_external_url` принимает только `https` и открывает через
  `open::that` в `spawn_blocking`. Новый `src/lib/audioDevices.ts` (label-кэш,
  `resolveDeviceLabel`) + 5 новых тестовых файлов.
- `baa53ba`, `8b3ce1d`, `2ef0ee1`, `76ee68a`, `d04df2d`, `60ab9d6`, `d790d51`, `5779d66`,
  `14b57bc` — Linux `.deb`+AppImage в общем draft-релизе (Ubuntu 22.04, Vulkan SDK 1.4.363.0 с
  проверкой суммы), подпись пакетов, job `validate-release`, AppImage со статическим
  type2-runtime (без `libfuse2`). В `tauri.conf.json` — bundle-метаданные, стандартные иконки,
  `linux.deb.depends = ["libvulkan1","libpulse0","libasound2"]`; issue-шаблоны `.md` → `.yml`.
- `AGENTS.md` добавлен (77 строк), `CLAUDE.md` урезан с 410 строк до ссылки; обновлены
  `CONTRIBUTING.md`, `docs/architecture.md` (+35), `docs/BUILDING.md` (+61),
  `docs/building_in_linux.md` (+53).

## 3. Ломающие и миграционно-значимые изменения

1. **Миграций БД нет** — схема для перехода 0.4.1 → 0.5.0 не меняется.
2. **`stop_recording` сменил тип возврата**: `Result<(), String>` →
   `Result<StopOutcome, String>` со значениями `"stopped" | "already_stopping" | "not_active"`
   (`lib.rs@v0.5.0`, `recording_commands.rs@v0.5.0`, `services/recordingService.ts@v0.5.0`).
   Удалён ранний выход «already stopped → Ok(())».
3. **Новые события**: `recording-shutdown-progress` (`{stage, message, progress}`,
   `stage == "stopping_audio"`), `recording-stop-complete`, `linux-audio-notice`
   (`{error, message}`).
4. **Расширены payload'ы**: `mic-device-switched`, `mic-swap-failed`, `mic-recovery-exhausted`
   получили необязательный `device_label` — обратная совместимость сохранена.
5. **`AudioDevice` переработан** (`devices/configuration.rs@v0.5.0`): добавлены
   `label: Option<String>` (не сериализуется при `None`) и `is_default: bool` (всегда);
   `Deserialize` больше не выводится; `PartialEq`/`Hash` только по `name + device_type`; добавлены
   `device_from_id`, `with_label`, `with_default`.
6. **Семантика `name` на Linux** — точный Pulse source/sink без суффиксов `" (input)"`/
   `" (output)"`. Миграция в `audio/recording_preferences.rs`: новый
   `device_selection_version: [u8; 2]` (`0` = legacy, `1` = exact ID); неразрешимый legacy на
   Linux отклоняет старт.
7. **Настройки записи**: в сторе появился `device_selection_version`; TS-интерфейсы
   `RecordingPreferences` (`services/configService.ts`, `components/RecordingSettings.tsx`) теперь
   требуют поле — ломает моки/тесты, если не обновлены.
8. **`tauri.conf.json`**: `assetProtocol.scope` сужен `$APPDATA/**` → `$APPDATA/doodle/*`;
   `img-src` + `http://asset.localhost`; `preventOverflow = true`. Сужение scope ломает любой
   `convertFileSrc` вне `doodle/`.
9. **`open_external_url` отклоняет не-HTTPS** (`api/api.rs@v0.5.0`) — вызовы `http://`/`mailto:`
   сломаются.
10. **Cargo**: убраны `env_logger`, `tracing`, `tracing-subscriber`; добавлены
    `libpulse-binding 2.30.1`, `open 5.3.3`. Версии движков **не менялись**: `whisper-rs 0.13.2`,
    `ort 2.0.0-rc.10`, `tauri 2.11.1` (lock), `tauri-build 2.6.1` (lock).
11. **Логи**: release → `<app-data>/logs`, dev → `logs-dev`, `MEETILY_DEBUG_LOG` включает debug;
    старые логи не мигрируют. **Модели**: состав/имена файлов Parakeet v3 не менялись, только
    URL-источник; дефолтные модели онбординга не менялись. Linux: нужен `libpulse0`
    (+ `libvulkan1`, `libasound2` в DEB); ручной `~/.asoundrc` больше не нужен.

## 4. Коммиты, которых нет в форке

Уточнение: пять коммитов — **не «после тега», а между merge-коммитом форка `e991463` и тегом**
(`rev-list --count e991463..v0.5.0` = 5; `merge-base --is-ancestor <sha> v0.5.0` = YES для всех).

- `4cf4836` — `llama-helper` без OpenSSL (`CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=TRUE` в
  `frontend/build-gpu.{sh,bat}`, `dev-gpu.{sh,bat}`, workflows). macOS 26-раннеры перешли на
  Homebrew OpenSSL 4.1.0, а `llama-cpp-sys-2 0.1.146` не компилируется (`X509_get_subject_name()`
  теперь `const X509_NAME *`).
- `21de5b5` — пин `rust-toolchain@1.99.1` во всех build-workflows: Rust 1.100 стабилизировал новый
  build-dir layout, `tauri-build 2.6.1`/`ort-sys 2.0.0-rc.10` падают (Linux: `IsADirectory` на
  сайдкаре; Windows: path not found на `DirectML.dll`).
- `a70d6f0` — merge PR #915; `e03a15b` — `validate-release` на `ubuntu-24.04` (`minisign` есть
  только с Ubuntu 23.04); `9e50dc2` — merge PR #916 (сам тег).

После тега на `origin/main` ещё три: `72c1d13` (`scripts/install-linux.sh`, 304 строки,
`--version/--uninstall/--purge`), `56d0d81` (формулировки), `2166bc6` (README про Linux).

## 5. Влияние на форк

53 файла меняли обе стороны. Горячие точки:

- **`audio/recording_commands.rs`** — форк переписал 1080+ строк (audio.cpp, diarization, unload
  провайдера); upstream добавил `StopOutcome`, `StoppingGuard`, `IS_RECORDING_STOPPING`,
  `linux-audio-notice`. Основная ручная работа: обе стороны меняли функцию остановки.
- **`audio/pipeline.rs`** — у форка уже версия v0.5.0 (включая микшер `70b829f`); отличие —
  один комментарий («Samobranka Pro»). Риск минимален.
- **`audio/transcription/worker.rs`** — форк добавил audio.cpp-провайдер, upstream заменил логи на
  `WorkerLogStats` (~13 строк расхождения); статистика upstream не знает о новом вендоре.
- **Реестр провайдеров** — `audio_cpp_engine/` и `transcription/{provider,engine}.rs` upstream не
  трогал, коллизии нет; после слияния `worker.rs` проверить dispatch глазами.
- **Онбординг** — `onboarding.rs` upstream не менялся, но форк переписал `ModelStatus` под audio.cpp
  (`ONBOARDING_TRANSCRIPT_MODEL = "gigaam-v3-e2e-ctc-f16"`, `#[serde(default)]`); конфликтов быть
  не должно.
- **Ребрендинг** — механические конфликты: `Logo.tsx`, `doodle.rs` (`SAMOBRANKA_DOODLE_URL`),
  `lib.rs` (`samobranka_log_builder`, `SAMOBRANKA_DEBUG_LOG`), `parakeet_engine.rs`
  (`user-agent: Samobranka/<ver>`), `tauri.conf.json`, `About.tsx`, `Sidebar/index.tsx`.
- **Diarization** — у upstream в v0.5.0 нет diarization/sortformer (`grep -i 'diariz|sortformer'`
  пуст), коллизии нет.
- **CI/сборка** — форк не содержит `4cf4836` и `21de5b5`: в `build-gpu.sh`, `dev-gpu.sh`, `*.bat`
  нет `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL`, все workflows на `rust-toolchain@stable`, а
  `release.yml` держит `validate-release` на `ubuntu-22.04`. Практически значимые пропуски;
  нужность `install-linux.sh` для форка — не подтверждено.
