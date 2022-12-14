import torch
import sounddevice as sd
import speech_recognition as sr
import time
import numpy
from glob import glob

device = torch.device('cpu')
model, decoder, utils = torch.hub.load(repo_or_dir='snakers4/silero-models',
                                       model='silero_stt',
                                       language='en', # en, ru
                                       device=device)
(read_batch, split_into_batches,
 read_audio, prepare_model_input) = utils

def callback(_r, audio):
    try:
        # CONVERT raw wav data to NumPy array
        # wav_raw = audio.get_wav_data()
        # data_s16 = numpy.frombuffer(wav_raw, dtype=numpy.int16, count=len(wav_raw) // 2, offset=0)
        # np_audio = data_s16 * 0.5 ** 15

        # Play it via sounddevice
        #sd.play(np_audio, m.SAMPLE_RATE)
        #time.sleep(len(np_audio) / m.SAMPLE_RATE)
        #sd.stop()

        print("Распознание ...")

        # TODO: fix crutch, pass audio data directly as a model input of Silero STT
        with open('speech.wav', 'wb') as f:
            f.write(audio.get_wav_data())

        test_files = glob('speech.wav')
        batches = split_into_batches(test_files, batch_size=10)
        input = prepare_model_input(read_batch(batches[0]),
                                    device=device)

        output = model(input)
        for example in output:
            print(decoder(example.cpu()))

        # voice = recognizer.recognize_google(audio, language="ru-RU").lower()
        # print("[log] Распознано: " + voice)

    except sr.UnknownValueError:
        print("[log] Голос не распознан!")


# запуск
r = sr.Recognizer()
r.pause_threshold = 0.5
m = sr.Microphone(device_index=1)

with m as source:
    r.adjust_for_ambient_noise(source)

stop_listening = r.listen_in_background(m, callback)
while True: time.sleep(0.1)
