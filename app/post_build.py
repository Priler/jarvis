# Simple python script used to
# copy some libraries to the "target" directory
# after Rust build

# Note that Rust build should be run via "cargo make <cmd>" command
# in order to automate all the compile process

import os
from pathlib import Path
import shutil

# some config vars
SOURCE = (
    "commands/",
    "vosk/",
    "lib/",
    "keywords/",
    "libgcc_s_seh-1.dll",
    "libstdc++-6.dll",
    "libvosk.dll",
    "libvosk.lib",
    "libwinpthread-1.dll"
)

TARGET_DIRS = (
    "target/debug",
    "target/release"
)

ABS_PATH = os.getcwd() + "/"

for tdir in TARGET_DIRS:
    tdir = ABS_PATH + tdir

    if not Path(tdir).is_dir():
        print("Skipping target, not a directory: ", tdir)
        continue

    # copy lib files
    for src in SOURCE:
        if os.path.isdir(ABS_PATH + src):
            # copy the whole directory
            full_target_dir_path = os.path.join(tdir, src)

            if os.path.isdir(full_target_dir_path):
                print("[-] Directory already exists, skipping: ", src)
            else:
                shutil.copytree(ABS_PATH + src, os.path.join(tdir, src))

                print("[+] Directory copied: ", src)
        elif os.path.isfile(ABS_PATH + src):
            # copy file
            full_target_file_path = os.path.join(tdir, src)
            if os.path.isfile(full_target_file_path):
                print("[-] File already exists, skipping: ", src)
            else:
                shutil.copy(ABS_PATH + src, tdir)
                print("[+] File copied: ", src)
        else:
            print("[?] Unknown entity to copy: ", src)


    print("Post compile build done.")