# Скрипт релизных ассетов: проверка пакетов, minisign и latest.json

Status: resolved
Type: task
Blocked by: —

## Контекст

Спека: `.scratch/upstream-0.5.0-sync/spec.md`, раздел «Implementation Decisions», пункт «Релизная валидация: новый шов».

Сейчас логика живёт как inline Python-heredoc внутри `.github/workflows/release.yml` (job `validate-release`) и непроверяема: её нельзя запустить локально, нельзя покрыть тестами, нельзя отладить без релиза. Она к тому же сломана: безусловно читает `tauri.conf.json["plugins"]["updater"]["pubkey"]` (секция удалена ADR 0002 → `KeyError`) и требует `*.sig` для всех пакетов, хотя `createUpdaterArtifacts: false`.

Этот тикет создаёт **только скрипт и его тесты**. Перепроводка workflow — тикет 04.

## Задача

Создать самостоятельный исполняемый скрипт в каталоге корневых скриптов (Python 3, только стандартная библиотека — как `scripts/check-brand.py`).

**Контракт вызова.** Аргументами передаются: каталог с уже скачанными ассетами, версия, `owner/repo`, и опционально путь к файлу публичного ключа. Скрипт не обращается к GitHub API и не знает про окружение CI: на вход — файлы на диске.

**Таблица типов пакетов.** Один источник правды: «тип → glob → подписывается ли». Начальное содержимое соответствует текущей проверке: dmg (без подписи), `app.tar.gz` (подпись), msi (подпись), `-setup.exe` (подпись), `.deb` (подпись), `.AppImage` (подпись). Правка конфига бандла должна быть правкой одной строки этой таблицы.

**Два режима, определяемые наличием публичного ключа** (не флагом в workflow):

- **Ключа нет (текущее состояние, автообновление выключено по ADR 0002):** для каждого типа проверить, что найден ровно один файл; подписи не проверять; `latest.json` не создавать; напечатать явную строку о том, что автообновление выключено и манифест пропущен.
- **Ключ есть (будущее):** дополнительно требовать `.sig` для каждого подписываемого типа, верифицировать подпись через minisign и собрать `latest.json` в текущем формате — `version`, `notes` (пустая строка), `pub_date` (UTC ISO-8601 с `Z`), `platforms` с записями `darwin-aarch64`, `darwin-aarch64-app`, `windows-x86_64-nsis`, `windows-x86_64-msi`, `windows-x86_64`, `linux-x86_64-deb`, `linux-x86_64-appimage`; каждая запись — `url` (GitHub Releases download с URL-кодированным именем файла) и `signature` (содержимое `.sig`).

**Ошибки.** Человекочитаемые, с перечислением фактически найденных файлов по типу: «ожидался ровно один X, найдено N: [имена]». Ненулевой код возврата.

**Изоляция.** Вызов minisign — одна отдельная функция, чтобы тесты подменяли её без настоящего ключа. Механизм подмены должен быть простым и явным (передаваемый callable или параметр модуля), без фреймворков.

## Тесты (TDD)

Писать тесты **до** реализации. Раннер — стандартный `unittest` Python; в репозитории нет тест-раннера для корневых скриптов, а внешние зависимости спека запрещает. Тесты не пишут в репозиторий: фикстуры — пустые файлы с правильными именами в `tempfile.TemporaryDirectory()`.

Обязательные случаи (номера — из спеки, раздел Testing Decisions):

1. полный комплект пакетов без ключа → успех, `latest.json` не создан;
2. отсутствует один тип → ошибка с именем отсутствующего типа;
3. два пакета одного типа → ошибка с указанием дубля;
4. ключ присутствует, `.sig` есть у всех подписываемых → успех, `latest.json` создан с корректным `version` и полным набором платформ;
5. ключ присутствует, у одного пакета нет `.sig` → ошибка;
6. верификация подписи провалилась → ошибка (через подменённую функцию minisign, без настоящего ключа).

Дополнительно: `pub_date` парсится как ISO-8601 UTC; `url` содержит версию и URL-кодированное имя файла.

Прогон: `python3 -m unittest discover` в каталоге скриптов (или явный путь к тест-файлу — зафиксировать точную команду в тикете при реализации и приложить вывод).

## Заметки

