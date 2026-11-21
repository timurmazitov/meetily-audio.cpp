# Запуск и сборка Samobranka на Linux

Практическая инструкция для локального запуска приложения из исходников на Linux
(Ubuntu/Debian). Проверено на Ubuntu 26.04, Rust 1.98, Node 26, pnpm 9.15.9.

> Это инструкция для разработчика: сборка из исходников. Готовых `.deb`/`.AppImage`
> она не заменяет.

---

## 0. Кратко

```bash
cd <repo>/frontend
pnpm install --frozen-lockfile
pnpm run tauri:dev          # запуск
pnpm run tauri:build        # сборка пакетов
```

Но сначала: системные пакеты (шаг 1), frontend-зависимости (шаг 2), sidecar
`llama-helper` (шаг 3) и стейджинг нативных библиотек: audio.cpp — для
диаризации (шаг 5), transcribe.cpp — для единственного движка транскрибации
(шаг 6).

---

## 1. Системные зависимости

Установить один раз:

```bash
sudo apt update
sudo apt install -y \
  build-essential cmake git pkg-config \
  libasound2-dev libpipewire-0.3-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev libssl-dev
```

(libclang из пакетов ставить не нужно — LLVM скачивается в `.tools/`, см. ниже в таблице)

Что зачем:

