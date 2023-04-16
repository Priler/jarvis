import os
import yaml
import time
import random
import openai
import config
import subprocess

from .tts import cached_response, model_response

from pycaw.pycaw import (
    AudioUtilities,
    IAudioEndpointVolume
)
from ctypes import POINTER, cast
from comtypes import CLSCTX_ALL
from gpytranslate import SyncTranslator

RESPONSES = yaml.safe_load(
    open('responses.yaml'),
)
CDIR = os.getcwd()
t = SyncTranslator()
openai.api_key = config.OPENAI_TOKEN

def execute_cmd(cmd: str):
    
    if cmd == 'help':
        
        text = (
            "Я умею: ..."
            "произносить время ..."
            "рассказывать анекдоты ..."
            "и открывать браузер"
        )
        model_response(text)


    elif cmd == 'joke':
        jokes = ['Как смеются программисты? ... ехе ехе ехе',
                'ЭсКьюЭль запрос заходит в бар, подходит к двум столам и спрашивает .. «м+ожно присоединиться?»',
                'Программист это машина для преобразования кофе в код']

        cached_response("ok")

        model_response(random.choice(jokes))

    elif cmd == 'open_browser':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Run browser.exe'])
        cached_response("ok")

    elif cmd == 'open_youtube':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Run youtube.exe'])
        cached_response("ok")

    elif cmd == 'open_google':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Run google.exe'])
        cached_response("ok")

    elif cmd == 'music':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Run music.exe'])
        cached_response("ok")

    elif cmd == 'music_off':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Stop music.exe'])
        time.sleep(0.2)
        cached_response("ok")

    elif cmd == 'music_save':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Save music.exe'])
        time.sleep(0.2)
        cached_response("ok")

    elif cmd == 'music_next':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Next music.exe'])
        time.sleep(0.2)
        cached_response("ok")

    elif cmd == 'music_prev':

        subprocess.Popen([f'{CDIR}\\custom-commands\\Prev music.exe'])
        time.sleep(0.2)
        cached_response("ok")

    elif cmd == 'sound_off':

        cached_response("ok")

        devices = AudioUtilities.GetSpeakers()
        interface = devices.Activate(IAudioEndpointVolume._iid_, CLSCTX_ALL, None)
        volume = cast(interface, POINTER(IAudioEndpointVolume))
        volume.SetMute(1, None)

    elif cmd == 'sound_on':

        devices = AudioUtilities.GetSpeakers()
        interface = devices.Activate(IAudioEndpointVolume._iid_, CLSCTX_ALL, None)
        volume = cast(interface, POINTER(IAudioEndpointVolume))
        volume.SetMute(0, None)

        cached_response("ok")

    elif cmd == 'thanks':

        cached_response("thanks")

    elif cmd == 'stupid':

        cached_response("stupid")

    elif cmd == 'gaming_mode_on':

        cached_response("ok")
        subprocess.check_call([f'{CDIR}\\custom-commands\\Switch to gaming mode.exe'])
        cached_response("ready")

    elif cmd == 'gaming_mode_off':

        cached_response("ok")
        subprocess.check_call([f'{CDIR}\\custom-commands\\Switch back to workspace.exe'])
        cached_response("ready")

    elif cmd == 'switch_to_headphones':

        cached_response("ok")
        subprocess.check_call([f'{CDIR}\\custom-commands\\Switch to headphones.exe'])
        time.sleep(0.5)
        cached_response("ready")

    elif cmd == 'switch_to_dynamics':

        cached_response("ok")
        subprocess.check_call([f'{CDIR}\\custom-commands\\Switch to dynamics.exe'])
        time.sleep(0.5)
        cached_response("ready")

    elif cmd == 'off':
        
        cached_response("off")
        exit(0)


def gpt_answer(message):
    model_engine = "text-davinci-003"
    max_tokens = 128  # default 1024
    prompt = t.translate(message, targetlang="en")
    completion = openai.Completion.create(
        engine=model_engine,
        prompt=prompt.text,
        max_tokens=max_tokens,
        temperature=0.5,
        top_p=1,
        frequency_penalty=0,
        presence_penalty=0
    )

    translated_result = t.translate(completion.choices[0].text, targetlang="ru")
    return translated_result.text