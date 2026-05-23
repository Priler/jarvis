# Simple python script used to
# copy some libraries to the "target" directory
# after Rust build
# Note that Rust build should be run via "cargo make <cmd>" command
# in order to automate all the compile process
import hashlib
import os
from pathlib import Path
import shutil
import sys
import filecmp

# some config vars
# format: (source, destination_name)
SOURCE = (
    ("resources/commands/", "resources/commands/"),
    ("resources/vosk/", "resources/vosk/"),
    ("resources/lib/", "lib/"),
    ("resources/keywords/", "resources/keywords/"),
    ("resources/rustpotter/", "resources/rustpotter/"),
    ("resources/sound/", "resources/sound/"),
    ("resources/models/", "resources/models/"),

    # vosk
    ("lib/windows/amd64/libgcc_s_seh-1.dll", None),
    ("lib/windows/amd64/libstdc++-6.dll", None),
    ("lib/windows/amd64/libvosk.dll", None),
    ("lib/windows/amd64/libvosk.lib", None),
    ("lib/windows/amd64/libwinpthread-1.dll", None),

    # pvrecorder
    ("lib/windows/amd64/libpv_recorder.dll", None),
)

TARGET_DIRS = (
    "target/debug",
    "target/release"
)

# Resolve all paths relative to this script's directory (project root),
# regardless of the working directory from which the script is invoked.
SCRIPT_DIR = Path(__file__).parent.resolve()
ABS_PATH = str(SCRIPT_DIR) + "/"

# SHA-256 manifest for Windows DLLs.
# Regenerate with: python -c "import hashlib,os; print(hashlib.sha256(open(path,'rb').read()).hexdigest())"
# If a DLL hash mismatches, the build is aborted — do NOT distribute unverified DLLs.
KNOWN_DLL_HASHES = {
    "libgcc_s_seh-1.dll":  "201bf8a54ebb490f93ea4beb20740a483bf67036b90b5720f32d0c746a72d1e9",
    "libstdc++-6.dll":     "a1d96a848a567d7657e152cfe92e2fd62da412968cc11ec8bdb04bf20ebc5747",
    "libvosk.dll":         "9331c2f6a32cf77141af27c9750e79532718f51bbc2e3cea3c60f14ca4251e4e",
    "libwinpthread-1.dll": "92caeb8a4ea281c674003462a233f3b445b685b0a6f93eb11254feda2dda9cde",
    "libpv_recorder.dll":  "055e91935a3c9a1595f63d0b3d4a33b61aa49a07893f5d39746f11070497e907",
}


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_dll(src_path: str) -> bool:
    """Return True if the DLL passes the known-hash check (or is not in the manifest)."""
    name = os.path.basename(src_path)
    expected = KNOWN_DLL_HASHES.get(name)
    if expected is None:
        return True
    actual = sha256_file(src_path)
    if actual != expected:
        print(f"[!] SHA-256 MISMATCH for {name}:")
        print(f"    expected: {expected}")
        print(f"    actual:   {actual}")
        return False
    return True

# flags
force_overwrite = "--force" in sys.argv
sync_mode = "--sync" in sys.argv

if sync_mode:
    print("[*] Sync mode: will update changed files and remove orphans")


def files_differ(src, dst):
    """check if two files differ by size or content"""
    if not os.path.isfile(dst):
        return True
    # quick check: size
    if os.path.getsize(src) != os.path.getsize(dst):
        return True
    # compare modification time (src newer = needs update)
    if os.path.getmtime(src) > os.path.getmtime(dst):
        return True
    return False