- Держать скрипт свободным от знания о Tauri: он не читает `tauri.conf.json`. Решение «включён ли автообновление» принимает **вызывающая сторона** в workflow и передаёт ключ или не передаёт. Это делает скрипт тестируемым и переиспользуемым.
- Формат `latest.json` должен остаться побайтово совместимым по схеме с текущим — `indent=2` и завершающий перевод строки, как в существующем heredoc.
- Не логировать содержимое ключей и подписей.

## Answer

### Что сделано

- `scripts/check-release-assets.py` — исполняемый (`100755`) CLI, Python 3, только стандартная библиотека. Не читает `tauri.conf.json`, не ходит в GitHub API, не знает про CI-окружение.
- `scripts/test_check_release_assets.py` — тесты на `unittest`, фикстуры в `tempfile.TemporaryDirectory()`, в репозиторий ничего не пишут.
- `.gitignore` — добавлен `__pycache__/` (тесты выполняют скрипт как модуль).

Имя тест-файла с подчёркиваниями, а не с дефисами: `python3 -m unittest discover` импортирует только файлы, чьё имя — валидный идентификатор Python. Сам скрипт сохраняет дефисную конвенцию `scripts/`.

### Контракт вызова

```
python3 scripts/check-release-assets.py ASSETS_DIR VERSION OWNER/REPO [--public-key PATH]
```

- `ASSETS_DIR` — каталог с уже скачанными ассетами релиза.
- `VERSION` — версия без ведущего `v` (в URL подставляется `v{VERSION}`).
- `OWNER/REPO` — например `timurmazitov/samobranka`.
- `--public-key PATH` — **единственный переключатель режима**. Ключ передан → включаются проверка `.sig` через minisign и запись `latest.json`. Ключ не передан → проверяются только пакеты, `latest.json` не создаётся, печатается строка про ADR 0002.

Коды возврата: `0` — успех; `1` — любая ошибка (каталог/ключ не найден, неверный `OWNER/REPO`, нет/дубль пакета, нет `.sig`, подпись не прошла).

Точка подмены minisign для тестов — параметр функции `main`: `main(argv, run_minisign=<callable>)`, сигнатура callable `(asset: Path, public_key: Path, signature: Path) -> None`, исключение — `MinisignError`. Боевая реализация — `verify_with_minisign()` (shell-out на `minisign -Vm ... -p ... -x ...`). Она не логирует ни ключ, ни подпись.

Таблица типов — `PACKAGES` (dataclass `Package(name, glob, signed, updater_artifact)`), один источник правды. `updater_artifact=True` только у `*aarch64.app.tar.gz`: при выключенном updater этот тип не требуется, потому что `createUpdaterArtifacts: false`. Порядок ключей `latest.json` — `PLATFORM_ORDER`, совпадает с историческим heredoc.

### Команда прогона (точная)

```
python3 -m unittest discover -s scripts -v
```

(эквивалент без discover: `python3 scripts/test_check_release_assets.py`).

### Реальный вывод

```
$ cd /tmp/opencode/wt-03-assets && python3 -m unittest discover -s scripts -v
test_duplicate_package_type_is_reported (test_check_release_assets.ReleaseAssetsTest.test_duplicate_package_type_is_reported) ... ok
test_full_set_without_key_succeeds_and_skips_manifest (test_check_release_assets.ReleaseAssetsTest.test_full_set_without_key_succeeds_and_skips_manifest) ... ok
test_manifest_pub_date_is_iso8601_utc (test_check_release_assets.ReleaseAssetsTest.test_manifest_pub_date_is_iso8601_utc) ... ok
test_manifest_url_encodes_file_name (test_check_release_assets.ReleaseAssetsTest.test_manifest_url_encodes_file_name) ... ok
test_missing_package_type_is_reported (test_check_release_assets.ReleaseAssetsTest.test_missing_package_type_is_reported) ... ok
test_missing_signature_is_reported (test_check_release_assets.ReleaseAssetsTest.test_missing_signature_is_reported) ... ok
test_real_minisign_adapter_invokes_binary (test_check_release_assets.ReleaseAssetsTest.test_real_minisign_adapter_invokes_binary) ... ok
test_real_minisign_adapter_reports_nonzero_exit (test_check_release_assets.ReleaseAssetsTest.test_real_minisign_adapter_reports_nonzero_exit) ... ok
test_signature_verification_failure_is_reported (test_check_release_assets.ReleaseAssetsTest.test_signature_verification_failure_is_reported) ... ok
test_signed_packages_with_key_produce_manifest (test_check_release_assets.ReleaseAssetsTest.test_signed_packages_with_key_produce_manifest) ... ok
test_without_key_signatures_are_not_required (test_check_release_assets.ReleaseAssetsTest.test_without_key_signatures_are_not_required) ... ok
test_without_key_updater_artifact_is_optional (test_check_release_assets.ReleaseAssetsTest.test_without_key_updater_artifact_is_optional) ... ok

----------------------------------------------------------------------
Ran 12 tests in 0.026s

OK
```

