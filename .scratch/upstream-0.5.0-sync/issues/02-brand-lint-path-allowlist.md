# Brand-lint: path-aware allowlist и исключение research-документа

Status: resolved
Type: task
Blocked by: —

## Контекст

Спека: `.scratch/upstream-0.5.0-sync/spec.md`, раздел «Implementation Decisions», пункт «Brand-гейт».

`scripts/check-brand.py` — единственный автоматический гейт на проникновение upstream-бренда; спека запрещает вводить второй. Два дефекта:

1. `EXCLUDED_FILES` сравнивается с `name` (только базовое имя файла), а не с путём. Поэтому исключение по конкретному пути невыразимо: любая запись вида `docs/research/...` никогда не совпадёт.
2. Новый `docs/research/upstream-0.5.0.md` содержит имя upstream как **предмет исследования** (пути `com.meetily.ai`, `Zackriya-Solutions/meetily`, `install-linux.sh`). Сейчас линт падает на четырёх строках этого файла.

## Задача

- Научить `EXCLUDED_FILES` сопоставляться и по базовому имени, и по нормализованному относительному пути (разделитель — `/`, независимо от платформы). Существующая запись `LICENSE.md` должна продолжать работать.
- Добавить в исключения `docs/research/upstream-0.5.0.md` с комментарием, объясняющим, почему (имя upstream в нём — предмет исследования, а не регресс бренда).
- Не расширять allowlist другими способами: не ослаблять `BRAND`-паттерн, не исключать весь каталог `docs/research/` — там в будущем появятся документы, где бренд всё равно важен.

## Тесты (TDD)

Отдельного тест-раннера у корневых скриптов нет, а вводить пакетный менеджер ради одного скрипта спека запрещает. Поэтому верификация — поведенческая, на реальном дереве, и результат приложить к `## Answer`:

1. `python3 scripts/check-brand.py` на текущем дереве → `OK`, exit 0.
2. **Негативный контроль** (обязателен, иначе нельзя утверждать, что гейт не сломан): временно добавить строку с `meetily` в произвольный отслеживаемый `.md` вне allowlist (например `docs/architecture.md`), прогнать линт → должен упасть с exit 1 и указать этот файл. Вернуть файл в исходное состояние (`git checkout -- <file>`) и перепроверить, что линт снова OK. Приложить оба вывода.
3. **Контроль path-awareness**: временно положить строку с `meetily` в файл с именем, встречающимся и в исключённом, и в другом пути — не обязателен; достаточно показать, что `LICENSE.md` по-прежнему исключается, а одноимённый файл в подкаталоге — нет.

## Заметки

- Правка локально уже была сделана при подготовке спеки, но в базовый коммит не попала — реализовать её в этом тикете как новую работу, не полагаясь на рабочее дерево.
- Линт исключает `.scratch/`, поэтому тикеты и спека его не ломают.
- При прогоне в git-ворочном дереве линт может спотыкаться о файл `.git` (в нём `gitdir: /path/.git/worktrees/<name>`). Убедиться, что `.git` (как файл, а не каталог) корректно пропускается — иначе гейт бесполезен в worktree-схеме, которую использует этот репозиторий.

## Answer

### Что изменено

Только `scripts/check-brand.py` (единственный гейт, второй не вводился).

1. **Path-aware исключения.** `EXCLUDED_FILES` теперь сравнивается с нормализованным
   repo-root-относительным путём (`/` на любой платформе), а не с `name`. `LICENSE.md`
   трактуется как путь корня, поэтому одноимённый файл в подкаталоге **не** исключается.
   Проверка `name in EXCLUDED_FILES` удалена.
2. **Исследовательский документ.** `docs/research/upstream-0.5.0.md` остаётся в исключениях;
   комментарий переписан так, что фиксирует причину (имя upstream — предмет исследования) и
   прямо оговаривает, что остальные документы в `docs/research/` продолжают проверяться.
   Весь каталог не исключался.
3. **VCS-метаданные, а не только каталог `.git`.** Добавлен `VCS_METADATA = {".git", ".hg",
   ".svn", ".bzr", "CVS"}`: он и вырезается из `dirnames`, и **пропускается среди `filenames`**.
   В linked worktree `.git` — это файл с `gitdir: /home/timur/sources/meetily-audio.cpp/.git/...`,
   то есть строка содержит `meetily`, и гейт падал на самом себе. Из `EXCLUDED_DIRS` `.git` убран
   (иначе исключение было бы частичным и вводило в заблуждение).
4. **Мелкий рефактор.** Проверка имени файла объединена в основной цикл (раньше это был второй
   проход по `filenames`): убрано дублирование и снята разница в нормализации пути (раньше
   `FILENAME:`-ветка не делала `replace(os.sep, "/")`). Содержимое нечитаемых/binary-файлов
   по-прежнему не инспектируется, но имя такого файла теперь проверяется гарантированно.

`BRAND`, `ALLOWED_LINE_PATTERNS`, `ALLOWED_FILES`, список `EXCLUDED_DIRS` по существу не менялись.

