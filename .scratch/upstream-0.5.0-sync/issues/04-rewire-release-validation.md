# Перевести release.yml на скрипт ассетов и убрать требования отключённого updater

Status: resolved
Type: task
Blocked by: 01, 03

## Контекст

Спека: `.scratch/upstream-0.5.0-sync/spec.md`, разделы «Implementation Decisions» (пункты «Согласование пайплайна с ADR 0002») и «Problem Statement».

Тикет 03 создаёт скрипт проверки ассетов. Тикет 01 вливает upstream-фиксы, включая перевод `validate-release` на образ с `minisign` (Ubuntu 24.04). Этот тикет соединяет их и убирает из пайплайна всё, что предполагает включённое автообновление.

Текущие дефекты — каждый падает независимо от merge:

1. `.github/workflows/release.yml`, job `validate-release`, шаг «Download and verify release assets»: безусловное чтение `tauri.conf.json["plugins"]["updater"]["pubkey"]` → `KeyError` под `set -euo pipefail` (секция удалена ADR 0002).
2. Там же: требование `.sig` и minisign-верификация для `*.deb`, `*.AppImage`, `*-setup.exe`, `*64_en-US.msi` и **ровно одного** `*aarch64.app.tar.gz`, при том что в конфиге `createUpdaterArtifacts: false`. Такого артефакта не будет.
3. Job `release-summary`: скачивает `latest.json` по id и падает, если ассета нет; тексты сводки утверждают, что `.sig`, `*.app.tar.gz` и `latest.json` загружены.

## Задача

**Job `validate-release`:**

- Заменить inline-Python на вызов скрипта из тикета 03. Решение «передавать ли публичный ключ» принимается в workflow **по факту наличия** `plugins.updater.pubkey` в конфиге бандла — без нового ручного флага: при включении автообновления (отдельная будущая работа) проверка включится сама.
- Сохранить существующий шаг скачивания ассетов релиза по `RELEASE_ID` (он рабочий и не зависит от updater).
- Убрать из шага всё, что генерирует `latest.json` и грузит его обратно как ассет, — когда автообновление выключено.

**Job `release-summary`:**

- Сделать блок «latest.json content» условным: если манифеста нет — пропустить, не падать.
- Привести тексты к правде: при выключенном автообновлении сводка перечисляет только реально загруженные пакеты; `.sig`/`app.tar.gz`/`latest.json` не упоминаются как загруженные. Упоминание ручной S3-загрузки оставить как указание на будущую работу со ссылкой на ADR 0002.

**Проверки, которые должны остаться рабочими:**

- Защита от релиза на уже существующем теге (`create-release`).
- Скачивание ассетов по `RELEASE_ID` (draft-релиз не виден по тегу).
- Подпись и загрузка Linux-пакетов в `build.yml` (там `tauri signer sign` вызывается явно и не зависит от `createUpdaterArtifacts`) — не трогать.

## Тесты (TDD)

Сам workflow тестами не покрывается (спека: такой тест хрупок и ловит правку YAML, а не поведение). Верификация:

- **Синтаксис:** разобрать все затронутые workflow как YAML (например `python3 -c "import yaml, sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" <files>`); приложить вывод.
- **Статический контроль отсутствия:** `grep` по `.github/workflows/release.yml` не должен находить `plugins"]["updater"]["pubkey` внутри inline-Python; должен находить вызов скрипта.
- **Проверка режима:** прогонять скрипт тикета 03 на фикстуре локально не требуется (это покрыто его тестами), но требуется подтвердить, что workflow передаёт ключ только при наличии секции — привести фрагмент.
- **Консистентность с ADR:** убедиться, что `plugins.updater` в `frontend/src-tauri/tauri.conf.json` отсутствует и `createUpdaterArtifacts` равен `false` — то есть выбранная ветка режима действительно текущая.

Реальный прогон `Release` невозможен локально; так и указать в `## Answer`.

## Заметки

- Не «починить» падение включением автообновления обратно: это Out of Scope спеки (нужны собственный endpoint и новая пара minisign-ключей).
- `includeUpdaterJson` в `build.yml` оставить как есть, если он не ломает сборку при выключенном updater; если ломает — зафиксировать наблюдение и минимально скорректировать.
- Проверить, не осталось ли в `release.yml` других мест, предполагающих updater, и перечислить их в `## Answer`.

## Answer

### Что изменено

**`.github/workflows/release.yml`, job `validate-release`** (было 1 шаг, стало 5 шагов):

