# 03: Экспорт саммари в markdown через нативный диалог

**What to build:** На вкладке Summary карточки встречи рядом с Save и Copy появляется кнопка **Export** с иконкой скачивания (подпись скрывается на узких ширинах, как у соседних кнопок) и видна при тех же условиях: саммари есть и не генерируется.

Нажатие открывает системный диалог сохранения с предложенным именем `20261118-Планёрка_по_релизу-summary.md` и фильтром «Markdown Files». В файл пишется markdown-документ, идентичный тому, что кладёт в буфер **Copy**, с единственным отличием: строка метаданных звучит как `**Exported on:**`. Экспортируется текущее состояние редактора, включая несохранённые правки — файл равен тому, что пользователь видит на экране, никакого автосохранения не происходит.

После успешного сохранения полный путь к файлу копируется в буфер обмена и показывается тост `Summary exported` с путём — как у экспорта транскрипции. Отмена диалога бесшумна и буфер не трогает; ошибка диалога или записи даёт тост об ошибке, тоже не трогая буфер.

Нативный диалог перестаёт быть специфичным для транскрипции: команда принимает опциональные расширение и подпись фильтра, по умолчанию `txt` и `Text Files`, поэтому прежний вызов экспорта транскрипции работает без изменений. Расширение нормализуется и используется как в фильтре, так и для дозаписи выбранному пути, если пользователь стёр расширение в диалоге. Запись остаётся UTF-8 без BOM. Имя команды не меняется — контракт аргументов расширяется аддитивно.

Заодно в глоссарий добавляется термин «экспорт саммари» с той же формулировкой про идентичность содержимого копируемому тексту, что и у экспорта транскрипции, включая правило имени файла с датой.

**Blocked by:** 01 (единый сборщик имени файла), 02 (общий текст саммари)

**Status:** resolved

- [x] Кнопка Export видна на вкладке Summary при наличии сгенерированного саммари и отсутствии генерации
- [x] Диалог предлагает `<YYYYMMDD>-<название>-summary.md` и фильтр «Markdown Files»
- [x] Файл содержит шапку, метаданные со строкой `**Exported on:**`, разделитель и markdown саммари
- [x] Текст в файле совпадает с текстом кнопки Copy за вычетом слова в строке метки
- [x] Несохранённые правки редактора попадают в файл
- [x] Экспорт работает для саммари в форматах BlockNote-JSON, markdown и legacy
- [x] После сохранения путь файла оказывается в буфере обмена и показан тост `Summary exported`
- [x] Отмена диалога не показывает тост и не меняет буфер обмена
- [x] Ошибка сохраняется как тост об ошибке, буфер обмена не меняется
- [x] Экспорт транскрипции продолжает работать с фильтром «Text Files» и расширением `.txt`
- [x] Rust-юнит-тест покрывает дефолты, нормализацию расширения и дозапись расширения к пути без расширения
- [x] `bun test` покрывает аргументы команды для саммари и для транскрипции, попадание пути в буфер и отсутствие изменений буфера при отмене
- [ ] Ручная проверка в Tauri-приложении: сохранение, отмена, встреча с кириллическим и с пустым названием
- [x] В глоссарии появился термин «экспорт саммари» с правилом имени файла

## Comments

Реализовано в `11a520a` «Экспорт саммари: кнопка, диалог и глоссарий».

- **Rust** (`frontend/src-tauri/src/export.rs`): у `export_transcript_file_command` появились два опциональных
  параметра — `extension: Option<String>` и `filter_label: Option<String>`. Итоговая сигнатура:
  `pub async fn export_transcript_file_command<R: Runtime>(app: AppHandle<R>, content: String, default_file_name: String, extension: Option<String>, filter_label: Option<String>) -> Result<Option<String>, String>`.
  Нормализация вынесена в `resolve_export_params(...)` (trim, обрезка ведущих точек, фолбэк `txt` / `Text Files`),
  дозапись расширения — в `with_extension_if_missing(path, extension)`. Имя команды, регистрация в `lib.rs` и тип
  возврата не менялись; прежний двухключевой вызов продолжает работать, потому что Tauri заполняет отсутствующие
  `Option`-аргументы как `None` (см. `deserialize_option` в крейте `tauri`: при отсутствии ключа вызывается
  `visit_none`). Запись осталась `std::fs::write` над `content.as_bytes()` — UTF-8 без BOM.
