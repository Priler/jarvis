import speech_recognition as sr
from rich import print
from utils.benchmark import Benchmark

# import speech_recognition as sr
# for index, name in enumerate(sr.Microphone.list_microphone_names()):
#     print("Microphone with name \"{1}\" found for `Microphone(device_index={0})`".format(index, name))
# exit(1)

# obtain audio from the microphone
r = sr.Recognizer()
r.pause_threshold = 0.5
with sr.Microphone(device_index=2) as source:
    print("Say something!")
    audio = r.listen(source)

bench = Benchmark()

# recognize speech using Google Speech Recognition
try:
    # for testing purposes, we're just using the default API key
    # to use another API key, use `r.recognize_google(audio, key="GOOGLE_SPEECH_RECOGNITION_API_KEY")`
    # instead of `r.recognize_google(audio)`\
    while True:
        bench.start()
        recognized_text = r.recognize_google(audio)
        end_time = bench.end()
        print(f"[light_sea_green]Google Speech Recognition thinks you said[/]: [dodger_blue1]{recognized_text}[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")
        print()
except sr.UnknownValueError:
    print("Google Speech Recognition could not understand audio")
except sr.RequestError as e:
    print("Could not request results from Google Speech Recognition service; {0}".format(e))