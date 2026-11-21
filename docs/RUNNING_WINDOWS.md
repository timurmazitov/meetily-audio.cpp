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

Перед этим — шаги 1–5 (VS Build Tools, Rust, Node/pnpm, sidecars, стейджинг
нативных библиотек: transcribe.cpp — шаг 5a (единственный движок
транскрибации), audio.cpp — шаг 5 (диаризация)).

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

## 5. Нативная библиотека для диаризации (audio.cpp)

Библиотека audio.cpp нужна **диаризации** (модель Nemotron-3 Diarization):
`audiocpp.dll` + MSVC-рантайм в корне установки. Скрипт стейджинга сам
скачивает и хеш-проверяет архив:

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

---

## 5a. Нативная библиотека для transcribeCpp (обязательно для движка транскрибации)

Провайдер `transcribeCpp` (единственный движок транскрибации,
[ADR 0006](adr/0006-transcribe-cpp-as-fourth-engine.md), консолидация —
[ADR 0007](adr/0007-consolidation-on-transcribe-cpp.md)) требует нативную
библиотеку transcribe.cpp v0.4.2 с ABI-пином `version=0.4.2`,
`header_hash=f99ad4169374b8f3` (сверяется по `contract.json` рядом с
библиотекой до первого вызова; несовпадение — понятная ошибка несовместимого
ABI, не краш). Кроссплатформенный скрипт стейджинга сам скачивает lane
`windows-x86_64-cpu-vulkan` и хеш-проверяет архив:

```powershell
cd <repo>\frontend
pnpm run stage:transcribe
```

Проверка — в каталоге должны появиться:

```powershell
Get-ChildItem frontend\src-tauri\transcribecpp-runtime
# transcribe.dll
# ggml*.dll (модули бэкендов CPU/Vulkan)
# contract.json
# licenses\LICENSE
```

Повторный запуск пропускается по штамп-файлу; проверка целостности —
`pnpm run stage:transcribe -- --verify` (это же делает шаг
`node scripts/stage-transcribe-native.mjs --verify` в
`.github/workflows/build-windows.yml`). `tauri.windows.conf.json` маппит
`transcribecpp-runtime` в корень установки, поэтому установленное приложение
находит библиотеку в плоской раскладке рядом с `.exe`.

**Системные зависимости (в отличие от audio.cpp — не бандлятся).** Upstream-сборка
v0.4.2 исправляется `delvewheel repair --exclude vulkan-1.dll` и delvewheel никогда
не вендорит MSVC-рантайм, поэтому в архиве нет `msvcp140*`/`vcruntime140*`/`vcomp140`.
От целевой машины ожидаются:

- **Visual C++ 2015-2022 Redistributable** (`vcruntime140*`, `msvcp*`);
- **Vulkan-загрузчик `vulkan-1.dll`** — ставится с драйверами GPU.

Без рабочего Vulkan-драйвера движок автоматически падает на CPU с
пользовательским предупреждением — транскрибация продолжает работать, медленнее.
Маска бэкендов фиксируется библиотекой на процесс при первой регистрации: смена
режима «Режим вычислений» (Авто / Vulkan (ГПУ) / CPU) после первой загрузки движка
применяется только после перезапуска приложения.

Обнаружение библиотеки (`transcribe_cpp_engine/wrapper.rs`), по порядку:
`TRANSCRIBECPP_LIBRARY_PATH` (путь к `transcribe.dll`; битый путь — ошибка, не
тихий fallback) → `transcribe-libs\windows-x86_64-cpu-vulkan\` рядом с exe →
плоско рядом с exe (корень установки). Для **dev-сборки** (`pnpm run tauri:dev`
запускает exe из `target\debug\`) укажите переменную окружения:

```powershell
$env:TRANSCRIBECPP_LIBRARY_PATH = "<repo>\frontend\src-tauri\transcribecpp-runtime\transcribe.dll"
```

Диагностика — команда Tauri `transcribecpp_diagnostics` возвращает
`{ available, library_path, version, header_hash, backends, error }`: путь к
библиотеке, версию, ABI-хеш, бэкенды lane и текст ошибки при недоступности
(`available: false`; ошибка в данных, команда не падает). Модели (дефолт —
`parakeet-ultra` Q8_0) скачиваются в приложении, **Settings → Transcript Model →
провайдер `Transcribe.cpp`**, и менеджер моделей работает без нативной библиотеки.

---

## 6. Запуск в режиме разработки

Из **Developer PowerShell for VS 2022**:

```powershell
cd <repo>\frontend
pnpm run tauri:dev
```

Приложение компилируется без GPU-фич; режим вычисления transcribe.cpp
(Авто / Vulkan / CPU) выбирается в настройках и применяется в рантайме — шаг 5a.

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

Сборка варианта, близкого к релизному (переносимый x86-64-v2, без AVX-512):

```powershell
$env:CMAKE_PROJECT_INCLUDE = "$PWD\.github\force-portable-ggml.cmake"
$env:RUSTFLAGS = "-C target-cpu=x86-64-v2"

