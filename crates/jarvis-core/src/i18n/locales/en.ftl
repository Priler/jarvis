# ### APP INFO
app-name = JARVIS
app-description = Voice Assistant

# ### TRAY MENU
tray-restart = Restart
tray-settings = Settings
tray-exit = Exit
tray-tooltip = JARVIS - Voice Assistant
tray-language = Language
tray-voice = Voice
tray-wake-word = Wake Word Engine
tray-noise-suppression = Noise Suppression
tray-vad = Voice Activity Detection
tray-gain-normalizer = Gain Normalizer

# ### HEADER
header-home = HOME
header-commands = COMMANDS
header-settings = SETTINGS
header-system = SYSTEM

# ### SEARCH
search-placeholder = Enter a command manually or say «Jarvis» ...

# ### MAIN PAGE
assistant-not-running = ASSISTANT NOT RUNNING
assistant-offline-hint = You can configure it without starting.
btn-start = START
btn-starting = STARTING...

# ### STATUS
status-disconnected = Disconnected
status-standby = Standby
status-listening = Listening...
status-processing = Processing...

# ### STATS
stats-microphone = MICROPHONE
stats-neural-networks = NEURAL NETWORKS
stats-resources = RESOURCES
stats-system-default = System Default
stats-not-selected = Not selected
stats-loading = Loading...

# ### FOOTER
footer-author = Project author
footer-telegram = Our Telegram channel
footer-github = Github repository
footer-support = Support the project on

# ### SETTINGS
settings-title = Settings
settings-general = General
settings-devices = Devices
settings-neural-networks = Neural Networks
settings-audio = Audio
settings-recognition = Recognition
settings-about = About
settings-language = Language
settings-microphone = Microphone
settings-microphone-desc = The assistant will listen to this microphone.
settings-mic-default = Default (System)
settings-voice = Assistant voice
settings-voice-desc =
    Not all commands work with all sound packs.
    Click to listen the preview of sound.
settings-wake-word-engine = Wake word engine
settings-wake-word-desc = Choose the engine for wake word recognition.
settings-stt-engine = Speech recognition
settings-intent-engine = Intent recognition
settings-intent-engine-desc = Select neural network for command recognition.
settings-noise-suppression = Noise suppression
settings-noise-suppression-desc = Reduces background noise. May negatively affect recognition.
settings-vad = Voice detection (VAD)
settings-vad-desc = Skips silence, saves CPU resources.
settings-gain-normalizer = Gain normalizer
settings-gain-normalizer-desc = Automatically adjusts volume level.
settings-api-keys = API Keys
settings-save = Save
settings-cancel = Cancel
settings-back = Back
settings-enabled = Enabled
settings-disabled = Disabled

# settings - beta notice
settings-beta-title = BETA version!
settings-beta-desc = Some features may not work correctly.
settings-beta-feedback = Report all bugs to
settings-beta-bot = our Telegram bot
settings-open-logs = Open logs folder

# settings - picovoice
settings-attention = Attention!
settings-picovoice-warning = This neural network doesn't work for everyone!
settings-picovoice-waiting = We are waiting for an official patch from the developers.
settings-picovoice-key-desc = Enter your Picovoice key here. It is issued for free upon registration at
settings-picovoice-key = Picovoice Key

# settings - vosk
settings-auto-detect = Auto-detect
settings-vosk-model = Speech recognition model (Vosk)
settings-vosk-model-desc =
    Select Vosk model for speech recognition.
    You can download models here: https://alphacephei.com/vosk/models
settings-models-not-found = Models not found
settings-models-hint = Place Vosk models in resources/vosk folder

# settings - ollama
settings-ollama-desc = Ollama lets you run language models locally — no internet, no data sent to third-party servers.
settings-ollama-url = Ollama server URL
settings-ollama-url-desc = Address of your running Ollama instance. Default: http://localhost:11434
settings-ollama-load-models = Load models
settings-ollama-model = Model
settings-ollama-model-desc = Select a model after connecting to the Ollama server.
settings-ollama-no-models = No models found. Make sure Ollama is running and models are installed.
settings-ollama-error = Could not connect to Ollama

# ### COMMANDS PAGE
commands-title = Commands
commands-search = Search commands...
commands-count = { $count } commands
commands-no-commands = No commands available
commands-load-error = Failed to load commands
commands-retry = Retry
commands-wip-title = [404] This section is under development!
commands-wip-desc = Here will be a list of commands + full-featured command editor.
commands-wip-follow = Follow updates in
commands-wip-channel = our Telegram channel

# ### ERRORS
error-generic = An error occurred
error-connection = Connection error
error-not-found = Not found

# ### NOTIFICATIONS
notification-saved = Settings saved!
notification-error = Error
notification-assistant-started = Assistant started
notification-assistant-stopped = Assistant stopped

# SLOTS EXTRACTION
settings-slot-engine = Slot extraction
settings-slot-engine-desc = Extract parameters from voice commands (e.g. city name, number).
settings-gliner-model = GLiNER ONNX model
settings-gliner-model-desc =
    Select model variant.
    Smaller quantized models (int8, uint8) are faster but less accurate.
settings-gliner-models-hint = No GLiNER models found.

# CONFIRM OVERLAY
confirm-overlay-title = Command Confirmation
confirm-overlay-execute = Execute
confirm-overlay-cmd-label = Command

# ETC
search-error-not-running = Assistant is not running
search-error-failed = Failed to execute command
settings-no-voices = No voices found