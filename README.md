# JARVIS Voice Assistant

`Jarvis` - is a voice assistant made as an experiment using neural networks for things like **STT/TTS/Wake Word/NLU** etc.

The main project challenges we try to achieve is:
 - 100% offline *(no cloud)*
 - Open source *(full transparency)*
 - No data collection *(we respect your privacy)*

Our backend stack is 🦀 **[Rust](https://www.rust-lang.org/)** with ❤️ **[Tauri](https://tauri.app/)**.
For the frontend we use ⚡️ **[Vite](https://vitejs.dev/)** + 🛠️ **[Svelte](https://svelte.dev/)**.

*Other libraries, tools and packages can be found in source code.*

 [Silero](https://github.com/snakers4/silero-models) & [Vosk](https://pypi.org/project/vosk/).  
Later on [Picovoice Porcupine Wake Word Detection](https://picovoice.ai/platform/porcupine/) & [ChatGPT](https://chat.openai.com/) was added.

Hi! I'm your first Markdown file in **StackEdit**. If you want to learn about StackEdit, you can read me. If you want to play with Markdown, you can edit me. Once you have finished with me, you can create new files by opening the **file explorer** on the left corner of the navigation bar.


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

Currently, only Russian language is supported.
But soon, Ukranian and English will be added.

## How to build?

Nothing special was used to build this project.
You need only Rust and NodeJS installed on your system.
Other than that, all you need is to install all the dependencies and then compile the code with `cargo tauri build` command.
Or run dev with `cargo tauri dev`.

## Author

Abraham Tugalov

## License

[Attribution-NonCommercial-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-nc-sa/4.0/)
See LICENSE.txt file for more details.