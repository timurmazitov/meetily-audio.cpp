# Map: audioCpp provider

## Decisions-so-far

- 07 windows-packaging-msvc-runtime — resolved: CI стейджит `audiocpp.dll` + MSVC-рантайм (`msvcp140*`/`vcruntime140*`/`vcomp140`) + Apache-2.0 лицензию из пинованного релиза v0.9.4 (`scripts/stage-audiocpp-runtime.ps1`, SHA256-пин) в `audiocpp-runtime/`, маппинг в корень установки через `tauri.windows.conf.json`; обнаружение рядом с exe из тикета 02 лейаут бандлера поддерживает без правок. Детали: `issues/07-windows-packaging-msvc-runtime.md` (Answer). Непроверено на живой Windows: сборка/установка установщика и запуск без VC++ Redist.
