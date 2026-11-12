#!/usr/bin/env python3
"""Manual TS6 integration of the UI wire protocol against a trusted local server.

Starts only its own bridge, uses an ignored audit profile, never enables audio,
and verifies connect/snapshot/nickname/mute/refresh/disconnect. Not an offline CI test.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import time
from smoke_bridge import handshake, frame, send

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    profile = ROOT / ".audit-local/bridge-compatibility"
    profile.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["APPDATA"] = str(profile)
    env["ALLOW_PRIVATE_TARGETS"] = "1"
    env.pop("TS6_BRIDGE_PORT", None)
    env.pop("TS6_UI_DIR", None)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    process = subprocess.Popen(
        [str(args.binary.resolve()), "--port", str(port), "--ui", str(ROOT/"client/apps/ui/dist")],
        env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
    )
    connection = stream = None
    try:
        deadline = time.monotonic()+10
        while True:
            try:
                connection, stream, status = handshake(port, f"127.0.0.1:{port}", f"http://127.0.0.1:{port}")
                break
            except OSError:
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise RuntimeError("bridge startup failed")
                time.sleep(.05)
        assert status == 101
        connection.settimeout(35)
        def event_until(predicate):
            deadline = time.monotonic()+35
            while time.monotonic() < deadline:
                connection.settimeout(max(.1, deadline-time.monotonic()))
                opcode, payload = frame(stream)
                assert opcode == 1, "unexpected bridge frame"
                event = json.loads(payload)
                if event.get("ev") == "status" and event.get("error"):
                    raise RuntimeError("bridge reported a connection error; no private details displayed")
                if predicate(event):
                    return event
            raise RuntimeError("expected bridge event deadline exceeded")
        event_until(lambda event: event.get("ev") == "hello")
        send(connection, json.dumps({"op":"connect", "address":"127.0.0.1:9987", "nickname":"bridge-compatibility"}))
        event_until(lambda event: event.get("ev") == "status" and event.get("connected") is True)
        snapshot = event_until(lambda event: event.get("ev") == "state")["snapshot"]
        assert snapshot["server"]["version"].startswith("6."), "expected a TS6 server"
        assert any(client["is_self"] for client in snapshot["clients"])
        send(connection, json.dumps({"op":"nick", "name":"bridge-verified"}))
        event_until(lambda event: event.get("ev") == "state" and any(client["is_self"] and client["name"] == "bridge-verified" for client in event["snapshot"]["clients"]))
        for muted in [True, False]:
            send(connection, json.dumps({"op":"mute", "input":muted, "output":muted}))
            event_until(lambda event: event.get("ev") == "log" and event.get("text", "").startswith("muted: input="))
            send(connection, json.dumps({"op":"refresh"}))
            event_until(lambda event: event.get("ev") == "state" and any(client["is_self"] and client["input_muted"] == muted and client["output_muted"] == muted for client in event["snapshot"]["clients"]))
        send(connection, json.dumps({"op":"disconnect"}))
        event_until(lambda event: event.get("ev") == "status" and event.get("connected") is False)
        print(json.dumps({"connect":True,"snapshot":True,"nickname":True,"mute_unmute":True,"refresh":True,"disconnect":True,"microphone_started":False}))
    finally:
        if stream is not None:
            stream.close()
        if connection is not None:
            connection.close()
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    main()