RED до реализации (тест-файл существовал, скрипта ещё не было):

```
FileNotFoundError: [Errno 2] No such file or directory: '.../scripts/check-release-assets.py'
----------------------------------------------------------------------
Ran 3 tests in 0.001s
FAILED (errors=3)
```

### Покрытие: шесть случаев спеки

1. Полный комплект без ключа → успех, `latest.json` не создан, в выводе ADR 0002 и «latest.json not created» — `test_full_set_without_key_succeeds_and_skips_manifest`.
2. Отсутствует один тип → ошибка с именем типа и glob, «found 0: [none]» — `test_missing_package_type_is_reported`.
3. Два пакета одного типа → ошибка, «found 2» и оба имени файла — `test_duplicate_package_type_is_reported`.
4. Ключ есть, `.sig` у всех подписываемых → успех, `latest.json` с корректным `version`, `notes=""`, полным набором из 7 платформ и порядком ключей — `test_signed_packages_with_key_produce_manifest`.
5. Ключ есть, у одного пакета нет `.sig` → ошибка, названо ожидаемое имя `.sig`, манифест не создан — `test_missing_signature_is_reported`.
6. Верификация подписи провалилась (подменённый callable) → ошибка, манифест не создан — `test_signature_verification_failure_is_reported`.

Дополнительно к шести:

- `test_manifest_pub_date_is_iso8601_utc` — `pub_date` оканчивается на `Z`, парсится `datetime.fromisoformat`, смещение UTC (0), свежесть < 5 мин.
- `test_manifest_url_encodes_file_name` — URL содержит `v{VERSION}/` и URL-кодированное имя (в фикстуре имя `app.tar.gz` намеренно содержит пробелы → `%20`).
- `test_without_key_signatures_are_not_required` — режим без ключа не требует `.sig` при `createUpdaterArtifacts: false`.
- `test_without_key_updater_artifact_is_optional` — без ключа отсутствие `*.app.tar.gz` не ошибка (проверено, что тест падает, если требование вернуть).
- `test_real_minisign_adapter_invokes_binary` / `test_real_minisign_adapter_reports_nonzero_exit` — боевой `verify_with_minisign` (base64-декод `.sig` + shell-out) прогнан через стаб-бинарь `minisign` в `PATH`, без настоящего ключа.

### Ручная проверка (вне тестов)

- Полный комплект без ключа на реальном каталоге: exit 0, манифеста нет.
- Дубль `*.deb` + отсутствующий msi: exit 1, оба сообщения в stderr.
- `--public-key` на несуществующий файл: exit 1.
- Несуществующий каталог: exit 1.
- **Настоящий minisign 0.12 с настоящей парой ключей** (в `/tmp`, не в репозитории): сгенерировал ключи, подписал 5 подписываемых файлов, `base64`-обёрнул `.minisig` в `.sig` — скрипт вернул exit 0 и собрал `latest.json`; после порчи одного байта `.deb` — exit 1 и «signature verification failed for ...: Signature verification failed». Это подтверждает, что base64-формат `.sig` и вызов `minisign -Vm` совпадают с тем, что делает heredoc.
- `python3 -m py_compile` на обоих файлах — OK.
- `python3 scripts/check-brand.py` проходит по содержимому нового кода; в worktree он ругается только на `.git`-файл (gitdir указывает на основной репозиторий с upstream-именем каталога) — это артефакт worktree, не новое нарушение бренда.

### Что осталось непроверенным

- Настоящий прогон `Release` workflow невозможен локально: не проверено, что GitHub-раннеры отдают именно такие имена ассетов и что `tauri signer sign` кладёт `.sig` в base64 (локально формат `.sig` воспроизведён вручную).
- Реальная minisign-подпись артефактов, собранных Tauri, не проверялась — только синтетические файлы, подписанные локально сгенерированным ключом.
- `.github/workflows/release.yml` не изменялся (тикет 04). Скрипт в пайплайне ещё не вызывается.
