# Запуск и сборка Meetily на Linux

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

Но сначала должны быть выполнены шаги 1–3: системные пакеты, sidecar `llama-helper`,
нативная библиотека audio.cpp (нужна только для провайдера `audioCpp`).

---

## 1. Системные зависимости

Установить один раз:

```bash
sudo apt update
sudo apt install -y \
  build-essential cmake git pkg-config \
  libasound2-dev libpipewire-0.3-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev libssl-dev \
  libclang-dev clang
```

Что зачем:

| Пакет | Зачем |
| --- | --- |
| `cmake`, `build-essential` | сборка нативных C/C++ зависимостей (Whisper, llama.cpp, audio.cpp) |
| `pkg-config` | поиск системных библиотек |
| `libasound2-dev`, `libpipewire-0.3-dev` | захват звука (микрофон, системный звук) |
| `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev` | окно и webview Tauri |
| `libssl-dev` | HTTPS |
| `libclang-dev`, `clang` | `bindgen` при сборке `llama-helper` |

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

Скрипт `scripts/tauri-auto.js` определяет GPU и сам выбирает фичу
(`cuda` / `hipblas` / `vulkan` / `openblas` / CPU). На машине без GPU будет CPU-режим —
это нормально и работоспособно.

Форсировать конкретный режим:

```bash
TAURI_GPU_FEATURE=none pnpm run tauri:dev      # явный CPU
TAURI_GPU_FEATURE=vulkan pnpm run tauri:dev    # Vulkan
TAURI_GPU_FEATURE=cuda pnpm run tauri:dev      # CUDA (нужен CUDA toolkit)
```

> `TAURI_GPU_FEATURE=""` **не** выключает авто-детект: `tauri-auto.js` проверяет
> значение на truthy, поэтому пустая строка уходит в авто-определение. Для
> принудительного CPU используйте `none`.

Либо напрямую через CLI, без авто-детекта:

```bash
cd <repo>/frontend
./node_modules/.bin/tauri dev
```

Логи разработки: терминал + `logs-dev/` в каталоге данных приложения:
`${XDG_DATA_HOME:-$HOME/.local/share}/com.meetily.ai/logs-dev/`.

---

## 5. Провайдер audioCpp и нативная библиотека

Whisper и Parakeet работают без дополнительных действий. Провайдер **audioCpp**
требует нативную библиотеку `libaudiocpp.so` (релиз v0.9.5, ABI 0.3.0;
совместимость: major 0, minor ≥ 2).

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

Если библиотеки нет, приложение запустится, модели audioCpp скачаются, но
транскрибация вернёт понятную ошибку «library not found». Это не краш.

Веса моделей скачиваются в самом приложении:
**Settings → Transcript Model → провайдер `audioCpp`**:
- `GigaAM v3 e2e CTC` — транскрибация (русский);
- `Nemotron-3-Diarization BF16` — метки спикеров (опционально).

---

## 6. Сборка релиза

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

> **Важно:** `pnpm run tauri:build -- <args>` **не работает** для передачи флагов.
> `scripts/tauri-auto.js` читает только `process.argv[2]` (`dev`/`build`) и
> игнорирует остальные аргументы. Либо задавайте режим через `TAURI_GPU_FEATURE`,
> либо вызывайте `tauri` напрямую.

**Подпись кода на Linux не требуется** и не настраивается: `sign-windows.ps1`
вызывается только из Windows-конфигурации.

---

## 7. Частые проблемы

| Симптом | Причина и решение |
| --- | --- |
| `cargo: command not found` | не установлен Rust: https://rust-lang.org/tools/install |
| `Unable to find libclang` | нет `libclang-dev` / `clang` — шаг 1 |
| `failed to find a pre-opened...` / пустое окно | не хватает `libwebkit2gtk-4.1-dev`; проверить `pkg-config --exists webkit2gtk-4.1` |
| `Missing external binary ... llama-helper` | не выполнен шаг 3 (sidecar не застейджен) |
| Сборка падает на `alsa-sys` | нет `libasound2-dev` |
| Ошибка линковки Vulkan | задайте `VULKAN_SDK=/usr` и `BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu`, либо соберите CPU-режимом `TAURI_GPU_FEATURE=none` |
| audioCpp: «library not found» | не задан `AUDIOCPP_LIBRARY_PATH` и библиотеки нет рядом с exe — шаг 5 |
| Долгая первая сборка | нормально: компилируются Whisper, llama.cpp и весь Rust-граф |

---

## 8. Проверка без GUI

Полный `cargo test` и `bun test` не требуют окна:

```bash
cd <repo>/frontend/src-tauri
cargo test --lib

cd <repo>/frontend
./node_modules/.bin/tsc --noEmit
bun test
```

Известные предсуществующие падения frontend-тестов (не связаны с изменениями):
`empty summary state > renders a persistent retryable error` и межфайловые ошибки
изоляции моков `@tauri-apps/api`. Отдельные затронутые файлы тестов проходят.
