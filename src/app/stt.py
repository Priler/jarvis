import json
import time
import yaml
import config
import logging

from .tts import cached_response, model_response
from .commands import gpt_answer
from .recorder import Recorder

from fuzzywuzzy import fuzz
from vosk import KaldiRecognizer


log = logging.getLogger('jarvis.stt')
COMMANDS = yaml.safe_load(open('responses.yaml'))

class Dispatcher(object):

    _last_action = 0

    def __init__(self, recorder: Recorder, vosk: KaldiRecognizer):

        self.recorder = recorder
        self.vosk = vosk

    def start(self):

        log.info('Started recording')

        with self.recorder():

            while True:

                data = self.recorder.queue.get()

                if not self.vosk.AcceptWaveform(data):

                    continue

                text = json.loads(self.vosk.Result())['text']

                if self._last_action < time.time() - 10:

                    self.check_input(text)

                else:

                    self.dispatch(text)


    def check_input(self, text: str):

        if not text:

            return
        
        for word in text.split():
            
            if fuzz.ratio(word, 'джарвис') >= 60:

                cached_response('ok')
                self._last_action = time.time()
                return self.dispatch(text)
                

    def dispatch(self, text: str):

        if text and self.va_respond(text):

            self._last_action = time.time()


    def va_respond(self, voice: str):
        
        log.info('Распознано: ')

        cmd = self.recognize_cmd(voice)

        if len(cmd['cmd'].strip()) <= 0:
            
            return False
        
        elif cmd['percent'] < 70 or cmd['cmd'] not in config.VA_CMD_LIST.keys():
            
            if fuzz.ratio(voice.join(voice.split()[:1]).strip(), "скажи") > 75:
                
                gpt_result = gpt_answer(voice)
                model_response(gpt_result)
                
                return False
            
            else:
                
                cached_response("not_found")

            return False
        
        else:

            self.execute_cmd()
            return True


    def respond(self, phrase: str):

        self.recorder.stream.stop()
        cached_response(phrase)
        self.recorder.stream.stop()


    @staticmethod
    def filter_cmd(raw_voice: str):
        cmd = raw_voice

        for x in config.VA_ALIAS:
            cmd = cmd.replace(x, "").strip()

        for x in config.VA_TBR:
            cmd = cmd.replace(x, "").strip()

        return cmd


    @classmethod
    def recognize_cmd(cls, cmd: str):

        cmd = cls.filter_cmd(cmd)
        
        rc = {'cmd': '', 'percent': 0}
        for c, v in COMMANDS.items():

            for x in v:
                vrt = fuzz.ratio(cmd, x)
                if vrt > rc['percent']:
                    rc['cmd'] = c
                    rc['percent'] = vrt

        return rc
