import vosk
import sys
import sounddevice as sd
import json

model = vosk.Model("model_small")
samplerate = 16000
device = 2
blocksize = 16000

def va_listen(callback):
    def callback_wrapper(indata, frames, time, status):
        if status:
            print(status, file=sys.stderr)
        result = rec.AcceptWaveform(indata)
        if result:
            text = json.loads(rec.Result())["text"]
            callback(text)

    with sd.RawInputStream(samplerate=samplerate, blocksize=blocksize, device=device, dtype='int16',
                           channels=1, callback=callback_wrapper):
        rec = vosk.KaldiRecognizer(model, samplerate)
        while True:
            pass
