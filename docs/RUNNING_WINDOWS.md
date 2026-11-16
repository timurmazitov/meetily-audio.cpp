# Запуск и сборка Samobranka на Windows

Практическая инструкция для локальной сборки установщика на Windows x64.

> Собирать нужно **на Windows**. Кросс-сборка MSI/NSIS с Linux не поддерживается
> (нужны MSVC, WiX, NSIS и подпись). В репозитории для этого есть готовый
> CI-workflow `.github/workflows/build-windows.yml`.

Целевой трипл: `x86_64-pc-windows-msvc`. Требуется AVX2-совместимый x64 CPU.

---

## 0. Кратко

```powershell
cd <repo>\frontend
pnpm install --frozen-lockfile
pnpm run tauri:build
```

Перед этим — шаги 1–5 (VS Build Tools, Rust, Node/pnpm, sidecars).

**Подпись не нужна для локальной сборки.** Скрипт `sign-windows.ps1` при
отсутствии `DIGICERT_KEYPAIR_ALIAS` просто пишет `Skipping signing` и завершается
успешно.

---

## 1. Visual Studio Build Tools (MSVC + Windows SDK)

Установить **Visual Studio 2022 Build Tools** с workload
**«Desktop development with C++»**. Это даёт `cl.exe`, `link.exe`, Windows SDK
(`kernel32.lib`, `user32.lib`, …).

Проверка — в **Developer PowerShell for VS 2022**:

```powershell
cl
link
```

Если `cl` не найден, откройте именно Developer PowerShell (обычный PowerShell не
подхватывает окружение MSVC).

CMake идёт в комплекте с Build Tools; отдельная установка не нужна (`cmake --version`).

---

## 2. Rust

```powershell
# установщик: https://rust-lang.org/tools/install
rustc -vV
rustup target list --installed
```

Должен быть `x86_64-pc-windows-msvc`. Если нет:

```powershell
rustup target add x86_64-pc-windows-msvc
```

---

## 3. Node.js и pnpm

```powershell
# Node.js LTS: https://nodejs.org/
npm install -g pnpm@9.15.9
pnpm --version      # 9.15.9
```

Зависимости frontend:

```powershell
cd <repo>\frontend
pnpm install --frozen-lockfile
```

---

## 4. Sidecar `llama-helper` (обязательно)

Tauri ожидает готовый `.exe` в `frontend\src-tauri\binaries\`. Установка
frontend-зависимостей его не создаёт.

```powershell
cd <repo>
cargo build -p llama-helper --release
rustc -vV | Select-String "host:"     # -> host: x86_64-pc-windows-msvc

New-Item -ItemType Directory -Force -Path frontend\src-tauri\binaries
Copy-Item target\release\llama-helper.exe `
          frontend\src-tauri\binaries\llama-helper-x86_64-pc-windows-msvc.exe
```

Суффикс — это `host` из `rustc -vV`, а не литерал.

`ffmpeg` скачивается автоматически (`build/ffmpeg.rs`) в
`binaries\ffmpeg-x86_64-pc-windows-msvc.exe` при первой сборке — заранее класть не нужно.

---

## 5. Нативные библиотеки для audioCpp (обязательно для провайдера)

Провайдер `audioCpp` требует `audiocpp.dll` + MSVC-рантайм в корне установки.
Скрипт стейджинга сам скачивает и хеш-проверяет архив:

```powershell
cd <repo>
powershell -ExecutionPolicy Bypass -File "frontend/src-tauri/scripts/stage-audiocpp-runtime.ps1" `
  -Version "v0.9.5" `
  -Archive "audio-v0.9.5-lib-windows-x64-vulkan.zip" `
  -Sha256 "33b16d1afa800c9f73bbcc3d230873296a51fee1dc79512afe0ad14c29fe2d62" `
  -Destination "frontend/src-tauri/audiocpp-runtime"
```

Проверка — в каталоге должны появиться:

```powershell
Get-ChildItem frontend\src-tauri\audiocpp-runtime
# audiocpp.dll
# msvcp140*.dll, vcruntime140*.dll, vcomp140.dll
# audio.cpp-LICENSE.txt
```

`tauri.windows.conf.json` уже маппит `audiocpp-runtime` в корень установки, так что
приложение находит `audiocpp.dll` рядом с `.exe`, а MSVC-рантайм подхватывается
Windows-загрузчиком — отдельный VC++ Redistributable не нужен.

**ONNX Runtime** (для Parakeet) стейджится автоматически в
`frontend\src-tauri\binaries\onnxruntime` из `build/onnxruntime.rs`.

---

## 6. Запуск в режиме разработки

Из **Developer PowerShell for VS 2022**:

```powershell
cd <repo>\frontend
pnpm run tauri:dev
```

`scripts/tauri-auto.js` сам определит GPU и выберет фичу. Форсировать режим:

```powershell
$env:TAURI_GPU_FEATURE="vulkan"; pnpm run tauri:dev
$env:TAURI_GPU_FEATURE="none";   pnpm run tauri:dev   # CPU-only
$env:TAURI_GPU_FEATURE="cuda";   pnpm run tauri:dev   # нужен CUDA toolkit
```