- `Download release assets` — сохранён прежний механизм: ассеты скачиваются по `RELEASE_ID` (draft-релиз по тегу недоступен). `mkdir -p` вместо `mkdir`, установка `minisign` убрана из этого шага.
- `Detect updater public key` (`id: updater`) — единственное место, где читается `frontend/src-tauri/tauri.conf.json`. Читает `plugins.updater.pubkey` через `.get()`-цепочку; при отсутствии печатает строку про ADR 0002 и выходит с кодом 0 **без** записи в `GITHUB_OUTPUT`; при наличии пишет `release-assets/public.key` и `enabled=true` в `GITHUB_OUTPUT`.
- `Install minisign` — `if: steps.updater.outputs.enabled == 'true'`, чтобы Ubuntu 24.04 не ставила неиспользуемый пакет при выключенном updater.
- `Validate release assets` — вызывает `python3 scripts/check-release-assets.py release-assets "$VERSION" "$GITHUB_REPOSITORY"`, добавляя `--public-key release-assets/public.key` только когда `steps.updater.outputs.enabled == 'true'`.
- `Upload updater manifest` — `if: steps.updater.outputs.enabled == 'true'`; грузит `latest.json`, созданный скриптом. Когда updater выключен, шаг не выполняется и `latest.json` не создаётся и не загружается.

Inline-heredoc с `KeyError`-путём и генерацией `latest.json` удалён полностью.

**`.github/workflows/release.yml`, job `release-summary`:**

- Блок «latest.json content» стал условным: `LATEST_JSON_ID` берётся из `gh api`, и при пустом результате (манифеста нет) в сводку пишется строка «No updater manifest: automatic updates are disabled (see ADR 0002)», а шаг не падает. Раньше `gh api .../assets/$LATEST_JSON_ID` вызывался безусловно с пустым id.
- Убрано утверждение, что `.sig`, `app.tar.gz` и `latest.json` загружены. Сводка перечисляет только реальные пакеты: DMG, MSI+NSIS (подписаны `signCommand`), DEB+AppImage (подписаны явно `tauri signer sign` в `build.yml`), и отдельной строкой сообщает, что `app.tar.gz`, macOS/Windows `.sig` и `latest.json` не производятся.
- Упоминание S3 оставлено только как «Future work … see ADR 0002».

**Не тронуто:** защита от релиза на существующем теге (`create-release` → `git rev-parse --verify refs/tags/v${VERSION}` → `exit 1`), скачивание по `RELEASE_ID`, подпись и загрузка Linux-пакетов в `build.yml` (`tauri signer sign` вызывается явно, не зависит от `createUpdaterArtifacts`), `includeUpdaterJson` в `build.yml`.

### Верификация (реальный вывод)

**1. YAML разбирается для затронутых workflow — PASS**

```
$ python3 -c "import yaml, sys; [print(f, 'OK') for f in sys.argv[1:] if yaml.safe_load(open(f))]" \
    .github/workflows/release.yml .github/workflows/build.yml
.github/workflows/release.yml OK
.github/workflows/build.yml OK

$ python3 -c "import yaml; d=yaml.safe_load(open('.github/workflows/release.yml')); print(list(d['jobs'])); print([s.get('name') for s in d['jobs']['validate-release']['steps']]); print([s.get('name') for s in d['jobs']['release-summary']['steps']])"
['create-release', 'build-all-platforms', 'validate-release', 'release-summary']
[None, 'Download release assets', 'Detect updater public key', 'Install minisign', 'Validate release assets', 'Upload updater manifest']
['Get release assets and updater manifest', 'Release Summary']
```

Дополнительно все `run:` блоки `release.yml` прогнаны через `bash -n` — 0 ошибок:

```
create-release / Get version from tauri.conf.json and check for existing tags: OK
validate-release / Download release assets: OK
validate-release / Detect updater public key: OK
validate-release / Install minisign: OK
validate-release / Validate release assets: OK
validate-release / Upload updater manifest: OK
release-summary / Get release assets and updater manifest: OK
release-summary / Release Summary: OK
failures: 0
```

**2. Статический контроль отсутствия — PASS**

```
$ grep -n 'plugins"\]\["updater"\]\["pubkey\]' .github/workflows/release.yml
(нет вывода; exit 1 — inline-путь с KeyError отсутствует)

$ grep -n "scripts/check-release-assets.py" .github/workflows/release.yml
124:          # scripts/check-release-assets.py: with a key, signatures are verified
159:          python3 scripts/check-release-assets.py release-assets "$VERSION" "$GITHUB_REPOSITORY" ${args[@]+"${args[@]}"}
```

**3. Ключ передаётся только при наличии секции — PASS, фрагмент**

