import logging
from queue import Queue
from sounddevice import RawInputStream


log = logging.getLogger('app.recorder')

class Recorder(object):

    def __init__(self, device: int, sample_rate: int=16_000):

        self.queue = Queue()
        self.stream = RawInputStream(
            samplerate=sample_rate,
            blocksize=8_000,
            device=device,
            dtype='int16',
            channels=1,
            callback=self._write_to_queue,
        )

    def _write_to_queue(self, data, _, __, status):

        if status:

            log.error(str(status))

        self.queue.put(bytes(data))

    def __call__(self):

        return self.stream
