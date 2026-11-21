<div align="center">
    <h1>Самобранка</h1>
    <h3>Волшебный предмет, который в любое время и в любом месте помогает своему хозяину.</h3>
    <br>
    <a href="https://github.com/timurmazitov/samobranka/releases/"><img src="https://img.shields.io/badge/Скачать-релизы-brightgreen" alt="Releases"></a>
    <a href="https://github.com/timurmazitov/samobranka/releases"><img alt="GitHub Repo stars" src="https://img.shields.io/github/stars/timurmazitov/samobranka?style=flat"></a>
    <a href="https://github.com/timurmazitov/samobranka/releases"><img alt="GitHub Downloads (all assets, all releases)" src="https://img.shields.io/github/downloads/timurmazitov/samobranka/total?style=plastic"></a>
    <a href="https://github.com/timurmazitov/samobranka/blob/main/LICENSE"><img src="https://img.shields.io/badge/Лицензия-MIT-blue" alt="License"></a>
    <a href="https://github.com/timurmazitov/samobranka/releases"><img src="https://img.shields.io/badge/ОС-Windows,_Linux-white" alt="Supported OS"></a>
    <a href="https://github.com/timurmazitov/samobranka/releases"><img alt="GitHub Tag" src="https://img.shields.io/github/v/tag/timurmazitov/samobranka?include_prereleases&color=yellow"></a>
    <br>
    <h3>
    Открытый код • Приватность прежде всего • Русскоязычная версия
    </h3>
</div>

---

**Самобранка** — приватный помощник для встреч: записывает, расшифровывает и подводит итоги прямо на вашем компьютере. Всё распознавание идёт локально, аудио и текст встреч не покидают ваше устройство. Если для саммари выбран облачный ИИ-провайдер, ему отправляется только текст расшифровки.

Версия целиком рассчитана на русскоязычных пользователей: интерфейс и встроенные шаблоны саммари переведены на русский, каталог моделей распознавания собран вокруг лучших русских моделей — GigaAM, Parakeet, Qwen3-ASR, Whisper.

## Возможности

- 🎙️ **Автоматическая транскрибация** — в реальном времени во время встречи или после загрузки аудиофайла. Язык определяется автоматически.
- 👥 **Разделение по спикерам** — diarization на модели NVIDIA Nemotron, чтобы было понятно, кто что сказал.
- 📝 **Саммари** — краткие итоги встречи на выбранной языковой модели.
- ✅ **Задачи** — выделение задач с привязкой к ответственным и сроку.
- 📌 **Ключевые тезисы и решения** — отдельным блоком в саммари.
- 📤 **Экспорт** — расшифровка и саммари во внешние файлы (`.txt`, `.md`).

## Почему Самобранка?

- **Приватность прежде всего.** Запись, распознавание и разделение по спикерам выполняются локально. Модели, записи и расшифровки хранятся на вашей машине.
- **Русский язык.** Интерфейс, шаблоны саммари и каталог моделей распознавания ориентированы на русскую речь.
- **Экономия.** Вместо платных API — открытые модели; для саммари рекомендуется локальный Ollama.
- **Гибкость.** Работает офлайн; для саммари можно выбрать Ollama (локально), Claude, Groq, OpenRouter, OpenAI или собственный OpenAI-совместимый эндпоинт.
- **Открытый код.** Самобранка бесплатна и открыта — можно собирать из исходников и дорабатывать под себя.