```yaml
      - name: Detect updater public key
        id: updater
        run: |
          set -euo pipefail
          python3 - <<'PYCODE'
          import base64, json, os
          from pathlib import Path

          config = json.loads(Path("frontend/src-tauri/tauri.conf.json").read_text())
          pubkey = config.get("plugins", {}).get("updater", {}).get("pubkey")
          if not pubkey:
              print("plugins.updater.pubkey is absent: signatures are not checked, latest.json is not created (ADR 0002).")
              raise SystemExit(0)
          Path("release-assets/public.key").write_bytes(base64.b64decode(pubkey))
          with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
              output.write("enabled=true\n")
          print("plugins.updater.pubkey is present: signatures will be verified and latest.json will be created.")
          PYCODE

      - name: Validate release assets
        env:
          VERSION: ${{ needs.create-release.outputs.version }}
        run: |
          set -euo pipefail
          args=()
          if [ "${{ steps.updater.outputs.enabled }}" = "true" ]; then
            args+=(--public-key release-assets/public.key)
          fi
          python3 scripts/check-release-assets.py release-assets "$VERSION" "$GITHUB_REPOSITORY" ${args[@]+"${args[@]}"}
```

`Install minisign` и `Upload updater manifest` оба имеют `if: steps.updater.outputs.enabled == 'true'`.

**3a. Прогон обеих веток локально на теле шагов, извлечённом из YAML** (не на рукописном эквиваленте). Фикстура: пустые файлы с реальными именами пакетов, `--public-key` не передаётся; выражения `${{ ... }}` заменены фактическим значением соответствующего `GITHUB_OUTPUT`:

```
$ bash -euo pipefail Detect_updater_public_key.sh   # конфиг текущей ветки
plugins.updater.pubkey is absent: signatures are not checked, latest.json is not created (ADR 0002).
detect exit=0                          # GITHUB_OUTPUT не создан → enabled пустой

$ bash Validate_release_assets.sh      # enabled=''
ok: dmg: samobranka_0.5.0_aarch64.dmg
ok: msi: samobranka_0.5.0_x64_en-US.msi
ok: nsis installer: samobranka_0.5.0_x64-setup.exe
ok: deb: samobranka_0.5.0_amd64.deb
ok: appimage: samobranka_0.5.0_amd64.AppImage
updater disabled (see ADR 0002): signatures not checked, latest.json not created
validate exit=0
latest.json: absent
```

Ветка «ключ есть» проверена на копии конфига с синтетическим (не настоящим) `plugins.updater.pubkey` и стабом `minisign` в `PATH` (настоящего `minisign` в окружении нет `which minisign` → «no minisign»):

```
$ bash Detect_updater_public_key.sh
plugins.updater.pubkey is present: signatures will be verified and latest.json will be created.
GITHUB_OUTPUT: [enabled=true]

$ bash Validate_release_assets.sh      # enabled='true', .sig отсутствуют
release asset validation failed:
  - expected exactly one app tarball package matching '*aarch64.app.tar.gz', found 0: [none]
exit=1                                 # ключ включил требование не только .sig, но и app.tar.gz

# после добавления *.app.tar.gz и .sig ко всем подписываемым пакетам:
ok: dmg: ... ; ok: app tarball: ... ; ok: msi: ... ; ok: nsis installer: ... ; ok: deb: ... ; ok: appimage: ...
ok: wrote latest.json for version 0.5.0
exit=0
# стаб minisign вызван 5 раз (app.tar.gz, msi, setup.exe, deb, AppImage) — ровно для signed=true типов
```

Это подтверждает, что флаг переключает режим, и что при выключенном updater `aarch64.app.tar.gz` не требуется.

**4. Консистентность с ADR 0002 — PASS, выбранная ветка действительно текущая**

```
$ python3 -c "import json; d=json.load(open('frontend/src-tauri/tauri.conf.json')); print('plugins key present:', 'plugins' in d); print('bundle.createUpdaterArtifacts:', d['bundle']['createUpdaterArtifacts'])"
plugins key present: False
bundle.createUpdaterArtifacts: False

$ grep -n updater frontend/src-tauri/tauri.conf.json
(нет совпадений; секция plugins.updater отсутствует)
```

То есть на текущем конфиге `Detect updater public key` гарантированно оставляет `enabled` пустым, `latest.json` не создаётся, а секция `plugins.updater` появится — и проверка включится сама — только при отдельной будущей работе по ADR 0002.

**5. `release-summary` без `latest.json` — PASS** (тело шага из YAML, `gh` заменён стабом, `GITHUB_STEP_SUMMARY` — во временный файл):

```
# Вариант «latest.json нет»
exit=0
### latest.json content
_No updater manifest: automatic updates are disabled (see ADR 0002)._

# Вариант «latest.json есть» (id=777) — содержимое выводится, шаг тоже exit=0
### latest.json content
```json
{"version":"0.5.0"}
```
```

**6. Скрипт и brand-lint (регресс на своей ветке)**

```
$ python3 -m unittest discover -s scripts
Ran 12 tests in 0.024s
OK
$ python3 scripts/check-brand.py
OK: no old-brand occurrences outside allowed zones
```

