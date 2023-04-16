import logging

from app import tts
from app.stt import Dispatcher
from app.recorder import Recorder

from vosk import KaldiRecognizer, Model, SetLogLevel


log = logging.getLogger('jarvis.main')

def main():

    log.info('Starting up...')

    tts.cached_response('run')
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    SetLogLevel(-1)
    vosk = KaldiRecognizer(Model("model_small"), 16000)
    recorder = Recorder(9)    

    dp = Dispatcher(recorder, vosk)
    dp.start()


if __name__ == '__main__':

    main()