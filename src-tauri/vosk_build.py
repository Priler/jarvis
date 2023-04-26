# Simple python script used to
# copy Vosk libraries to the "target" directory
# after Rust build

# Note that Rust build should be run via "cargo make <cmd>" command
# in order to automate all the compile process

import os
from pathlib import Path
import shutil

# some config vars
VOSK_LIBRARIES_PATH = "D:/Rust/vosk"
TARGET_DIRS = (
    os.getcwd() + "/target/debug",
    os.getcwd() + "/target/release"
)

for tdir in TARGET_DIRS:
    if not Path(tdir).is_dir():
        continue

    vosk_lib_testfile = Path(tdir + "/libvosk.dll")

    if vosk_lib_testfile.is_file():
        # skip
        print("[Vosk] library files already exist in " + tdir)
    else:
        # copy lib files
        src_files = os.listdir(VOSK_LIBRARIES_PATH)
        for file_name in src_files:
            full_file_name = os.path.join(VOSK_LIBRARIES_PATH, file_name)
            if os.path.isfile(full_file_name):
                shutil.copy(full_file_name, tdir)
        
        print("[Vosk] library files was copied to " + tdir)