cd frontend
pnpm run tauri:build
```

Только установщик нужного типа — через CLI напрямую:

```powershell
cd frontend
.\node_modules\.bin\tauri build --bundles nsis
.\node_modules\.bin\tauri build --bundles msi
```

> **Замечание:** авто-детект GPU при сборке (`scripts/tauri-auto.js`) удалён
> вместе с компилируемыми GPU-фичами — `pnpm run tauri:dev` / `tauri:build`
> теперь простые обёртки над `tauri dev` / `tauri build` (флаги pnpm 9
> пропускает напрямую; при сомнениях вызывайте `tauri` из `node_modules\.bin`).

### Про подпись

- **Код-подпись (Authenticode)** — не нужна: без `DIGICERT_KEYPAIR_ALIAS`
  `sign-windows.ps1` выходит с кодом 0.
- **Подпись updater-артефактов** — нужна только релизному конвейеру:
  `bundle.createUpdaterArtifacts: true` и `plugins.updater` включены
  (автообновление с собственного GitHub-контура, см. Update 2026-11-17 в
  [ADR 0002](adr/0002-rebrand-to-samobranka.md)), поэтому Tauri генерирует `.sig`
  и для локальной сборки нужен `TAURI_SIGNING_PRIVATE_KEY` (minisign-ключ), либо
  отключение updater-артефактов локальной сборкой.

---

## 8. Установка и проверка

```powershell
# тихая установка NSIS
.\target\release\bundle\nsis\<name>_x64-setup.exe /S
```

Проверить на «чистой» машине (без установленного VC++ Redistributable):

1. установить приложение;
2. запустить, записать короткую встречу;
3. **Settings → Transcript Model**, скачать модель Transcribe.CPP (дефолт —
   `parakeet-ultra` Q8_0);
4. транскрибировать — должен появиться текст;
5. в секции «Speaker diarization» скачать `Nemotron-3-Diarization BF16` и
   перетранскрибировать — должны появиться метки `Speaker 1`, `Speaker 2`.

Установка должна лежать в `%LOCALAPPDATA%\<app>\` с `audiocpp.dll`,
`msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll` рядом с `.exe`, а также с
`transcribe.dll`, модулями `ggml*.dll`, `contract.json` и `licenses\` из
`transcribecpp-runtime` (шаг 5a).

> **Оговорка про «чистую» машину:** audio.cpp работает без VC++ Redistributable
> (рантайм забандлен), а transcribe.cpp — **нет**: ему нужны системные
> VC++ 2015-2022 Redistributable и `vulkan-1.dll` (шаг 5a). Проверка transcribeCpp
> на «чистой» машине без них ожидает ошибку загрузки библиотеки или ошибку вида
> «модуль `vcruntime140.dll` не найден» — это ожидаемое поведение, а не регрессия.

---

## 9. Частые проблемы

| Симптом | Причина и решение |
| --- | --- |
| `link.exe not found` / `cl.exe not found` | открыт обычный PowerShell — нужен **Developer PowerShell for VS 2022** |
| `error: linker 'link.exe' not found` | не установлен workload «Desktop development with C++» |
| `Missing external binary ... llama-helper` | не выполнен шаг 4 |
| `audiocpp.dll` не найден при запуске | не выполнен шаг 5 или скрипт упал на проверке SHA256 (диаризация деградирует в `no_runtime`, это не краш) |
| `transcribe.dll` не найден при запуске | не выполнен шаг 5a (`pnpm run stage:transcribe`) или не застейджен `transcribecpp-runtime` |
| transcribeCpp: ошибка несовместимого ABI | `contract.json` рядом с `transcribe.dll` не совпадает с пином v0.4.2 / `f99ad4169374b8f3` — перестейджерите рантайм (`pnpm run stage:transcribe`) или укажите библиотеку через `TRANSCRIBECPP_LIBRARY_PATH` |
| transcribeCpp: предупреждение про CPU | нет рабочего Vulkan-драйвера (нужен системный `vulkan-1.dll`) или не установлен VC++ Redistributable; фолбэк на CPU штатный, смена режима вычислений требует перезапуска |
| Требуется `TAURI_SIGNING_PRIVATE_KEY` | updater-артефакты включены (`createUpdaterArtifacts: true`): для локальной сборки передайте minisign-ключ в эту переменную или собирайте с `--config`, отключающим `createUpdaterArtifacts` |
| Приложение падает на чистой машине | не забандлен MSVC-рантайм — проверить содержимое `audiocpp-runtime` в установке |
| Долгая первая сборка | нормально: компилируются llama.cpp (sidecar) и весь Rust-граф |
| Транскрибация на CPU вместо GPU | нет рабочего Vulkan-драйвера (нужен системный `vulkan-1.dll` с драйверами GPU) — штатный фолбэк; проверьте режим «Режим вычислений» и перезапустите приложение после его смены |

---

## 10. Чем отличается CI-сборка

`.github/workflows/build-windows.yml` дополнительно делает то, чего нет в локальной
сборке:

- выставляет `RUSTFLAGS=-C target-cpu=x86-64-v2` и
  `CMAKE_PROJECT_INCLUDE=.github/force-portable-ggml.cmake` (отключает `GGML_NATIVE`,
  запрещает AVX-512 — переносимость sidecar llama-helper);
- проверяет переносимость через `.github/verify-portable-ggml.cjs`;
- стейджерит рантаймы audio.cpp (шаг 5) и transcribe.cpp (шаг 5a) с проверкой SHA256;
- подписывает код через DigiCert KeyLocker (Authenticode).

Vulkan SDK в этом workflow не ставится: единственный движок транскрибации
(transcribe.cpp) грузит prebuilt-рантайм в рантайме и использует системный
Vulkan-загрузчик с драйверами GPU.

Для локальной сборки всё это опционально; раздел 7 показывает, как повторить
переносимую конфигурацию, если нужен установщик «как в релизе».
