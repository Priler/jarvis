# JARVIS Voice Assistant

`Jarvis` - is a voice assistant made as an experiment using neural networks for things like **STT/TTS/Wake Word/NLU** etc.

The main project challenges we try to achieve is:
 - 100% offline *(no cloud)*
 - Open source *(full transparency)*
 - No data collection *(we respect your privacy)*

Our backend stack is 🦀 **[Rust](https://www.rust-lang.org/)** with ❤️ **[Tauri](https://tauri.app/)**.<br>
For the frontend we use ⚡️ **[Vite](https://vitejs.dev/)** + 🛠️ **[Svelte](https://svelte.dev/)**.

*Other libraries, tools and packages can be found in source code.*

 [Silero](https://github.com/snakers4/silero-models) & [Vosk](https://pypi.org/project/vosk/).  
Later on [Picovoice Porcupine Wake Word Detection](https://picovoice.ai/platform/porcupine/) & [ChatGPT](https://chat.openai.com/) was added.

## Neural Networks

This are the neural networks we are currently using:

 - Speech-To-Text
	 - [Vosk Speech Recognition Toolkit](https://github.com/alphacep/vosk-api) via [Vosk-rs](https://github.com/Bear-03/vosk-rs)
 - Text-To-Speech
	 - [~~Silero TTS~~](https://github.com/snakers4/silero-models) (currently not used)
	 - [~~Coqui TTS~~](https://github.com/coqui-ai/TTS) (currently not used)
 - Wake Word
	 - [Picovoice Porcupine](https://github.com/Picovoice/porcupine) via [official SDK](https://github.com/Picovoice/porcupine#rust)
	 - [~~Rustpotter~~](https://github.com/GiviMAD/rustpotter) (coming soon)
 - NLU
	 - Nothing yet.

## Supported Languages

Currently, only Russian language is supported.<br>
But soon, Ukranian and English will be added.

## How to build?

Nothing special was used to build this project.<br>
You need only Rust and NodeJS installed on your system.<br>
Other than that, all you need is to install all the dependencies and then compile the code with `cargo tauri build` command.<br>
Or run dev with `cargo tauri dev`.

## Author

## Python version?
Old version of Jarvis was build with Python.<br>
The last Python version commit can be found [here](https://github.com/Priler/jarvis/tree/943efbfbdb8aeb5889fa5e2dc7348ca4ea0b81df).

Abraham Tugalov

## License

[Attribution-NonCommercial-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-nc-sa/4.0/)<br>
See LICENSE.txt file for more details.