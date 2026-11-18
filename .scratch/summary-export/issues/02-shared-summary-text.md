# 02: Общий текст саммари для Copy и Export

**What to build:** Для пользователя ничего не меняется: кнопка **Copy** на вкладке Summary кладёт в буфер байт-в-байт прежний текст. Меняется внутреннее устройство: сборка текста уезжает из хука копирования в чистую функцию, которая принимает markdown саммари, название встречи, её ID, дату и слово метки (`Copied` / `Exported`), а возвращает полный документ — шапку с названием, блок метаданных с ID, датой и строкой метки, разделитель и markdown саммари.

Различие между вариантами `Copied` и `Exported` должно быть ровно в одном слове строки метки; всё остальное, включая форматирование даты в метаданных, собирается одинаково. Это единственная точка, через которую впоследствии пойдут обе кнопки, поэтому расхождение буфера и файла становится структурно невозможным.

Конвертация legacy-саммари (секции со списками блоков) тоже становится чистой функцией рядом со сборщиком, чтобы её можно было проверить тестом без React. Резолв markdown из редактора (ref BlockNote → сохранённый markdown → legacy-конвертация) остаётся в хуке: он зависит от ref и жизненного цикла компонента.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] Copy выдаёт тот же текст, что и до изменения: шапка `# Meeting Summary: <название>`, `**Meeting ID:**`, `**Date:**`, `**Copied on:**`, `---`, markdown саммари
- [x] Вариант для файла отличается от варианта для буфера только словом в строке метки
- [x] Legacy-саммари превращается в markdown той же чистой функцией, что и раньше по смыслу
- [x] Пустой результат резолва по-прежнему не копируется и даёт тост об отсутствии содержимого
- [x] `bun test` проверяет совпадение двух вариантов за вычетом строки метки и корректность legacy-конвертации
- [x] Хук копирования не содержит собственной сборки шапки и метаданных

## Comments

Реализовано в ветке `ticket/summary-export-02`.

Коммиты:

- `492d377` — Саммари: общий сборщик текста для Copy и Export (модуль, тест, перевод хука).

Новый модуль `frontend/src/lib/summary-document.ts`:

- `buildSummaryDocument({ markdown, meetingTitle, meetingId, meetingDate, label, timestamp? }): string` — собирает шапку, блок метаданных (`**Meeting ID:**`, `**Date:**`, `**<label> on:**`), `\n\n---\n\n` и markdown саммари. `label` — `'Copied' | 'Exported'`, `timestamp` по умолчанию `new Date()`. Форматирование обеих дат — прежнее `toLocaleDateString('en-US', { year, month, day, hour: '2-digit', minute: '2-digit' })`.
- `convertLegacySummaryToMarkdown(summary: unknown): string` — прежняя конвертация секций `{ title, blocks }` в `## <title>` + `- <content>`, с тем же набором пропускаемых ключей (`markdown`, `summary_json`, `_section_order`, `MeetingName`) и тем же отбрасыванием пустых секций.

Хук `useCopyOperations.ts` (`handleCopySummary`): собственной сборки шапки и метаданных больше нет; резолв markdown (ref BlockNote → `aiSummary.markdown` → legacy-конвертация), проверка `hasVisibleSummaryContent`, проверка пустого результата и тосты остались на месте. Изменения локальны для summary-колбэка; transcript-часть не тронута.

Проверено:

- `bun test tests/lib/summary-document.test.ts` — 5 pass / 0 fail.
- `./node_modules/.bin/tsc --noEmit` — чисто.
- `bun test tests/lib/summary-content.test.ts` (3 pass), `tests/lib/transcript-format.test.ts` (17 pass), `tests/hooks/meeting-details-refresh.test.tsx` (3 pass) — соседние тесты не задеты.
- Побайтовое совпадение с прежней реализацией проверено разовым скриптом (не коммитился): старый inline-код против новой функции на 8 legacy-фикстурах (включая кириллицу, пустые секции, не-секции, `markdown`/`summary_json`/`_section_order`/`MeetingName`, многострочный контент) и полный документ с фиксированным `copiedAt` — совпадение полное.

Не проверено:

- Поведение в живом Tauri-приложении (реальный ref BlockNote, реальный буфер обмена, тосты) — по контексту спеки это ручная проверка оркестратора/ревьюера.
- Полный `bun test` не гонялся целиком: в нём 12 предсуществующих падений из-за утечки `mock.module` между файлами (см. контекст спеки).

### Дополнение по ревью `review-fixes/summary-export` (2026-11-18)

- **Дата в метаданных читается общим хелпером.** `formatMetadataDate(meetingDate)` заменён на
  `formatMeetingDate`, который сначала зовёт `resolveMeetingDate` из нового нейтрального модуля
  `frontend/src/lib/meeting-date.ts`, а форматирование оставляет прежним. Причина — расхождение
  с именем файла: дата-онли `created_at` (`2026-11-18`) разбиралась здесь как UTC-полночь и к западу
  от Гринвича давала в блоке `**Date:**` предыдущий день, тогда как `export-file.ts` считал тот же
  `created_at` локальным календарным днём. Теперь буфер и файл всегда называют один день.
  Для полноценных инстансов (`2026-11-18T10:30:00Z`) форматирование не изменилось.
- **Единственный аргумент `buildSummaryText`.** В хуке `action: 'Copy' | 'Export'` + `label`
  свёрнуты в один `label: SummaryDocumentLabel`; глагол для тостов выводится из метки
  (`Copied` → `copy`, `Exported` → `export`). Внешнее поведение тостов прежнее.
- **Общее название встречи.** `meetingTitle` заменён на `exportTitle = meetingTitle ?? meeting.title`
  и в сборке документа тоже, поэтому шапка `# Meeting Summary:` не теряет название, когда
  редактируемый заголовок не передан.

Проверено: `bun test tests/lib/summary-document.test.ts` — 6 pass, 0 fail (включая новый тест
«reads a date-only created_at as that local calendar day»); набор из пяти файлов экспорта — 70 pass
под `TZ=UTC`, `TZ=America/New_York` и `TZ=Asia/Kamchatka`; `./node_modules/.bin/tsc --noEmit` — чисто.

Не проверено: живое Tauri-приложение, как и выше — `tauri:dev`/`tauri:build` в этой среде не запускались.