def sync_directory(src_dir, dst_dir):
    """sync dst_dir to match src_dir: copy new/changed, remove orphans"""
    copied = 0
    updated = 0
    removed = 0

    # walk source, copy new/changed files
    for root, dirs, files in os.walk(src_dir):
        rel_root = os.path.relpath(root, src_dir)
        dst_root = os.path.join(dst_dir, rel_root) if rel_root != "." else dst_dir

        # ensure dir exists
        os.makedirs(dst_root, exist_ok=True)

        for f in files:
            src_file = os.path.join(root, f)
            dst_file = os.path.join(dst_root, f)

            if not os.path.exists(dst_file):
                shutil.copy2(src_file, dst_file)
                copied += 1
            elif files_differ(src_file, dst_file):
                shutil.copy2(src_file, dst_file)
                updated += 1

    # walk destination, remove files/dirs not in source
    for root, dirs, files in os.walk(dst_dir, topdown=False):
        rel_root = os.path.relpath(root, dst_dir)
        src_root = os.path.join(src_dir, rel_root) if rel_root != "." else src_dir

        for f in files:
            dst_file = os.path.join(root, f)
            src_file = os.path.join(src_root, f)
            if not os.path.exists(src_file):
                os.remove(dst_file)
                print(f"  [-] Removed orphan file: {os.path.relpath(dst_file, dst_dir)}")
                removed += 1

        for d in dirs:
            dst_sub = os.path.join(root, d)
            src_sub = os.path.join(src_root, d)
            if not os.path.exists(src_sub):
                shutil.rmtree(dst_sub)
                print(f"  [-] Removed orphan dir: {os.path.relpath(dst_sub, dst_dir)}")
                removed += 1

    return copied, updated, removed


def sync_file(src_path, dst_path):
    """sync a single file, returns (copied, updated)"""
    if not os.path.exists(dst_path):
        os.makedirs(os.path.dirname(dst_path), exist_ok=True)
        shutil.copy2(src_path, dst_path)
        return 1, 0
    elif files_differ(src_path, dst_path):
        shutil.copy2(src_path, dst_path)
        return 0, 1
    return 0, 0


for tdir in TARGET_DIRS:
    tdir = ABS_PATH + tdir

    if not Path(tdir).is_dir():
        print("Skipping target, not a directory: ", tdir)
        continue

    for entry in SOURCE:
        if isinstance(entry, tuple):
            src, dest_name = entry
        else:
            src, dest_name = entry, None

        src_path = ABS_PATH + src

        if os.path.isdir(src_path):
            target_name = dest_name if dest_name else os.path.basename(src.rstrip('/'))
            full_target_dir_path = os.path.join(tdir, target_name)

            if sync_mode:
                # sync: update changed, add new, remove orphans
                if os.path.isdir(full_target_dir_path):
                    c, u, r = sync_directory(src_path, full_target_dir_path)
                    if c or u or r:
                        print(f"[~] Synced: {src} -> {target_name} (+{c} new, ~{u} updated, -{r} removed)")
                    else:
                        print(f"[=] Up to date: {src} -> {target_name}")
                else:
                    shutil.copytree(src_path, full_target_dir_path)
                    print("[+] Directory copied: ", src, "->", target_name)

            elif os.path.isdir(full_target_dir_path):
                if force_overwrite:
                    shutil.rmtree(full_target_dir_path)
                    shutil.copytree(src_path, full_target_dir_path)
                    print("[+] Directory overwritten: ", src, "->", target_name)
                else:
                    print("[-] Directory already exists, skipping: ", src, "->", target_name)
            else:
                shutil.copytree(src_path, full_target_dir_path)
                print("[+] Directory copied: ", src, "->", target_name)

        elif os.path.isfile(src_path):
            # SHA-256 integrity check before copying DLLs.
            if not verify_dll(src_path):
                print(f"[!] Aborting: DLL integrity check failed for {src}")
                sys.exit(1)

            target_name = dest_name if dest_name else os.path.basename(src)
            full_target_file_path = os.path.join(tdir, target_name)

            if sync_mode:
                c, u = sync_file(src_path, full_target_file_path)
                if c:
                    print("[+] File copied: ", src, "->", target_name)
                elif u:
                    print("[~] File updated: ", src, "->", target_name)
                else:
                    print("[=] Up to date: ", src, "->", target_name)

            elif os.path.isfile(full_target_file_path):
                if force_overwrite:
                    os.remove(full_target_file_path)
                    shutil.copy(src_path, full_target_file_path)
                    print("[+] File overwritten: ", src, "->", target_name)
                else:
                    print("[-] File already exists, skipping: ", src, "->", target_name)
            else:
                shutil.copy(src_path, full_target_file_path)
                print("[+] File copied: ", src, "->", target_name)
        else:
            print("[?] Unknown entity to copy: ", src)

    print("Post compile build done.")