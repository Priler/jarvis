Русский вариант установки есть [тут](https://github.com/Priler/jarvis/blob/master/README_RU.md)
# Kesha v3.0 very early (aka Jarvis update)
Simple Voice Assistant made as an experiment using [Silero](https://github.com/snakers4/silero-models) & [Vosk](https://pypi.org/project/vosk/).
<br>Later on [Picovoice Porcupine Wake Word Detection](https://picovoice.ai/platform/porcupine/) & [ChatGPT](https://chat.openai.com/) was added.

![image](https://i.pinimg.com/originals/63/e9/b7/63e9b72b983793f64bffc07fd14a0e62.jpg)

`The code has NOT been polished and is provided "as is". There are a lot of code that are redundant and there are tons of improvements that can be made.`

# Pre-installation steps
You need [Python 3.8.3 (64-bit)](https://www.python.org/ftp/python/3.8.3/python-3.8.3-amd64.exe) or higher. Be sure to add to PATH during installation!<br>
Install [GIT](https://git-scm.com/download/). <br><br>

# Installation
First, Install the dependencies from the `requirements.txt` file by typing `pip install -r requirements.txt` in the unpacked project directory.<br>
Second, copy the VOSK archive to the main folder. I am using version [`small 0.22`](https://alphacephei.com/vosk/models/vosk-model-small-en-0.22.zip). But you can find other versions on [official website.](https://alphacephei.com/vosk/models). You need to rename the folder like in [screenshot](https://i.imgur.com/N3bu2lC.png)<br>
Thirdly, look at `config.py` and change the values you need to yours, for example OpenAI Key, Device ID, etc.<br>
Then, run the `main.py` file in the console, and Voilà, that's it.<br><br>

# Author
(2022) Abraham Tugalov