Логи разработки: терминал + `%APPDATA%\ai.samobranka.app\logs-dev\`.

---

## 7. Сборка установщика

```powershell
cd <repo>\frontend
pnpm run tauri:build
```

Результат:

```
<repo>\target\release\bundle\msi\*.msi
<repo>\target\release\bundle\nsis\*.exe
```

Сборка варианта, близкого к релизному (Vulkan + переносимый x86-64-v2, без AVX-512):

```powershell
$env:CMAKE_PROJECT_INCLUDE = "$PWD\.github\force-portable-ggml.cmake"
$env:RUSTFLAGS = "-C target-cpu=x86-64-v2"
$env:TAURI_GPU_FEATURE = "vulkan"

cd frontend
pnpm run tauri:build
```

Только установщик нужного типа — через CLI напрямую:

```powershell
cd frontend
.\node_modules\.bin\tauri build --bundles nsis
.\node_modules\.bin\tauri build --bundles msi
```

> **Важно:** `pnpm run tauri:build -- <args>` **не работает** для передачи флагов.
> `scripts/tauri-auto.js` читает только `process.argv[2]` (`dev`/`build`) и
> игнорирует остальные аргументы. Режим GPU задавайте через `TAURI_GPU_FEATURE`,
> а флаги CLI передавайте, вызывая `tauri` напрямую.

### Про подпись

- **Код-подпись (Authenticode)** — не нужна: без `DIGICERT_KEYPAIR_ALIAS`
  `sign-windows.ps1` выходит с кодом 0.
- **Подпись updater-артефактов** — в `tauri.conf.json` включено
  `bundle.createUpdaterArtifacts: true`, поэтому при сборке Tauri также хочет
  сгенерировать `.sig` и может потребовать приватный ключ updater'а. Для локальной
  сборки без неё проще всего отключить артефакты одноразовым конфигом:

  ```powershell
  cd frontend
  '{ "bundle": { "createUpdaterArtifacts": false } }' | Set-Content ..\updater-off.json
  .\node_modules\.bin\tauri build --config ..\updater-off.json
  ```

  Флаг `--config` тоже нужно передавать напрямую в `tauri`, а не через
  `pnpm run tauri:build` (см. примечание в разделе 7).

  В CI ключ передаётся через `TAURI_SIGNING_PRIVATE_KEY` /
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; вам, по вашим словам, подпись не нужна.

---

## 8. Установка и проверка

```powershell
# тихая установка NSIS
.\target\release\bundle\nsis\<name>_x64-setup.exe /S
```

Проверить на «чистой» машине (без установленного VC++ Redistributable):

1. установить приложение;
2. запустить, записать короткую встречу;
3. **Settings → Transcript Model → audioCpp**, скачать `GigaAM v3 e2e CTC`;
4. транскрибировать — должен появиться текст;
5. скачать `Nemotron-3-Diarization BF16` и перетранскрибировать — должны появиться
   метки `Speaker 1`, `Speaker 2`.

Установка должна лежать в `%LOCALAPPDATA%\<app>\` с `audiocpp.dll`,
`msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll` рядом с `.exe`.

---

## 9. Частые проблемы

| Симптом | Причина и решение |
| --- | --- |
| `link.exe not found` / `cl.exe not found` | открыт обычный PowerShell — нужен **Developer PowerShell for VS 2022** |
| `error: linker 'link.exe' not found` | не установлен workload «Desktop development with C++» |
| `Missing external binary ... llama-helper` | не выполнен шаг 4 |
| Сборка `whisper-rs-sys` падает на CMake | нет CMake/Windows SDK — переустановить Build Tools с C++ workload |
| `audiocpp.dll` не найден при запуске | не выполнен шаг 5 или скрипт упал на проверке SHA256 |
| Требуется `TAURI_SIGNING_PRIVATE_KEY` | `createUpdaterArtifacts: true` — см. раздел 7, отключить одноразовым конфигом |
| Приложение падает на чистой машине | не забандлен MSVC-рантайм — проверить содержимое `audiocpp-runtime` в установке |
| Долгая первая сборка | нормально: компилируются Whisper, llama.cpp, ONNX-обвязка и весь Rust-граф |
| GPU не используется | нет `vulkaninfo` / `VULKAN_SDK` — приложение работает на CPU |

---

## 10. Чем отличается CI-сборка

`.github/workflows/build-windows.yml` дополнительно делает то, чего нет в локальной
сборке:

- ставит Vulkan SDK 1.4.309.0 и копирует `vulkan-1.dll` в `vulkan-runtime`;
- выставляет `RUSTFLAGS=-C target-cpu=x86-64-v2` и
  `CMAKE_PROJECT_INCLUDE=.github/force-portable-ggml.cmake` (отключает `GGML_NATIVE`,
  запрещает AVX-512);
- проверяет переносимость через `.github/verify-portable-ggml.cjs`;
- подписывает код через DigiCert KeyLocker и генерирует updater-подписи.

Для локальной сборки всё это опционально; раздел 7 показывает, как повторить
переносимую конфигурацию, если нужен установщик «как в релизе».
