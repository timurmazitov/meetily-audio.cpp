# 01 — Ребрендинг приложения в Samobranka (samobranka.ai)

Status: resolved
Type: task
Spec: .scratch/rebrand-samobranka/spec.md

## Answer

Реализовано коммитами на ветке `rebrand/samobranka`:

- `7757eaf` — полный ребрендинг (идентификаторы, строки, пути, env, workflow, доки, иконки, README)
- `17d267d` — исправления по итогам двухосного code-review: восстановлены 29 изображений docs/, возвращены legacy Homebrew-пути импорта (`/opt/homebrew/var/meetily/…`), доки data-dir приведены к `ai.samobranka.app`, updater-доки помечены как отключённые, мелкие правки формулировок
- `7a349de` — восстановлены потерянные в stash round-trip эмбеды скриншотов и блок Website в README

Верификация: `cargo check` ок; `cargo test --lib` 495 passed; `npx tsc --noEmit` ок; `bun test` 103 pass / 15 fail — идентично baseline до изменений (пре-существующие падения, не относятся к ребренду); `scripts/check-brand.py` OK; ревью обеих осей повторно пройдено после фиксов.

## Известные оговорки

- Ручной запуск Tauri-приложения и сборка установщиков не выполнялись (Linux).
- Пребилт `frontend/src-tauri/binaries/llama-helper-*` содержит встроенные строки старого бренда — пересоберётся при следующей сборке sidecar (в allowlist-е линта).
- CI-секреты `SAMOBRANKA_*` нужно создать в настройках GitHub перед первым релизом (TODO в WORKFLOWS_OVERVIEW.md, ADR 0002).
- Пуш не выполнялся — требуется отдельное разрешение.

## Comments

### 2026-11-16 — Процессный инцидент и урок

Промежуточный `git stash -u` / `git stash pop` (baseline-замер тестов) молча потерял часть рабочего дерева: 29 бинарников docs/, эмбеды README, untracked-переименования. Причина эскейпила обнаружение: вывод stash/pop был подавлен (`>/dev/null 2>&1`), а пост-проверка ограничилась количеством файлов без сравнения состава. Поймано двухосным code-review (Spec: «скриншоты сохранены»), а не процессом. Урок: после любого stash/pop — полная проверка целостности (список файлов против baseline, `git status`, сверка бинарников), вывод никогда не подавлять.