> ⚠️ Официальные сборки выходят только в этом репозитории: [github.com/timurmazitov/samobranka/releases](https://github.com/timurmazitov/samobranka/releases).

## Возможности подробнее

### 🎯 Локальная транскрибация

Распознавание полностью на вашем устройстве: движок **transcribe.cpp** и каталог GGUF-моделей с русским уклоном — GigaAM v3 (CTC и RNN-T), NVIDIA Parakeet, Qwen3-ASR, Whisper large-v3. Облако не нужно.

### ⚡ Ускорение на GPU

Режим вычислений выбирается в настройках: **Авто**, **Vulkan (GPU)** или **CPU**, с автоматическим откатом на CPU, если рабочего Vulkan-драйвера нет. Инструменты сборки для GPU не требуются: Vulkan-загрузчик берётся из системы вместе с драйверами. Подробнее — [руководство по GPU-ускорению](docs/GPU_ACCELERATION.md).

### 👥 Разделение по спикерам

Отдельный stage на библиотеке audio.cpp с моделью NVIDIA Nemotron-3 Diarization: в расшифровке видно, кто и что сказал.

### 📥 Импорт и улучшение

Можно загрузить существующий аудиофайл и получить расшифровку, либо «улучшить» уже записанную встречу — расшифровать заново другой моделью. Всё локально, язык определяется автоматически.

### 🤖 Саммари на языковых моделях

Краткие итоги, задачи с ответственными и сроками, ключевые тезисы и решения. Рекомендуется локальный **Ollama**; также поддерживаются Claude, Groq, OpenRouter, OpenAI и собственный OpenAI-совместимый эндпоинт — удобно для организаций со своей ИИ-инфраструктурой.

### 🎧 Профессиональное сведение звука

Одновременная запись микрофона и системного звука с интеллектуальным приглушением и защитой от клиппинга.

### 🔄 Автообновление

Приложение само проверяет обновления и устанавливает их.

## Установка

### Windows

| Платформа | Скачать | Установка |
|-----------|---------|-----------|
| **Windows (x64)** | [.exe-установщик](https://github.com/timurmazitov/samobranka/releases/latest) | Скачайте и запустите установщик |

> **Совместимость:** установщик рассчитан на портативную x64-базу — нужен процессор с AVX2; AVX-512 не требуется. Распознавание работает на готовом рантайме transcribe.cpp через Vulkan-загрузчик (`vulkan-1.dll`) из комплекта драйверов GPU; без рабочего драйвера происходит автоматический откат на CPU.

### Linux

Требуется x86_64 Linux с glibc 2.35 или новее (предпочтителен `.deb`, иначе AppImage).

1. Скачайте последний `.deb` или `.AppImage` для Linux x64 со страницы [Releases](https://github.com/timurmazitov/samobranka/releases/latest).
2. Установите или запустите выбранный пакет:
   - **Debian/Ubuntu (.deb):** в папке загрузки выполните `sudo apt install ./<файл>.deb`.
   - **AppImage:** в свойствах файла разрешите исполнение как программы, затем запустите двойным щелчком.

Сборка из исходников: [Building on Linux](docs/building_in_linux.md) или [общие инструкции](docs/BUILDING.md).

## Архитектура

Самобранка — единое самодостаточное приложение на [Tauri](https://tauri.app/): ядро на Rust отвечает за запись звука, распознавание, diarization, хранение и саммари, интерфейс построен на Next.js. Подробнее — [документация по архитектуре](docs/architecture.md).

## Для разработчиков

Чтобы внести вклад или собрать приложение из исходников, понадобится Rust и Node.js (pnpm 9.15.9). Подробные инструкции — в [руководстве по сборке](docs/BUILDING.md).

## Участие в проекте

Будем рады вашему вкладу! Перед началом поищите похожие задачи и PR и прочитайте [CONTRIBUTING.md](CONTRIBUTING.md). Сообщения об ошибках, предложения функций и проблемы документации — через [формы задач](https://github.com/timurmazitov/samobranka/issues/new/choose).

История изменений каждого релиза — на странице [Releases](https://github.com/timurmazitov/samobranka/releases).

## Лицензия

MIT License — используйте проект в своих целях.

## Благодарности

- Часть кода заимствована у [Screenpipe](https://github.com/mediar-ai/screenpipe).
- Часть кода заимствована у [transcribe-rs](https://crates.io/crates/transcribe-rs).
- Распознавание работает на [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) (MIT); каталог GGUF-моделей загружает модели с Hugging Face `handy-computer/*-gguf`.
- Разделение по спикерам работает на [audio.cpp](https://github.com/0xShug0/audio.cpp) (Apache-2.0) с моделью NVIDIA Nemotron-3 Diarization.
