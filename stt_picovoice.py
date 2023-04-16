import time

import pvporcupine
from pvrecorder import PvRecorder
from rich import print
from utils.benchmark import Benchmark
import keyboard
from utils.time import sleep

porcupine = pvporcupine.create(
    access_key='H2JhXpfLdGYG2wMqvc61tipo0uKZQvwkEfA26CAQQe5n1y7zfZGneQ==',
    # keywords=['picovoice', 'bumblebee', 'aloha mora'],
    keyword_paths=["./pv_custom_keywords/aloha-mora_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/Expellee-aramus_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/Lumas_en_windows_v2_1_0.ppn",

                    "./pv_custom_keywords/Proud-to-go_en_windows_v2_1_0.ppn",
                    "./pv_custom_keywords/A-Knocks_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/In-St-Your_en_windows_v2_1_0.ppn",

                   "./pv_custom_keywords/The-pool-so_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/Lady-also_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/R-Vail-leo_en_windows_v2_1_0.ppn",

                   "./pv_custom_keywords/Conceal_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/terrifical-status_en_windows_v2_1_0.ppn",
                   "./pv_custom_keywords/Disillusion_en_windows_v2_1_0.ppn",

                   "./pv_custom_keywords/Can-finger_en_windows_v2_1_0.ppn"
                   ],
    sensitivities=[1] * 13
)

# `-1` is the default input audio device.
recorder = PvRecorder(device_index=0, frame_length=porcupine.frame_length)
recorder.start()
print('Using device: %s' % recorder.selected_device)

bench = Benchmark()

while True:
    bench.start()
    pcm = recorder.read()
    keyword_index = porcupine.process(pcm)
    end_time = bench.end()

    if keyword_index == 0:
        print("[gold1]Aloha mora[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("f")
    elif keyword_index == 1:
        print("[bright_red]Expelliarmus[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F1")
        sleep(0.05)
        keyboard.press_and_release("2")
    elif keyword_index == 2:
        print("[gold1]Lumos[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F1")
        sleep(0.05)
        keyboard.press_and_release("1")

    elif keyword_index == 3:
        print("[dodger_blue2]Protego[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press("q")
        sleep(1/2)
        keyboard.release("q")

    elif keyword_index == 4:
        print("[gold1]Nox[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F1")
        sleep(0.05)
        keyboard.press_and_release("1")

    elif keyword_index == 5:
        print("[bright_red]Incendio[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F2")
        sleep(0.05)
        keyboard.press_and_release("1")

    elif keyword_index == 6:
        print("[dodger_blue2]Depulso[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F1")
        sleep(0.05)
        keyboard.press_and_release("4")

    elif keyword_index == 7:
        print("[gold1]Levioso[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F1")
        sleep(0.05)
        keyboard.press_and_release("3")

    elif keyword_index == 8:
        print("[dodger_blue2]Revelio[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("R")

    elif keyword_index == 9:
        print("[dodger_blue2]Accio[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F2")
        sleep(0.05)
        keyboard.press_and_release("3")

    elif keyword_index == 10:
        print("[bright_red]Petrificus Totalus[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("f")

    elif keyword_index == 11:
        print("[dodger_blue2]Disillusionment Spell[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F2")
        sleep(0.05)
        keyboard.press_and_release("2")

    elif keyword_index == 12:
        print("[bright_red]Confringo Spell[/]")
        print(f"[grey37]This took[/] [red]{end_time[1]}[/]")

        keyboard.press_and_release("F2")
        sleep(0.05)
        keyboard.press_and_release("4")

    #if keyword_index >= 0:
    #    break

porcupine.delete()