- **Хук** (`useCopyOperations.ts`): markdown резолвится один раз в `resolveSummaryMarkdown` (ref редактора →
  `aiSummary.markdown` → legacy-конвертация), документ собирает общий `buildSummaryText(label)`, поэтому
  Copy и Export не могут разойтись. `handleExportSummary` собирает документ с `label: 'Exported'` и зовёт
  `exportFileWithDialog` с `kind: 'summary'` (дескриптор `EXPORT_KINDS` даёт `md` / `Markdown Files` /
  `Summary exported` / `Failed to export summary`);
  имя по умолчанию — `buildExportFileName({ date: meeting.created_at, title: exportTitle, kind: 'summary' })`.
  Автосохранения нет: экспортируется текущее состояние редактора, даже несохранённое.
  (Исходно было `buildSummaryText(action, label)` и `suffix` / `extension` в аргументах — см. уборку по ревью ниже.)
- **UI**: в `SummaryUpdaterButtonGroup` добавлена кнопка Export (`Download`) сразу после Copy, подпись скрывается
  так же (`hidden @[40rem]:inline`), проброшена через `SummaryPanel` (`onExportSummary`) и
  `app/meeting-details/page-content.tsx`. Группа, а значит и кнопка, видна при `hasSummary && !isSummaryLoading`;
  дополнительных условий не добавлено. Панель живой записи и legacy-вьюер не тронуты.
- **Глоссарий**: добавлен термин «Экспорт саммари (summary export)» и уточнено имя файла у экспорта транскрипции
  (`<YYYYMMDD>-<название>-transcript.txt`), которое появилось в тикете 01.
- Поведение Save/Copy не менялось; текст Copy байт-в-байт прежний (его проверяет тест «file text equals the copied
  text except for the label word»).

Проверено:

- `bun test tests/hooks/copy-export-operations.test.tsx` — 10 pass, 0 fail (аргументы диалога `md`/`Markdown Files`
  и `txt`/`Text Files`, совпадение текста файла с копией, unsaved-правки, saved-markdown и legacy-фолбэки,
  путь в буфере, отмена без тостов и без буфера, ошибка → тост `Failed to export summary` без буфера).
- `bun test` по каждому затронутому/смежному файлу отдельно: `tests/lib/export-file.test.ts` (24),
  `tests/lib/summary-document.test.ts` (5), `tests/lib/summary-content.test.ts` (3),
  `tests/lib/transcript-format.test.ts` (17), `tests/lib/empty-state-summary.test.tsx` (1),
  `tests/hooks/meeting-details-refresh.test.tsx` (3), `tests/hooks/summary-generation.test.tsx` (16),
  `tests/hooks/paginated-transcripts.test.tsx` (10) — все pass.
- `./node_modules/.bin/tsc --noEmit` — чисто.
- `cargo test --lib export` (общий `CARGO_TARGET_DIR=/tmp/opencode/cargo-shared`) — 9 новых тестов, все ok,
  614 отфильтровано.
- `rustfmt --check --edition 2021 src/export.rs` — чисто.

Не проверено в этой среде:

- **Сам нативный диалог**: фильтры «Markdown Files» / «Text Files», отмена, дозапись расширения после стирания
  его пользователем, сохранение в реальную файловую систему. `pnpm tauri:dev`/`tauri:build` здесь не запускаются
  (нет sidecars/GPU/дисплея), так что ручная проверка из чек-листа остаётся за оркестратором/ревьюером.
- Встречи с кириллическим и с пустым названием в живом приложении: имя собирается `buildExportFileName`, и это
  покрыто тестами тикета 01, но сам диалог с такими именами руками не открывался.
