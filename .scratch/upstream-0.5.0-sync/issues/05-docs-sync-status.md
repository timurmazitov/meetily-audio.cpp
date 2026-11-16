# Документация: статус синхронизации, релизная валидация и обновление research-аудита

Status: resolved
Type: task
Blocked by: 01, 03, 04

## Контекст

Спека: `.scratch/upstream-0.5.0-sync/spec.md`, пункт 25 списка user stories («Как будущий агент, я хочу найти запись о том, что уже влито из upstream»).

Аудит `docs/research/upstream-0.5.0.md` написан **до** синхронизации и в разделе 6 описывает план, а не результат. После тикетов 01–04 он частично устарел: там сказано, что `install-linux.sh` «варианты: не брать либо перебрендировать», что brand-lint падает на research-файле, и что релизный пайплайн сломан. Всё это больше не правда.

Документация по сборке (`docs/BUILDING.md`, `docs/building_in_linux.md`) описывает подпись пакетов и `latest.json`, не объясняя, что автообновление выключено по ADR 0002.

## Задача

1. **Изменить `docs/research/upstream-0.5.0.md`** так, чтобы он описывал **факт**, а не план:
   - раздел 6 — состояние «после синхронизации»: какие два merge-коммита сделаны, что влито, чего осознанно не взято (`install-linux.sh`) и почему;
   - убрать из раздела 6 указание, что brand-lint падает на этом документе (тикет 02 это чинит);
   - заменить абзац о предсуществующем дефекте релизного пайплайна на ссылку на его устранение (тикет 04) с указанием, что реальный прогон на GitHub-раннерах не выполнялся;
   - **не дублировать** хеши, PR-номера и списки коммитов в двух местах: если сведения уже есть в разделах 1–5, на них ссылаться.
   - Сохранить честные пометки «не подтверждено» там, где они остаются в силе (например: сборка macOS на OpenSSL 4, поведение Tauri при `createUpdaterArtifacts: false` на реальном прогоне).

2. **Обновить `docs/BUILDING.md` и `docs/building_in_linux.md`** — минимально, одной ясной вставкой в разделе о релизных артефактах: пакеты подписываются minisign-ключом, но механизм автообновления в приложении отключён (ADR 0002), поэтому `latest.json` не публикуется и обновления не раздаются. Ссылка на ADR обязательна.

3. **Добавить указатель в `GLOSSARY.md` ничего не нужно**; вместо этого проверить, что `docs/architecture.md` не описывает автообновление как действующее — если описывает, поправить минимально.

## Тесты (TDD)

Документация — проверка не тестами, а консистентностью. Приложить к `## Answer`:

- `python3 scripts/check-brand.py` → OK.
- Проверка, что все относительные ссылки, которые ты добавил или изменил, ведут на существующие файлы (перечислить проверенные пути).
- Проверка, что утверждения документа соответствуют коду: `grep` по `tauri.conf.json` подтверждает отсутствие `plugins.updater` и `createUpdaterArtifacts: false`; `grep` по `release.yml` подтверждает вызов скрипта ассетов и отсутствие inline `KeyError`-пути.
- `git log --oneline` — указать фактические хеши двух merge-коммитов из тикета 01 (не выдумывать).

## Заметки

- Не переписывать разделы 1–5 аудита: они верны и являются источником фактов.
- Не добавлять дату «сегодня» в текст: репозиторий использует даты только в research-документах, где они уже проставлены; для статуса синхронизации достаточно хешей коммитов.
- Тон документации — как в существующих `docs/`: короткие фактические формулировки, без маркетинга.

## Answer

### Что изменено

**`docs/research/upstream-0.5.0.md`** — разделы 1–5 не переписывались, кроме одного пункта
раздела 5 (см. ниже). Новый раздел 6 «Состояние после синхронизации» описывает результат,
а не план:

- таблица двух merge-коммитов с фактическими хешами из истории: `45d5175` (тег `v0.5.0`,
  `9e50dc2`), `ba19819` (`upstream/main`, `2166bc6`);
- «Что принято»: сборчные фиксы upstream (OpenSSL-фри `llama-helper`, пин
  `rust-toolchain@1.99.1`, `validate-release` на `ubuntu-24.04`), структура README и glibc,
  сторона форка в `tauri.conf.json`;
- «Что осознанно не взято»: `scripts/install-linux.sh` (коммит `72c1d13`) с причинами —
  чужой `REPO`, чужие пути данных, чужой бинарь;
- «Релизная валидация»: дефект устранён, логика в `scripts/check-release-assets.py` (merge
  `68bad18`), workflow переведён (merge `79a9871`), режим — по наличию `plugins.updater.pubkey`;
