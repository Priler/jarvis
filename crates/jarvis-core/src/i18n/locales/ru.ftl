# APP INFO
app-name = JARVIS
app-description = Голосовой ассистент

# TRAY MENU
tray-restart = Перезапустить
tray-settings = Настройки
tray-exit = Выход
tray-tooltip = JARVIS - Голосовой ассистент
tray-language = Язык
tray-voice = Голос
tray-wake-word = Движок wake-word
tray-noise-suppression = Шумоподавление
tray-vad = Детекция голоса (VAD)
tray-gain-normalizer = Нормализация громкости

# HEADER
header-home = ГЛАВНАЯ
header-commands = КОМАНДЫ
header-settings = НАСТРОЙКИ
header-system = СИСТЕМА

# SEARCH
search-placeholder = Введите команду вручную или произнесите «Джарвис» ...

# MAIN PAGE
assistant-not-running = АССИСТЕНТ НЕ ЗАПУЩЕН
assistant-offline-hint = Настроить его можно не запуская.
btn-start = ЗАПУСТИТЬ
btn-starting = ЗАПУСК...

# STATUS
status-disconnected = Отключен
status-standby = Ожидание
status-listening = Слушаю...
status-processing = Обработка...

# STATS
stats-microphone = МИКРОФОН
stats-neural-networks = НЕЙРОСЕТИ
stats-resources = РЕСУРСЫ
stats-system-default = Системный
stats-not-selected = Не выбран
stats-loading = Загрузка...

# FOOTER
footer-author = Автор проекта
footer-telegram = Наш телеграм канал
footer-github = Github репозиторий проекта
footer-support = Поддержать проект на

# SETTINGS
settings-title = Настройки
settings-general = Основные
settings-devices = Устройства
settings-neural-networks = Нейросети
settings-audio = Аудио
settings-recognition = Распознавание
settings-about = О программе
settings-language = Язык
settings-microphone = Микрофон
settings-microphone-desc = Его будет слушать ассистент.
settings-mic-default = По умолчанию (Система)
settings-voice = Голос ассистента
settings-voice-desc =
    Не все команды работают со всеми звуковыми пакетами.
    Кликните, чтобы прослушать как звучит голос.
settings-wake-word-engine = Движок активации
settings-wake-word-desc = Выберите нейросеть для распознавания активационной фразы.
settings-stt-engine = Распознавание речи
settings-intent-engine = Определение намерения
settings-intent-engine-desc = Выберите нейросеть для распознавания команд.
settings-noise-suppression = Шумоподавление
settings-noise-suppression-desc = Уменьшает фоновый шум. Может негативно влиять на распознавание.
settings-vad = Определение голоса (VAD)
settings-vad-desc = Пропускает тишину, экономит ресурсы CPU.
settings-gain-normalizer = Нормализация громкости
settings-gain-normalizer-desc = Автоматически регулирует уровень громкости.
settings-api-keys = API Ключи
settings-save = Сохранить
settings-cancel = Отмена
settings-back = Назад
settings-enabled = Включено
settings-disabled = Отключено

# settings - beta notice
settings-beta-title = БЕТА версия!
settings-beta-desc = Часть функций может работать некорректно.
settings-beta-feedback = Сообщайте обо всех найденных багах в
settings-beta-bot = наш телеграм бот
settings-open-logs = Открыть папку с логами

# settings - picovoice
settings-attention = Внимание!
settings-picovoice-warning = Эта нейросеть работает не у всех!
settings-picovoice-waiting = Мы ждем официального патча от разработчиков.
settings-picovoice-key-desc = Введите сюда свой ключ Picovoice. Он выдается бесплатно при регистрации в
settings-picovoice-key = Ключ Picovoice

# settings - vosk
settings-auto-detect = Авто-определение
settings-vosk-model = Модель распознавания речи (Vosk)
settings-vosk-model-desc =
    Выберите модель Vosk для распознавания речи.
    Вы можете скачать модели здесь: https://alphacephei.com/vosk/models
settings-models-not-found = Модели не найдены
settings-models-hint = Поместите модели Vosk в папку resources/vosk

# settings - ollama
settings-ollama-desc = Ollama позволяет запускать языковые модели локально, без интернета и без передачи данных на сторонние серверы.
settings-ollama-url = URL сервера Ollama
settings-ollama-url-desc = Адрес запущенного Ollama. По умолчанию: http://localhost:11434
settings-ollama-load-models = Загрузить модели
settings-ollama-model = Модель
settings-ollama-model-desc = Выберите модель после подключения к серверу Ollama.
settings-ollama-no-models = Модели не найдены. Убедитесь что Ollama запущена и модели установлены.
settings-ollama-error = Не удалось подключиться к Ollama

# COMMANDS PAGE
commands-title = Команды
commands-search = Поиск команд...
commands-count = { $count } команд
commands-no-commands = Нет доступных команд
commands-load-error = Не удалось загрузить команды
commands-retry = Повторить
commands-wip-title = [404] Этот раздел еще находится в разработке!
commands-wip-desc = Тут будет список команд + полноценный редактор команд.
commands-wip-follow = Следите за обновлениями в
commands-wip-channel = нашем телеграм канале

# ERRORS
error-generic = Произошла ошибка
error-connection = Ошибка подключения
error-not-found = Не найдено

# NOTIFICATIONS
notification-saved = Настройки сохранены!
notification-error = Ошибка
notification-assistant-started = Ассистент запущен
notification-assistant-stopped = Ассистент остановлен

# SLOTS EXTRACTION
settings-slot-engine = Извлечение параметров
settings-slot-engine-desc = Извлекает параметры из голосовых команд (напр. название города, число).
settings-gliner-model = Модель GLiNER ONNX
settings-gliner-model-desc =
    Выберите вариант модели.
    Квантизированные модели (int8, uint8) быстрее, но менее точны.
settings-gliner-models-hint = Модели GLiNER не найдены.

# CONFIRM OVERLAY
confirm-overlay-title = Подтверждение команды
confirm-overlay-execute = Выполнить
confirm-overlay-cmd-label = Команда

# ETC
search-error-not-running = Ассистент не запущен
search-error-failed = Не удалось выполнить команду
settings-no-voices = Голоса не найдены