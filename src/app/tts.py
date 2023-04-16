import os
import re
import torch
import random
import sounddevice

from pydub import AudioSegment, playback


device = torch.device('cpu')
model, _ = torch.hub.load(
    repo_or_dir='snakers4/silero-models',
    model='silero_tts',
    language='ru',
    speaker='ru_v3',
)
model.to(device)

def from_wav(path: str) -> None:
    """
    Play an audiofile

    :param str path: File path
    """
    
    with open(path, 'rb') as file:

        audio = AudioSegment.from_wav(file)
    
    playback.play(audio)
    

def cached_response(keyword: str, basedir: str='src', ext: str='wav') -> None:

    files = os.listdir(basedir)
    regexp = re.compile(
        r'%s\d{0,}\.%s' % (
            keyword, ext,
        ),
    )
    filename = random.choice([
        filename for filename in files
        if re.match(regexp, filename)
    ])

    return from_wav(
        os.path.join(basedir, filename),
    )


def model_response(text: str):

    audio: torch.Tensor = model.apply_tts(
        text=text + "..",
        speaker='aidar',
        sample_rate=24000,
        put_accent=True,
        put_yo=True,
    )
    sounddevice.play(audio, 24000, blocking=True)