- «Brand-lint»: гейт проходит, документ в `EXCLUDED_FILES` по полному пути; фразы о падении
  lint на этом документе в тексте нет;
- «Что осталось непроверенным»: реальный прогон CI, поведение Tauri при
  `createUpdaterArtifacts: false`, настоящая minisign-подпись;
- «Правила для следующей синхронизации»: `git fetch upstream --tags`, не снимать пин без
  обновления `tauri-build`/`ort`, не возвращать `install-linux.sh`.

Удалено/исправлено устаревшее: абзац о «релизный пайплайн сломан» заменён разделом об
устранении; пункт раздела 5 про CI/сборку переформулирован в прошедшее время со ссылкой на
раздел 6 (сами разделы 1–5 как аудит не переписаны); не добавлена дата «сегодня».

**`docs/BUILDING.md`** — новый раздел «Release Artifacts and Updates» (после Build Notes):
пакеты Linux подписываются minisign через `tauri signer sign` (`.deb.sig` / `.AppImage.sig`,
это не updater-артефакты), Windows — Authenticode через `signCommand`; автообновление
отключено по [ADR 0002](adr/0002-rebrand-to-samobranka.md), `createUpdaterArtifacts: false`,
`plugins.updater` нет, `latest.json` не публикуется; коротко про режим
`scripts/check-release-assets.py`.

**`docs/building_in_linux.md`** — такой же раздел «📦 Release packages and updates» (язык файла —
английский) после «Build Output Location»; ссылка на ADR относительная
(`adr/0002-rebrand-to-samobranka.md`), так как файл лежит в `docs/`.

**`docs/architecture.md`** — не менялся: `grep -i "updater|latest.json|автообновлен"` по файлу
пуст, автообновление там не описано ни как действующее, ни как отсутствующее.

**`.scratch/…/05-docs-sync-status.md`** — `Status: resolved` и этот раздел.

### Три устаревших документа, помеченных в тикете 04

| Файл | Решение |
| --- | --- |
| `.github/workflows/WORKFLOWS_OVERVIEW.md` | **Исправлен минимально.** Раздел `release.yml` утверждал «Auto-generates `latest.json`», «macOS and Windows only (Linux excluded)», `.sig`-артефакты и автоинкремент версии, которого в workflow нет. Заменено на фактическое: macOS+Linux+Windows, при существующем теге — ошибка с требованием поднять версию, updater-артефактов и `latest.json` нет (ссылка на ADR). В `build-linux.yml` строка «Tauri updater signing» заменена на «No package signing (release signing lives in `build.yml`)» — скрипт `tauri signer sign` в `build-linux.yml` отсутствует. |
| `.github/workflows/README_DEVTEST.md` | **Исправлен минимально.** «Linux → Uses Tauri updater signing (Ed25519), signs update manifests for auto-updater» ложно: в `build-devtest.yml` Linux не подписывается. Заменено на «unsigned by default». Из списка артефактов убраны `*.app.tar.gz`, `.sig`; добавлено примечание, что updater-артефактов нет (ADR 0002), а пути в upload-artifact просто не совпадут. |
| `docs/RUNNING_WINDOWS.md` | **Исправлен минимально.** Три места утверждали `createUpdaterArtifacts: true` и требовали `TAURI_SIGNING_PRIVATE_KEY` для локальной сборки: раздел 7 «Про подпись» (переписан: `false`, `plugins.updater` нет, ключ не нужен; в CI ключ обслуживает `tauri signer sign` для Linux), строка таблицы «Частые проблемы», и пункт раздела 10 про «генерирует updater-подписи» (заменено на Authenticode без updater-подписей). |

### Верификация (выполнено реально)

**1. `python3 scripts/check-brand.py` — PASS**

```
$ cd /tmp/opencode/wt-05-docs && python3 scripts/check-brand.py; echo "exit=$?"
OK: no old-brand occurrences outside allowed zones
exit=0
```

**2. Относительные ссылки, добавленные или изменённые — все существуют**

```
checked links: 7
  ok  docs/BUILDING.md -> adr/0002-rebrand-to-samobranka.md
  ok  docs/BUILDING.md -> ../frontend/package.json
  ok  docs/BUILDING.md -> GPU_ACCELERATION.md
  ok  docs/BUILDING.md -> GPU_ACCELERATION.md
  ok  docs/building_in_linux.md -> adr/0002-rebrand-to-samobranka.md
  ok  docs/RUNNING_WINDOWS.md -> adr/0002-rebrand-to-samobranka.md
  ok  docs/RUNNING_WINDOWS.md -> adr/0002-rebrand-to-samobranka.md
failures: 0
```

(скрипт проверки — ad-hoc, разрешает ссылки относительно каталога файла; проверены все
markdown-ссылки шести затронутых файлов, кроме внешних URL и якорей)