### Почему прежний подход был недостаточен

- В присланном драфте `.git` исключался только как **каталог** (`EXCLUDED_DIRS`), поэтому
  в любом worktree линт давал ложный FAIL на `.git:1: gitdir: .../meetily-audio.cpp/...`.
  Это не косметика: репозиторий работает в worktree-схеме, и гейт был бы бесполезен (или
  выключен) именно там, где он нужен.
- `FILENAME:`-проверка шла отдельным проходом без нормализации разделителей и формально
  дублировала `EXCLUDED_FILES`-логику — расхождение легко становится дырой при правке.
- Комментарий к research-исключению не оговаривал, что каталог не исключён целиком, то есть
  не защищал от «расширения до всего `docs/research/`».

### Верификация (реальные прогоны, Linux, worktree `/tmp/opencode/wt-02-brandlint`)

Базлайн до правки (RED — воспроизводит дефект из заметок тикета):

```
$ python3 scripts/check-brand.py
FAIL: 1 old-brand occurrence(s) outside allowed zones:

.git:1: gitdir: /home/timur/sources/meetily-audio.cpp/.git/worktrees/wt-02-brandlint
EXIT=1
```

1. Чистое дерево после правки → OK:
```
$ python3 scripts/check-brand.py
OK: no old-brand occurrences outside allowed zones
EXIT=0
```
   То же из другого cwd (`cd /tmp && python3 /tmp/opencode/wt-02-brandlint/scripts/check-brand.py`) → `OK`, `EXIT=0`
   (ROOT считается от `__file__`, запуск не зависит от cwd).

2. **Негативный контроль** (временная строка в `docs/architecture.md`, затем откат):
```
+ python3 scripts/check-brand.py
OK: no old-brand occurrences outside allowed zones
+ echo EXIT=0
EXIT=0
+ printf '\nTEMP brand: meetily\n' >> docs/architecture.md
+ python3 scripts/check-brand.py
FAIL: 1 old-brand occurrence(s) outside allowed zones:

docs/architecture.md:80: TEMP brand: meetily
+ echo EXIT=1
EXIT=1
+ git checkout -- docs/architecture.md
+ python3 scripts/check-brand.py
OK: no old-brand occurrences outside allowed zones
+ echo EXIT=0
EXIT=0
```

3. **Path-awareness.** `LICENSE.md` корня исключён по пути, одноимённый файл в подкаталоге — нет:
```
--- A: same-named file in subdir must NOT be excluded
$ printf 'meetily\n' > docs/tmpbrand/LICENSE.md && python3 scripts/check-brand.py
FAIL: 1 old-brand occurrence(s) outside allowed zones:

docs/tmpbrand/LICENSE.md:1: meetily
EXIT=1
--- restored:
OK: no old-brand occurrences outside allowed zones
EXIT=0
```
   Обратная сторона: временная строка с брендом **в самом** корневом `LICENSE.md` → `OK`, `EXIT=0`
   (запись действительно исключена). Research-исключение не расширено на каталог — соседний документ ловится:
```
$ printf '\nmeetily\n' >> docs/research/audio-cpp-c-api.md && python3 scripts/check-brand.py
FAIL: 1 old-brand occurrence(s) outside allowed zones:

docs/research/audio-cpp-c-api.md:166: meetily
EXIT=1
(после отката) OK, EXIT=0
```
   Контроль «имя как у исключённого, но другой путь» — `docs/sub/upstream-0.5.0.md` → FAIL на этом файле
   (доказательство, что путь сравнивается, а не базовое имя).

4. **Доказательство, что research-документ — единственная причина исключения, а не маскировка**:
   прогон копии скрипта без строки `docs/research/upstream-0.5.0.md` в allowlist даёт 8 вхождений именно в нём
   (строки 1, 3, 4, 56, 65, 66, 84, 130) и `EXIT=1`; с исключением — OK. Значит документ чист по всем
   остальным файлам, а гейт на остальном дереве остаётся строгим.

5. **Дополнительно (сверх тикета)**: файл с брендом в *имени* в подкаталоге → `FILENAME: docs/a/b/MeetilyNotes.md`;
   бинарный файл с брендом в содержимом → FAIL на строке. После удаления фикстур — снова OK.
   `python3 -m py_compile scripts/check-brand.py` → компилируется.

Рабочее дерево после проверок: изменён только `scripts/check-brand.py` (`git status --short` → ` M scripts/check-brand.py`),
все временные файлы и `__pycache__` удалены.

### Что осталось непроверенным

- Windows/macOS не прогонялись; path-нормализация (`os.sep` → `/`) и `VCS_METADATA` покрывают их по коду,
  но фактический прогон был только на Linux.
- Вложенные/внешние worktree с иным именем каталога и сабмодуль `backend/whisper.cpp` (в `backend/`, уже исключён)
  отдельно не проверялись.
- CI-гейта, дергающего скрипт, в репозитории нет — он запускается вручную (как и раньше); вводить его — вне тикета.
