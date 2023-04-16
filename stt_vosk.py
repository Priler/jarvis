import vosk
import queue
import json
import sys
import sounddevice as sd
from rich import print
from utils.benchmark import Benchmark

# import speech_recognition as sr
# for index, name in enumerate(sr.Microphone.list_microphone_names()):
#     print("Microphone with name \"{1}\" found for `Microphone(device_index={0})`".format(index, name))
# exit(1)

bench = Benchmark()


def va_respond(voice: str):
    print(f"[light_sea_green]Vosk thinks you said[/]: [dodger_blue1]{voice}[/]")


model = vosk.Model("model_small")
samplerate = 16000
device = 2

q = queue.Queue()


def q_callback(indata, frames, time, status):
    if status:
        print(status, file=sys.stderr)
    q.put(bytes(indata))


def va_listen(callback):
    with sd.RawInputStream(samplerate=samplerate, blocksize=8000, device=device, dtype='int16',
                           channels=1, callback=q_callback):

        rec = vosk.KaldiRecognizer(model, samplerate)
        while True:
            bench.start()
            data = q.get()
            if rec.AcceptWaveform(data):
                callback(json.loads(rec.Result())["text"])
                end_time = bench.end()
                print(f"[grey37]This took[/] [red]{end_time[1]}[/]")
            #else:
            #    print(rec.PartialResult())

# начать прослушивание команд
va_listen(va_respond)