**3. Утверждения соответствуют коду**

```
$ grep -n "plugins" frontend/src-tauri/tauri.conf.json
(нет вывода; exit=1 — секции plugins нет)
$ grep -n "createUpdaterArtifacts" frontend/src-tauri/tauri.conf.json
97:        "createUpdaterArtifacts": false,
$ python3 -c "import json; d=json.load(open('frontend/src-tauri/tauri.conf.json')); print('plugins present:', 'plugins' in d); print('createUpdaterArtifacts:', d['bundle']['createUpdaterArtifacts'])"
plugins present: False
createUpdaterArtifacts: False

$ grep -n "scripts/check-release-assets.py" .github/workflows/release.yml
124:          # scripts/check-release-assets.py: with a key, signatures are verified
159:          python3 scripts/check-release-assets.py release-assets "$VERSION" "$GITHUB_REPOSITORY" ${args[@]+"${args[@]}"}
$ grep -n 'plugins"\]\["updater"\]\["pubkey"\]' .github/workflows/release.yml
(нет вывода; exit=1 — inline-пути с KeyError больше нет)
```

Дополнительно, для утверждений в новых разделах build-доков:

```
$ grep -l "CMAKE_DISABLE_FIND_PACKAGE_OpenSSL" frontend/build-gpu.sh frontend/dev-gpu.sh \
    frontend/build-gpu.bat frontend/dev-gpu.bat .github/workflows/*.yml
frontend/build-gpu.sh, frontend/dev-gpu.sh, frontend/build-gpu.bat, frontend/dev-gpu.bat,
.github/workflows/{build,build-devtest,build-linux,build-macos,build-windows}.yml
$ grep -rn "rust-toolchain@" .github/workflows/*.yml
все пять build-workflow → dtolnay/rust-toolchain@1.99.1 (вхождений @stable нет)
$ grep -n "ubuntu-24.04\|minisign" .github/workflows/release.yml
96: runs-on: ubuntu-24.04   143: - name: Install minisign   148: sudo apt-get install -y minisign
$ git ls-tree -r HEAD --name-only | grep -c install-linux.sh
0
$ python3 -m unittest discover -s scripts
Ran 12 tests in 0.024s
OK
```

**4. Фактические хеши merge-коммитов**

```
$ git log --oneline --merges feat/upstream-0.5.0-sync --grep="merge(ticket"
79a9871 merge(ticket 04): validate release assets with the shared script
68bad18 merge(ticket 03): standalone release asset validation script
598494f merge(ticket 02): path-aware brand lint and vcs metadata skip
9552da2 merge(ticket 01): upstream v0.5.0 build fixes and README structure

$ git log --format='%h %s' -1 45d5175
45d5175 Merge upstream tag v0.5.0: CI build fixes
$ git log --format='%h %s' -1 ba19819
ba19819 Merge upstream/main: README structure and Linux glibc note
$ git merge-base --is-ancestor ba19819 HEAD && echo ok
ok
$ git diff --stat 588e898..ba19819 | tail -1
11 files changed, 46 insertions(+), 27 deletions(-)
```

### Что осталось непроверенным

- Реальный прогон CI / `Release` workflow: сборка macOS на OpenSSL 4, пин Rust 1.99.1 на всех
  раннерах, наличие `minisign` на `ubuntu-24.04`, реальные имена ассетов с GitHub-раннеров.
- Поведение Tauri при `createUpdaterArtifacts: false` в реальном прогоне (что `.sig` и
  `*.app.tar.gz` не выпускаются). Утверждения в доках опираются на конфиг и на отчёт тикета 04,
  не на наблюдение за прогоном.
- Настоящая minisign-подпись артефактов, собранных Tauri (в тикете 03 проверялись синтетические
  файлы с локально сгенерированным ключом).
- `gh api` в `release-summary` при реальном отсутствии `latest.json` — проверено только стабом.
- Повторное включение автообновления (новый endpoint, пара ключей, capability) — вне спеки.

### Изменённые файлы

- `docs/research/upstream-0.5.0.md` — новый раздел 6, правка пункта «CI/сборка» в разделе 5.
- `docs/BUILDING.md` — раздел «Release Artifacts and Updates».
- `docs/building_in_linux.md` — раздел «📦 Release packages and updates».
- `.github/workflows/WORKFLOWS_OVERVIEW.md`, `.github/workflows/README_DEVTEST.md`,
  `docs/RUNNING_WINDOWS.md` — точечные правки ложных утверждений про updater.
- `.scratch/upstream-0.5.0-sync/issues/05-docs-sync-status.md` — `Status: resolved`, этот раздел.