- Итоговый `bun test` без фильтра в этом worktree даёт 34 fail (например, model manager, audiocpp, system prompt),
  но ровно те же 34 падения воспроизводятся на чистом `84fbd79` без моих изменений; мой файл добавляет только
  проходящие тесты (178 pass / 34 fail без него → 188 pass / 34 fail с ним). Пофайловый прогон всех `tests/`
  зелёный. Причина — известное протекание `mock.module` между файлами, не регрессия тикета.
- Для оркестратора: бинарник `binaries/llama-helper-x86_64-unknown-linux-gnu` пришлось скопировать из основного
  чекаута (`.gitignore`-нутый build-артефакт), иначе build-скрипт падал; в коммит он не попал.

### Уборка по ревью `review-fixes/summary-export` (2026-11-18)

Правки по результатам двухосевого ревью. Поведение, видимое пользователю, не менялось, кроме одного
случая — пункт 6 (ошибка буфера при уже сохранённом файле). Ручную проверку в Tauri-приложении
по-прежнему не делал, чек-бокс не отмечен.

- **Единый разбор `created_at` (критично).** Раньше дата встречи читалась двумя разными способами:
  `export-file.ts` трактовал `2026-11-18` как локальный календарный день, а `summary-document.ts`
  (`new Date(value)`) — как UTC-полночь. В итоге файл `20261118-…-summary.md` мог содержать
  `**Date:** November 17, 2026`. Общий хелпер вынесен в новый нейтральный модуль
  `frontend/src/lib/meeting-date.ts` — `resolveMeetingDate(value)`; его используют `export-file.ts`
  (имя файла), `summary-document.ts` (блок метаданных) и шапка копируемой транскрипции
  (`useCopyOperations.ts`, строка `## Date:`). Модуль отдельный, потому что оба потребителя —
  `lib/`-модули без знания друг о друге; складывать хелпер в один из них значило бы дать второму
  зависимость от первого ради одной функции.
- **Невалидные календарные даты.** `new Date(2026, 12, 1)` даёт 1 января 2027, `2026-02-31` — 3 марта.
  `resolveMeetingDate` теперь сверяет год/месяц/день результата с входными частями и при расхождении
  отдаёт текущий момент, как требует спецификация.
- **Фолбэк названия в экспорте саммари.** В `useCopyOperations.ts` введено одно значение
  `exportTitle = meetingTitle ?? meeting.title`, общее для копирования/экспорта транскрипции,
  экспорта саммари и обоих имён файлов. При непустом `meetingTitle` результат байт-в-байт прежний.
- **Дублирование аргументов экспорта.** Пять констант (`suffix` / `extension` / `filterLabel` /
  `successMessage` / `errorMessage`) больше не копипастятся в трёх местах: в `export-file.ts`
  появились `ExportKind = 'transcript' | 'summary'` и карта `EXPORT_KINDS: Record<ExportKind, ExportKindDescriptor>`.
  `buildExportFileName` и `exportFileWithDialog` принимают `kind`, а не разрозненные поля, поэтому
  рассогласовать `md`-файл с `txt`-фильтром больше нельзя (это проверяет `tsc`). Свободный
  `suffix: string` заменён типизированным `kind`, так что невалидный суффикс — ошибка типов.
  Обновлены три точки вызова: `useCopyOperations.ts` (транскрипция и саммари) и `TranscriptContext.tsx`.
- **Два аргумента `buildSummaryText`.** Вместо `action: 'Copy' | 'Export'` + `label: 'Copied' | 'Exported'`
  теперь один `label: SummaryDocumentLabel` (тип уже экспортирован из `summary-document.ts`),
  а глагол выводится из него.
- **Тост при сбое только буфера обмена.** Раньше исключение из `navigator.clipboard.writeText(path)`
  после успешной записи файла показывало `Failed to export summary` / `Failed to export transcript`,
  хотя файл на диске есть. Теперь успешный тост показывается сразу после сохранения, а сбой
  копирования пути сообщается отдельно: `toast.warning('Saved, but could not copy the path', { description: path })`.
  Ошибка диалога/записи по-прежнему даёт прежний тост об ошибке и не трогает буфер.
- **Комментарии.** В `resolveSummaryMarkdown` удалены комментарии, пересказывающие `if`-ы; шапка
  функции теперь объясняет, *почему* порядок именно такой (состояние редактора важнее сохранённых
  данных, потому что несохранённые правки — это то, что видит пользователь). Удалён и
  `// User cancelled the dialog` в `export-file.ts`.