### Другие места пайплайна, предполагающие включённый updater

Перечислены явно; **ни одно не ломалось** при `createUpdaterArtifacts: false`, поэтому ни одно не менялось в этом тикете:

1. **`build.yml:691` — `includeUpdaterJson: ${{ inputs.release-id == '' }}`** в `tauri-action`. В релизном прогоне `release-id` непустой → `false`, поэтому `tauri-action` (который иначе пишет свой `latest.json`) не вмешивается. Условие инвертировано относительно смысла: JSON запрашивается там, где релиза нет, и не запрашивается там, где он есть. Сборку не ломает; смотреть на него стоит при включении автообновления, а не сейчас.
2. **`build.yml:680-682` — `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` и комментарий «Tauri updater signing (ALWAYS enabled for .sig files)»**. Секреты передаются всегда, но при `createUpdaterArtifacts: false` Tauri не эмитит updater-артефакты и `.sig` не генерирует; ключ фактически нужен только шагу `tauri signer sign` для Linux-пакетов (см. п. 3). Комментарий устарел, но поведение корректно.
3. **`build.yml:757-779` — `Sign and upload Linux release packages`** вызывает `tauri signer sign` явно и грузит `*.deb.sig` / `*.AppImage.sig`. Это осознанно оставлено (тикет: «не трогать»); эти `.sig` — обычные minisign-подписи пакетов, они не являются updater-артефактами. Из-за них сводка теперь говорит «DEB and AppImage … signed explicitly (+ .sig)», а не «никаких .sig».
4. **`build.yml:697-706, 792-801` — `upload-artifact` пути включают `*.app.tar.gz`, `*.app.tar.gz.sig`, `*.msi.sig`, `*.exe.sig`.** Работает только в не-релизных вызовах (`upload-artifacts: true`), и `upload-artifact` без совпадений не падает (не задан `if-no-files-found`), а просто предупреждает. При включённом updater пути уже готовы.
5. **`.github/workflows/WORKFLOWS_OVERVIEW.md`** (раздел `release.yml`, строки ~140, ~152-154) утверждает «Auto-generates `latest.json` for Tauri updater», «app.tar.gz (updater), .sig», «Updater manifest: latest.json» и упоминает автоинкремент версии, которого в `release.yml` нет. Это документация, а не исполняемый пайплайн; в глобальном разделе «Tauri Updater» уже стоит дисклеймер про ADR 0002, но локальное описание `release.yml` ему противоречит. Намеренно не правил: синхронизацию документации покрывает тикет 05.
6. **`.github/workflows/README_DEVTEST.md:63-71`** — описывает updater-подпись и `*.app.tar.gz.sig` для devtest-сборок; некритично, тот же контекст, что п. 5.
7. **`docs/RUNNING_WINDOWS.md:199-206, 248, 265`** — утверждает, что в `tauri.conf.json` включено `createUpdaterArtifacts: true` и нужен `TAURI_SIGNING_PRIVATE_KEY`; это уже неверно (в конфиге `false`). Документация, не пайплайн; тикет 05.

Отдельно: `frontend/src-tauri/src/lib.rs:576-578` — `tauri_plugin_updater` закомментирован, `Cargo.toml`/`package.json` всё ещё содержат зависимость. Не влияет на релизную валидацию.

### Что осталось непроверенным

- **Реальный прогон `Release` workflow локально невозможен** — не проверено, что GitHub-раннеры отдают именно те имена ассетов, что ожидает скрипт (`*_aarch64.dmg`, `*64_en-US.msi`, `*-setup.exe`, `*.deb`, `*.AppImage`), и что при `createUpdaterArtifacts: false` Tauri действительно не выпускает `.sig`/`app.tar.gz` на macOS и Windows. Проверка шла на синтетической фикстуре.
- Настоящая minisign-верификация в контексте workflow не выполнялась (в окружении нет бинаря `minisign`); режим «ключ есть» проверен со стабом. Сама реализация `verify_with_minisign` покрыта тестами скрипта (тикет 03, включая прогон с настоящим minisign 0.12).
- Не проверено, что `gh api` в `release-summary` ведёт себя так же, как локальный стаб, при реальном отсутствии ассета `latest.json` (стаб возвращает пустой список; GitHub API возвращает пустой список для несуществующего `select`, что соответствует логике, но вживую не наблюдалось).
- Не прогонялась сборка `build-all-platforms`; изменения в ней не делались.

### Изменённые файлы

- `.github/workflows/release.yml` — единственный изменённый файл (jobs `validate-release` и `release-summary`).
- `.scratch/upstream-0.5.0-sync/issues/04-rewire-release-validation.md` — `Status: resolved` и этот раздел.
