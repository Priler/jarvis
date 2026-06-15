# ### APP INFO
app-name = JARVIS
app-description = Голосовий асистент

# ### TRAY MENU
tray-restart = Перезапустити
tray-settings = Налаштування
tray-exit = Вихід
tray-tooltip = JARVIS - Голосовий асистент
tray-language = Мова
tray-voice = Голос
tray-wake-word = Рушій детекції
tray-noise-suppression = Шумозаглушення
tray-vad = Детекцiя голосу (VAD)
tray-gain-normalizer = Нормалізація гучності

# ### HEADER
header-commands = КОМАНДИ
header-settings = НАЛАШТУВАННЯ

# ### SEARCH
search-placeholder = Введіть команду вручну або скажіть «Джарвіс» ...

# ### MAIN PAGE
assistant-not-running = АСИСТЕНТ НЕ ЗАПУЩЕНО
assistant-offline-hint = Налаштувати його можна не запускаючи.
btn-start = ЗАПУСТИТИ
btn-starting = ЗАПУСК...

# ### STATUS
status-disconnected = Відключено
status-standby = Очікування
status-listening = Слухаю...
status-processing = Обробка...

# ### STATS
stats-microphone = МІКРОФОН
stats-neural-networks = НЕЙРОМЕРЕЖІ
stats-resources = РЕСУРСИ
stats-system-default = Системний
stats-not-selected = Не вибрано
stats-loading = Завантаження...

# ### FOOTER
footer-author = Автор проєкту
footer-telegram = Наш телеграм канал
footer-github = Github репозиторій проєкту
footer-support = Підтримати проєкт на

# ### SETTINGS
settings-title = Налаштування
settings-general = Основні
settings-devices = Пристрої
settings-neural-networks = Нейромережі
settings-audio = Аудіо
settings-recognition = Розпізнавання
settings-about = Про програму
settings-language = Мова
settings-microphone = Мікрофон
settings-microphone-desc = Його буде слухати асистент.
settings-mic-default = За замовчуванням (Система)
settings-voice = Голос асистента
settings-voice-desc =
    Не всі команди працюють з усіма звуковими пакетами.
    Натисніть, щоб прослухати як звучить голос.
settings-wake-word-engine = Рушій активації
settings-wake-word-desc = Виберіть нейромережу для розпізнавання активаційної фрази.
settings-stt-engine = Розпізнавання мовлення
settings-intent-engine = Визначення наміру
settings-intent-engine-desc = Виберіть нейромережу для розпізнавання команд.
settings-noise-suppression = Шумозаглушення
settings-noise-suppression-desc = Зменшує фоновий шум. Може негативно впливати на розпізнавання.
settings-vad = Визначення голосу (VAD)
settings-vad-desc = Пропускає тишу, економить ресурси CPU.
settings-gain-normalizer = Нормалізація гучності
settings-gain-normalizer-desc = Автоматично регулює рівень гучності.
settings-api-keys = API Ключі
settings-save = Зберегти
settings-cancel = Скасувати
settings-back = Назад
settings-enabled = Увімкнено
settings-disabled = Вимкнено

# settings - beta notice
settings-beta-title = БЕТА версія!
settings-beta-desc = Частина функцій може працювати некоректно.
settings-beta-feedback = Повідомляйте про всі знайдені баги в
settings-beta-bot = наш телеграм бот
settings-open-logs = Відкрити папку з логами

# settings - picovoice
settings-attention = Увага!
settings-picovoice-warning = Ця нейромережа працює не у всіх!
settings-picovoice-waiting = Ми чекаємо офіційного патча від розробників.
settings-picovoice-key-desc = Введіть сюди свій ключ Picovoice. Він видається безкоштовно при реєстрації в
settings-picovoice-key = Ключ Picovoice

# settings - vosk
settings-auto-detect = Авто-визначення
settings-vosk-model = Модель розпізнавання мовлення (Vosk)
settings-vosk-model-desc =
    Виберіть модель Vosk для розпізнавання мовлення.
    Ви можете завантажити моделі тут: https://alphacephei.com/vosk/models
settings-models-not-found = Моделі не знайдено
settings-models-hint = Помістіть моделі Vosk в папку resources/vosk

# settings - openai
settings-openai-key = Ключ OpenAI
settings-openai-not-supported = Наразі ChatGPT не підтримується. Він буде доданий у наступних оновленнях.

# ### COMMANDS PAGE
commands-title = Команди
commands-search = Пошук команд...
commands-count = { $count } команд
commands-wip-title = [404] Цей розділ ще в розробці!
commands-wip-desc = Тут буде список команд + повноцінний редактор команд.
commands-wip-follow = Слідкуйте за оновленнями в
commands-wip-channel = нашому телеграм каналі

# ### ERRORS
error-generic = Сталася помилка
error-connection = Помилка підключення
error-not-found = Не знайдено

# ### NOTIFICATIONS
notification-saved = Налаштування збережено!
notification-error = Помилка
notification-assistant-started = Асистента запущено
notification-assistant-stopped = Асистента зупинено

# SLOTS EXTRACTION
settings-slot-engine = Витяг параметрів
settings-slot-engine-desc = Витягує параметри з голосових команд (напр. назва міста, число).
settings-gliner-model = Модель GLiNER ONNX
settings-gliner-model-desc = 
    Оберіть варіант моделі.
    Квантизовані моделі (int8, uint8) швидші, але менш точні.
settings-gliner-models-hint = Моделі GLiNER не знайдено.

# ETC

search-error-not-running = Асистента не запущено
search-error-failed = Не вдалося виконати команду
settings-no-voices = Голоси не знайдено