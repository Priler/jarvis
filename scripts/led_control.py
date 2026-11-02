import subprocess
import selectors
import sys
import time
from pathlib import Path

DEVICE_MAC = "BE:68:89:0F:0F:00"
DEVICE_NAME = "ELK-BLEDDM CD"
DEVICE_PATH = DEVICE_MAC.replace(":", "_")
DEVICE_OBJECT = f"/org/bluez/hci0/dev_{DEVICE_PATH}"
CACHE_FILE = Path("/tmp/jarvis-led-char-path")
TARGET_CHARS = {
    "0000fff3-0000-1000-8000-00805f9b34fb",
    "0000fff4-0000-1000-8000-00805f9b34fb",
    "0000ffe1-0000-1000-8000-00805f9b34fb",
}
SCAN_TIMEOUT = 20
CONNECT_TIMEOUT = 12


def run_cmd(cmd, timeout=20):
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    return result.returncode, result.stdout.strip(), result.stderr.strip()


def bt_connect():
    proc = subprocess.Popen(
        ["bluetoothctl"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    log = []

    def send_line(line):
        proc.stdin.write(line + "\n")
        proc.stdin.flush()

    selector = selectors.DefaultSelector()
    selector.register(proc.stdout, selectors.EVENT_READ)

    try:
        send_line("scan on")

        found = False
        deadline = time.monotonic() + SCAN_TIMEOUT
        while time.monotonic() < deadline:
            for key, _ in selector.select(timeout=0.5):
                line = key.fileobj.readline()
                if not line:
                    continue
                line = line.strip()
                log.append(line)
                if DEVICE_MAC in line or DEVICE_NAME in line:
                    found = True
                    break
            if found:
                break

        if not found:
            raise RuntimeError(
                f"Не нашёл подсветку {DEVICE_NAME} / {DEVICE_MAC} за {SCAN_TIMEOUT} сек. "
                "Проверь питание, расстояние и что телефон не подключен к ленте."
            )

        connected = False
        deadline = time.monotonic() + CONNECT_TIMEOUT
        next_attempt = 0.0

        while time.monotonic() < deadline and not connected:
            now = time.monotonic()
            if now >= next_attempt:
                send_line(f"connect {DEVICE_MAC}")
                next_attempt = now + 2.0

            for key, _ in selector.select(timeout=0.5):
                line = key.fileobj.readline()
                if not line:
                    continue
                line = line.strip()
                log.append(line)
                if "Connection successful" in line or "Connected: yes" in line:
                    connected = True
                    break

        if not connected:
            raise RuntimeError(
                f"Не удалось подключиться к подсветке {DEVICE_MAC}. "
                "Проверь, что лента включена, рядом и не подключена к телефону."
            )

        send_line("scan off")
        send_line("exit")
        proc.wait(timeout=5)
    except Exception:
        try:
            send_line("scan off")
            send_line("exit")
        except Exception:
            pass
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()

        if log:
            print("\n".join(log[-40:]), file=sys.stderr)
        raise
    finally:
        selector.close()

    # Give BlueZ a short moment to expose GATT objects on D-Bus.
    time.sleep(0.2)


def bluez_tree_paths():
    code, out, err = run_cmd(["busctl", "tree", "org.bluez"], timeout=10)
    if code != 0:
        raise RuntimeError(err or out or "Не удалось получить дерево BlueZ")

    paths = []
    for line in out.splitlines():
        line = line.strip()
        start = line.find(DEVICE_OBJECT)
        if start < 0:
            continue
        paths.append(line[start:].strip())
    return paths


def get_property(path, interface, prop):
    code, out, err = run_cmd(
        ["busctl", "get-property", "org.bluez", path, interface, prop],
        timeout=10,
    )
    if code != 0:
        return None
    return out


def find_write_characteristic():
    seen = []

    for path in bluez_tree_paths():
        if "/char" not in path:
            continue

        uuid_raw = get_property(path, "org.bluez.GattCharacteristic1", "UUID")
        if not uuid_raw:
            continue

        uuid = uuid_raw.split('"')[1].lower() if '"' in uuid_raw else uuid_raw.lower()
        flags_raw = get_property(path, "org.bluez.GattCharacteristic1", "Flags") or ""
        seen.append(f"{path} uuid={uuid} flags={flags_raw}")

        if uuid not in TARGET_CHARS:
            continue

        if "write" in flags_raw:
            CACHE_FILE.write_text(path)
            return path

    details = "\n".join(seen) if seen else "GATT characteristics не найдены"
    raise RuntimeError(f"Не нашёл writable GATT characteristic для подсветки:\n{details}")


def cached_write_characteristic():
    try:
        path = CACHE_FILE.read_text().strip()
    except OSError:
        return None

    if not path.startswith(DEVICE_OBJECT):
        return None

    return path


def write_value(char_path, bytes_list, write_type="command"):
    cmd = [
        "busctl",
        "call",
        "org.bluez",
        char_path,
        "org.bluez.GattCharacteristic1",
        "WriteValue",
        "aya{sv}",
        str(len(bytes_list)),
        *[str(b) for b in bytes_list],
        "1",
        "type",
        "s",
        write_type,
    ]
    code, out, err = run_cmd(cmd, timeout=10)
    return code == 0, out, err


def send(bytes_list):
    char_path = cached_write_characteristic()
    if char_path:
        ok, out, err = write_value(char_path, bytes_list, "command")
        if ok:
            print("OK")
            return

    bt_connect()
    char_path = cached_write_characteristic() or find_write_characteristic()

    ok, out, err = write_value(char_path, bytes_list, "command")
    if not ok:
        ok, out, err = write_value(char_path, bytes_list, "request")

    if not ok:
        raise RuntimeError(err or out or "Не удалось отправить команду")

    print("OK")


def power(on):
    if on:
        send([0x7E, 0x04, 0x04, 0xF0, 0x00, 0x01, 0xFF, 0x00, 0xEF])
    else:
        send([0x7E, 0x04, 0x04, 0x00, 0x00, 0x00, 0xFF, 0x00, 0xEF])


def color(r, g, b):
    r = max(0, min(255, r))
    g = max(0, min(255, g))
    b = max(0, min(255, b))
    send([0x7E, 0x00, 0x05, 0x03, r, g, b, 0x00, 0xEF])


def brightness_percent(percent):
    percent = max(0, min(100, percent))
    value = round(percent * 255 / 100)
    send([0x7E, 0x01, 0x01, value, 0x00, 0x00, 0x00, 0x00, 0xEF])


def main():
    if len(sys.argv) < 2:
        print("usage: led_control.py <command>", file=sys.stderr)
        sys.exit(1)

    cmd = sys.argv[1].lower()

    if cmd in ("on", "вкл", "включить"):
        power(True)
    elif cmd in ("off", "выкл", "выключить"):
        power(False)
    elif cmd in ("red", "красный"):
        color(255, 0, 0)
    elif cmd in ("green", "зеленый", "зелёный"):
        color(0, 255, 0)
    elif cmd in ("blue", "синий"):
        color(0, 0, 255)
    elif cmd in ("white", "белый"):
        color(255, 255, 255)
    elif cmd in ("yellow", "желтый", "жёлтый"):
        color(255, 255, 0)
    elif cmd in ("purple", "фиолетовый"):
        color(255, 0, 255)
    elif cmd in ("cyan", "голубой"):
        color(0, 255, 255)
    elif cmd in ("brightness", "яркость") and len(sys.argv) >= 3:
        brightness_percent(int(sys.argv[2]))
    elif cmd == "rgb" and len(sys.argv) >= 5:
        color(int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]))
    else:
        print("Неизвестная команда", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