- **Два правила «есть ли расширение».** Frontend-правило приведено к смыслу Rust-шного
  `Path::extension()`: текст после последней точки считается расширением, только если перед точкой
  есть символ и после неё непустой хвост. Поэтому `plan.` → `plan.md` и `.hidden` → `.hidden.md`
  совпадают с Rust. Одно осознанное отличие задокументировано в коде: хвост обязан начинаться
  с латинской буквы, иначе Rust принял бы `2-summary` в имени `20261118-Release_v1.2-summary`
  за расширение и файл остался бы без `.md`. `buildExportFileName` от эвристики вообще не зависит —
  он дописывает расширение безусловно (суффикс точки не содержит), так что точка в названии
  встречи безопасна.

Проверено:

- `bun test tests/lib/export-file.test.ts tests/lib/summary-document.test.ts tests/hooks/copy-export-operations.test.tsx
  tests/lib/summary-content.test.ts tests/lib/transcript-format.test.ts` — 70 pass, 0 fail; то же самое
  под `TZ=UTC`, `TZ=America/New_York`, `TZ=Asia/Kamchatka` — 70 pass, 0 fail во всех трёх.
  До правок репро из ревью падало: `TZ=America/New_York` давал `**Date:** November 17, 2026 at 07:00 PM`.
- Новые тесты: дата-онли `created_at` в метаданных и в шапке транскрипции; `2026-02-31`, `2026-13-01`,
  `2026-00-10` → текущий момент; общий фолбэк названия для обоих экспортов; сбой буфера после
  сохранения → успешный тост + `Saved, but could not copy the path`; `plan.`, `plan.v2`,
  `plan.markdown`, `plan.verylongextension`, `.hidden`, `Release_v1.2` для правила расширения.
- `bun test tests/hooks/meeting-details-refresh.test.tsx` (3), `tests/hooks/summary-generation.test.tsx` (16),
  `tests/hooks/paginated-transcripts.test.tsx` (10) — по отдельности зелёные.
- Прогон каждого файла из `tests/` по отдельности — падений нет.
- `./node_modules/.bin/tsc --noEmit` — чисто.

Повторная проверка на интеграционной ветке после мержа `review-fixes/summary-export` (`106b579`):

- `bun test tests/lib/export-file.test.ts tests/lib/summary-document.test.ts tests/hooks/copy-export-operations.test.tsx
  tests/lib/summary-content.test.ts tests/lib/transcript-format.test.ts` — 70 pass, 0 fail под `TZ=UTC`,
  `TZ=America/New_York` и `TZ=Asia/Kamchatka`.
- По файлам: `export-file.test.ts` — 31 pass, `summary-document.test.ts` — 6 pass,
  `copy-export-operations.test.tsx` — 13 pass (было 10 до правок по ревью).
- `./node_modules/.bin/tsc --noEmit` — чисто.
- `cargo test --lib export` на интеграционной ветке — 9 тестов, все ok (614 отфильтровано); Rust правки
  по ревью не затронули.

Не проверено:

- **Нативный диалог сохранения в этой среде не проверялся ни разу.** `pnpm run tauri:dev` и
  `pnpm run tauri:build` не запускались (нет sidecars, GPU-тулчейна и дисплея), поэтому открытие
  диалога, фильтры «Markdown Files» / «Text Files», отмена, дозапись расширения после стирания и
  запись в реальную файловую систему остаются непроверенными. Чек-бокс ручной проверки не отмечен.
- Встречи с кириллическим и с пустым названием в живом диалоге не открывались; имя собирает
  `buildExportFileName`, и это покрыто тестами тикета 01.
- Совместный прогон `meeting-details-refresh` + `summary-generation` + `paginated-transcripts` одним
  вызовом даёт 1 fail (`SyntaxError: Export named 'useSearchParams' not found`), но ровно тот же
  результат воспроизводится на `cf611a6` без этих правок — это известное протекание `mock.module`
  между файлами, не регрессия.