| Пакет | Зачем |
| --- | --- |
| `cmake`, `build-essential` | сборка нативных C/C++ зависимостей (llama.cpp для sidecar, обвязки `build.rs`) |
| `pkg-config` | поиск системных библиотек |
| `libasound2-dev`, `libpipewire-0.3-dev` | захват звука (микрофон, системный звук) |
| `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev` | окно и webview Tauri |
| `libssl-dev` | HTTPS |
| `libclang` (LLVM) | `bindgen` при сборке `llama-helper` — скачать LLVM в `.tools/`, см. [BUILDING.md](BUILDING.md#local-llvm-toolchain-tools) |

Инструменты JS/Rust:

```bash
npm install -g pnpm@9.15.9
rustup target add x86_64-unknown-linux-gnu   # обычно уже стоит
```

---

## 2. Frontend-зависимости

```bash
cd <repo>/frontend
pnpm install --frozen-lockfile
```

`--frozen-lockfile` обязателен: он держит `pnpm-lock.yaml` и установленный набор
зависимостей согласованными.

---

## 3. Sidecar `llama-helper` (обязательно)

Tauri-сборка ожидает готовый бинарник в `frontend/src-tauri/binaries/`. Установка
frontend-зависимостей его НЕ создаёт.

```bash
cd <repo>
cargo build -p llama-helper
rustc -vV | grep host        # -> host: x86_64-unknown-linux-gnu

mkdir -p frontend/src-tauri/binaries
cp target/debug/llama-helper \
   frontend/src-tauri/binaries/llama-helper-x86_64-unknown-linux-gnu
```

Для релизной сборки:

```bash
cargo build -p llama-helper --release
cp target/release/llama-helper \
   frontend/src-tauri/binaries/llama-helper-x86_64-unknown-linux-gnu
```

Суффикс файла — это `host` из `rustc -vV`, а не литерал.

Проверить, что оба sidecar'а на месте:

```bash
ls frontend/src-tauri/binaries/
# llama-helper-x86_64-unknown-linux-gnu
# ffmpeg-x86_64-unknown-linux-gnu   <- скачивается автоматически при первой сборке
```

`ffmpeg` подтягивает `build.rs` сам; если его нет — он скачается при сборке.

---

## 4. Запуск приложения

```bash
cd <repo>/frontend
pnpm run tauri:dev
```

Приложение собирается без GPU-фич: у единственного движка транскрибации
(transcribe.cpp) режим вычисления выбирается в настройках (Авто / Vulkan / CPU)
и применяется в рантайме — см. шаг 6. На машине без GPU всё работает в CPU-режиме.

Либо напрямую через CLI:

```bash
cd <repo>/frontend
./node_modules/.bin/tauri dev
```

Логи разработки: терминал + `logs-dev/` в каталоге данных приложения:
`${XDG_DATA_HOME:-$HOME/.local/share}/ai.samobranka.app/logs-dev/`.

---

## 5. Диаризация (audio.cpp) и нативная библиотека

Единственный движок транскрибации — transcribe.cpp (шаг 6). Библиотека
audio.cpp нужна только **диаризации** (модель Nemotron-3 Diarization) и
требуется `libaudiocpp.so` (релиз v0.9.5, ABI 0.3.0; совместимость: major 0,
minor ≥ 2).

Обнаружение библиотеки (`audio_cpp_engine/wrapper.rs`), по порядку:

1. переменная окружения `AUDIOCPP_LIBRARY_PATH`;
2. `audiocpp-libs/<cpu|vulkan>/` рядом с исполняемым файлом;
3. плоская раскладка — `libaudiocpp.so*` рядом с исполняемым файлом.

**Вариант А — переменная окружения (для разработки):**

```bash
# распакуйте из релиза audio.cpp v0.9.5 архив lib-ubuntu-x64-{cpu,vulkan}.tar.gz
export AUDIOCPP_LIBRARY_PATH=/абсолютный/путь/до/libaudiocpp.so
cd <repo>/frontend && pnpm run tauri:dev
```

**Вариант Б — рядом с бинарём dev-сборки:**

```bash
mkdir -p <repo>/target/debug/audiocpp-libs/cpu
cp /путь/до/libaudiocpp.so* <repo>/target/debug/audiocpp-libs/cpu/
cd <repo>/frontend && pnpm run tauri:dev
```

Если библиотеки нет, приложение запустится и транскрибация будет работать;
диаризация корректно деградирует до статуса `no_runtime` (встреча сохраняется
без меток спикеров). Это не краш.

Веса модели скачиваются в самом приложении:
**Settings → Transcript Model → секция «Speaker diarization»**:
- `Nemotron-3-Diarization BF16` — метки спикеров (опционально).

---

## 6. Движок transcribeCpp и нативная библиотека

Провайдер **transcribeCpp** (единственный движок транскрибации,
[ADR 0006](adr/0006-transcribe-cpp-as-fourth-engine.md), консолидация —
[ADR 0007](adr/0007-consolidation-on-transcribe-cpp.md))
требует нативную библиотеку transcribe.cpp v0.4.2 с ABI-пином `version=0.4.2`,
`header_hash=f99ad4169374b8f3` (сверяется по `contract.json` рядом с библиотекой
до первого вызова; несовпадение — понятная ошибка несовместимого ABI, не краш).

Стейджинг одной командой (загружает lane `linux-x86_64-cpu-vulkan` или
`linux-aarch64-cpu-vulkan` с проверкой SHA-256):

```bash
cd <repo>/frontend
pnpm run stage:transcribe
```

Это раскладывает `libtranscribe.so`, модули бэкендов ggml, `contract.json` и
`licenses/` в gitignored `frontend/src-tauri/transcribecpp-runtime/`. В debug-сборке
приложение само ищет библиотеку в этом каталоге — дополнительных действий не нужно.

Обнаружение библиотеки (`transcribe_cpp_engine/wrapper.rs`, `transcription.rs`), по порядку:

1. переменная окружения `TRANSCRIBECPP_LIBRARY_PATH` (путь к файлу
   `libtranscribe.so`; указывает на несуществующий файл — ошибка, а не тихий fallback);
2. `transcribe-libs/<lane>/` рядом с исполняемым файлом (lane вида
   `linux-x86_64-cpu-vulkan`);
3. плоская раскладка — `libtranscribe.so` рядом с исполняемым файлом;
4. (только debug-сборка Linux) каталог стейджинга
   `frontend/src-tauri/transcribecpp-runtime/`.

Вариант с переменной окружения (для разработки):

```bash
export TRANSCRIBECPP_LIBRARY_PATH=/абсолютный/путь/до/libtranscribe.so
cd <repo>/frontend && pnpm run tauri:dev
```

Если библиотеки нет, приложение запустится, модели transcribe.cpp скачаются
(менеджер моделей работает без нативной библиотеки), но транскрибация вернёт
понятную ошибку «library not found». Это не краш.

Веса моделей скачиваются в самом приложении:
**Settings → Transcript Model → провайдер `Transcribe.cpp`** — девять
RU-содержащих GGUF-моделей (семейства gigaam-v3, parakeet-tdt-0.6b-v3,
parakeet-ultra, qwen3-asr-0.6b, whisper-large-v3-turbo/large-v3); дефолт —
`parakeet-ultra` (Q8_0). Каталог и загрузки работают и без нативной библиотеки.

Режим вычислений выбирается в настройках («Режим вычислений»: Авто / Vulkan (ГПУ) /
CPU) и хранится в колонке `transcribeBackend`. Поведение:

- **Vulkan при недоступном драйвере** — автоматический фолбэк на CPU с
  пользовательским предупреждением (`transcription-warning`) и записью в логи;
  транскрипты не страдают, только медленнее.
- **Маска бэкендов фиксируется на процесс.** Библиотека transcribe.cpp фиксирует
  допустимый набор бэкендов при ПЕРВОЙ регистрации (`transcribe_init_backends_ex`:
  «call once, first»). Первая загрузка движка в запуске захватывает текущий режим;
  смена Авто/Vulkan/CPU после этого применится только после перезапуска приложения,
  а до перезапуска запрос вне зафиксированной маски деградирует как недоступный
  бэкенд (например, Vulkan при CPU-маске → CPU).

Диагностика движка — команда Tauri `transcribecpp_diagnostics` (её нет в UI менеджера
моделей; доступна из devtools webview или тестов) возвращает:
`{ available, library_path, version, header_hash, backends, error }` —
путь к загруженной библиотеке, версию, ABI-хеш из `contract.json`, список
бэкендов lane и текст ошибки, если библиотека не загрузилась
(`available: false`; ошибка возвращается в данных, а не падением команды).

Проверка Vulkan-ускорения без GUI — интеграционные тесты
`tests/transcribecpp_live_smoke.rs` (помечены `#[ignore]`, требуют застейджженную
библиотеку и рабочий Vulkan-драйвер).

---

## 7. Сборка релиза

```bash
cd <repo>/frontend
pnpm run tauri:build
```

Результат:

```
<repo>/target/release/bundle/deb/*.deb
<repo>/target/release/bundle/appimage/*.AppImage
```

(при кросс-таргетных сборках путь включает трипл:
`target/x86_64-unknown-linux-gnu/release/bundle/...`).

Только нужные форматы — через CLI напрямую:

```bash
cd <repo>/frontend
./node_modules/.bin/tauri build --bundles deb,appimage
```

> **Замечание:** авто-детект GPU при сборке (`scripts/tauri-auto.js`) удалён
> вместе с компилируемыми GPU-фичами — `pnpm run tauri:dev` / `tauri:build`
> теперь простые обёртки над `tauri dev` / `tauri build`. Флаги можно передавать
> напрямую (`pnpm run tauri:build --bundles deb,appimage` на pnpm 9) или вызывать
> `tauri` из `node_modules/.bin`, как выше.

**Подпись кода на Linux не требуется** и не настраивается: `sign-windows.ps1`
вызывается только из Windows-конфигурации.

---

## 8. Частые проблемы

| Симптом | Причина и решение |
| --- | --- |
| `cargo: command not found` | не установлен Rust: https://rust-lang.org/tools/install |
| `Unable to find libclang` | LLVM не скачан в `.tools/` или не заданы `LLVM_CONFIG_PATH`/`LIBCLANG_PATH` — см. [BUILDING.md](BUILDING.md#local-llvm-toolchain-tools) |
| `failed to find a pre-opened...` / пустое окно | не хватает `libwebkit2gtk-4.1-dev`; проверить `pkg-config --exists webkit2gtk-4.1` |
| `Missing external binary ... llama-helper` | не выполнен шаг 3 (sidecar не застейджен) |
| Сборка падает на `alsa-sys` | нет `libasound2-dev` |
| Диаризация: `no_runtime` | не задан `AUDIOCPP_LIBRARY_PATH` и библиотеки нет рядом с exe — шаг 5 |
| transcribeCpp: «library not found» | не выполнен `pnpm run stage:transcribe` (шаг 6) и библиотека не найдена по порядку поиска |
| transcribeCpp: ошибка несовместимого ABI | `contract.json` рядом с библиотекой не совпадает с пином v0.4.2 / `f99ad4169374b8f3` — застейджерите рантайм заново (`pnpm run stage:transcribe`) или укажите корректную библиотеку через `TRANSCRIBECPP_LIBRARY_PATH` |
| transcribeCpp: предупреждение «…continues on the CPU backend» | Vulkan-драйвер недоступен — это штатный фолбэк на CPU; для Vulkan проверьте `vulkaninfo` и перезапустите приложение после смены режима |
| Долгая первая сборка | нормально: компилируются llama.cpp (sidecar) и весь Rust-граф |

---

## 9. Проверка без GUI

Полный `cargo test` и `bun test` не требуют окна:

```bash
cd <repo>/frontend/src-tauri
cargo test --lib

cd <repo>/frontend
./node_modules/.bin/tsc --noEmit
bun test
```

Известные предсуществующие падения frontend-тестов (не связаны с изменениями,
проверено на ветке консолидации: 9 падений): suites в
`tests/hooks/meeting-rename-export.test.tsx` и тесты языка/заголовка окна.
Отдельные затронутые файлы тестов проходят.
