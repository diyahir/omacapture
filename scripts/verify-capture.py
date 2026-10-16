#!/usr/bin/env python3
"""Run with dbus-run-session -- python scripts/verify-capture.py BINARY WLPTR on Hyprland."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def wait_for(condition):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(0.02)
    raise AssertionError("Capture did not reach the expected state within 10 seconds")


def main():
    binary, pointer = (str(Path(arg).resolve()) for arg in sys.argv[1:])
    monitors = json.loads(subprocess.check_output(["hyprctl", "-j", "monitors"]))
    assert monitors, "An active Hyprland monitor is required"
    with tempfile.TemporaryDirectory(prefix="omacapture-verify-") as directory:
        root = Path(directory)
        config = root / "config" / "omacapture"
        config.mkdir(parents=True)
        captures = root / "captures"
        captures.mkdir()
        (config / "config.toml").write_text(
            f'[general]\nsave_folder = {json.dumps(str(captures))}\ndelay_ms = 300\n'
            'sound = false\nnotifications = false\n'
            '[post_capture.area]\nsave = true\ncopy = false\nquick_access = false\n'
            '[history]\nenabled = false\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
        with (root / "daemon.log").open("w+") as log:
            daemon = subprocess.Popen([binary, "daemon"], env=env, stdout=log, stderr=log)
            try:
                def ready():
                    assert daemon.poll() is None, "Capture daemon exited"
                    log.seek(0)
                    return "daemon running" in log.read()

                wait_for(ready)

                def overlays():
                    layers = json.loads(subprocess.check_output(["hyprctl", "-j", "layers"]))
                    return sum(
                        layer["pid"] == daemon.pid and layer["namespace"] == "omacapture-overlay"
                        for monitor in layers.values()
                        for level in monitor["levels"].values()
                        for layer in level
                    )

                clients = [subprocess.Popen([binary, "area"], env=env) for _ in range(2)]
                for client in clients:
                    assert client.wait(timeout=10) == 0
                wait_for(lambda: overlays() > 0)
                time.sleep(0.2)
                assert overlays() == len(monitors), "Overlapping requests created multiple selectors"
                subprocess.run([binary, "area"], env=env, check=True)
                time.sleep(0.8)
                assert overlays() == len(monitors), "A request during selection created another picker"
                subprocess.run([pointer, "drag", "200", "300", "600", "500"], check=True)
                wait_for(lambda: len(list(captures.glob("*.png"))) == 1)
                time.sleep(0.2)
                assert overlays() == 0, "A second selector remained after one selection"
                print("PASS delayed overlapping requests produce one capture", flush=True)

                subprocess.run([binary, "area"], env=env, check=True)
                wait_for(lambda: overlays() > 0)
                time.sleep(0.2)
                subprocess.run(["wtype", "-k", "Escape"], check=True)
                wait_for(lambda: overlays() == 0)
                time.sleep(0.1)
                subprocess.run([binary, "area"], env=env, check=True)
                wait_for(lambda: overlays() > 0)
                time.sleep(0.2)
                subprocess.run([pointer, "drag", "200", "300", "600", "500"], check=True)
                wait_for(lambda: len(list(captures.glob("*.png"))) == 2)
                assert overlays() == 0
                print("PASS capture succeeds after completion and cancellation", flush=True)
            finally:
                daemon.terminate()
                try:
                    daemon.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()


if __name__ == "__main__":
    main()
