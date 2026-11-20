# 06: Выбор режима вычислений «Авто / Vulkan / CPU» с фолбэком

**What to build:** Пользователь выбирает режим вычислений для движка Transcribe.cpp в настройках; выбор переживает перезапуск. «Vulkan» при недоступном драйвере/устройстве не ломает запись: движок сам переходит на CPU, пользователь видит предупреждение, а диагностика показывает фактический бэкенд. «CPU» работает без GPU-оборудования. Значение хранится отдельной колонкой в таблице настроек транскрибации и сохраняется тем же API, что провайдер/модель.

**Blocked by:** 02 (FFI-subset, ABI-гейт, диагностика), 04 (UI менеджера моделей), 05 (tracer bullet записи)

**Status:** resolved

- [x] Миграция БД добавляет колонку режима транскрибации (default 'auto') без изменения существующих значений
- [x] Контракт конфига и API сохранения пробрасывают режим; при отсутствии значения — 'auto'
- [x] UI-селектор Авто/Vulkan/CPU в настройках нового провайдера; сохранение без лишних полей
- [x] Выбор бэкенда доходит до нативного вызова: 'auto' — авто-политика устройств, 'vulkan'/'cpu' — точный выбор устройства нужного типа
- [x] Vulkan-запрос без доступного Vulkan: транскрибация продолжается на CPU, пользователю уходит предупреждение через существующий warning-канал, диагностика отражает фактический бэкенд
- [x] Юнит-тесты выбора бэкенда на fake-движке: маппинг настроек, фолбэк-путь с предупреждением
- [ ] Ручная проверка: принудительный Vulkan на машине с GPU даёт ускорение — ЧАСТИЧНО: forced-Vulkan smoke прогонялся на этой машине без GPU (честный фолбэк на CPU, backends ["vulkan","cpu"] из contract); факт ускорения на GPU-машине зафиксировать человеку: `cargo test --test transcribecpp_live_smoke -- --ignored --exact forced_vulkan_reports_the_actual_backend`

## Comments

- 2026-11-20 (реализация, ветка `feature/tcp-06-backend`, мерж `4b35405`): миграция `20261120000000_add_transcribe_backend.sql` (колонка `transcribeBackend TEXT NOT NULL DEFAULT 'auto'`); поле `transcribeBackend` в TranscriptSetting/TranscriptConfig/api_save/api_get (absent/invalid → 'auto' через ComputeMode::parse); UI-селектор в ветке transcribeCpp + i18n en/ru; режим читается в `get_or_init_transcribe_cpp_provider_with_model` (подпись диспатча не менялась), кэш-глобал хранит (provider, mode) — смена режима перезагружает провайдер; фолбэк Vulkan→CPU: `log::warn!` + `app.emit("transcription-warning", …)` (существующий канал).
- Важно (из transcribe.h): allowed-backend маска фиксируется для ПРОЦЕССА при первой регистрации модулей — добавлен `fix_process_backends` в backend.rs; смена режима внутри живого процесса деградирует через select_device, полный переход требует перезапуска (задокументировано).
- Тесты: `cargo test --lib` 788 green / 0 failed; миграционный тест (старые строки сохранены, default auto); fake-DeviceQuery фолбэк с предупреждением; roundtrip absent→auto; bun-тесты — совпадение с baseline, i18n parity/usage зелёные. Sidebar-save теперь тоже несёт transcribeBackend (иначе сбрасывался бы в 'auto').
