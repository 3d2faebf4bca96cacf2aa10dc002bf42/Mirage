#!/usr/bin/env python3
"""Manual local-server regression: tone audio must arrive before sending ends.

Requires a trusted server on 127.0.0.1:9987. Uses synthetic audio and separate,
ignored test identities; does not reset the server or stop existing processes.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    private = ROOT / ".audit-local/voice-progress"
    private.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["ALLOW_PRIVATE_TARGETS"] = "1"
    messages = queue.Queue()
    children = []

    def launch(role):
        command = "audio-recv" if role == "receiver" else "audio-send"
        process = subprocess.Popen(
            [str(args.binary.resolve()), command, "127.0.0.1:9987", "12" if role == "receiver" else "8",
             "--name", "progress-" + role, "--identity", str(private / (role + ".json"))],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        children.append(process)
        def read():
            for line in iter(process.stdout.readline, b""):
                messages.put((role, time.monotonic(), line.decode("utf-8", errors="replace")))
        threading.Thread(target=read, daemon=True).start()

    def wait_for(role, pattern, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                source, stamp, line = messages.get(timeout=max(0.01, deadline-time.monotonic()))
            except queue.Empty:
                break
            if source == role and re.search(pattern, line):
                return stamp, line
        raise RuntimeError("peer readiness or live voice progress deadline exceeded")

    try:
        launch("receiver")
        wait_for("receiver", "listening for voice", 35)
        launch("sender")
        start, ready = wait_for("sender", "sending 440 Hz", 35)
        match = re.search(r"sender_client_id=(\d+)", ready)
        if not match:
            raise RuntimeError("sender did not report its client ID; rebuild the diagnostic CLI")
        sender_id = match.group(1)
        pattern = r"\[audio\] packets=[1-9].*senders=\{[^}]*\b" + sender_id + r"\b"
        first, _ = wait_for("receiver", pattern, 11)
        delay = first-start
        for child in children:
            child.wait(timeout=20)
        result = {"first_positive_report_seconds": round(delay, 3),
                  "exit_codes": [child.returncode for child in children],
                  "live_progress": 0 <= delay < 3 and all(child.returncode == 0 for child in children)}
        print(json.dumps(result))
        return 0 if result["live_progress"] else 1
    finally:
        for child in children:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
