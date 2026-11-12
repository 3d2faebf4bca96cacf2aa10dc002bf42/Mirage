#!/usr/bin/env python3
"""Exercise a newly built bridge on an unused local port without a TS server.

Only the process launched here is stopped. No connect/audio commands are sent.
This checks HTTP and WebSocket behavior, not rendered UI or microphone quality.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def handshake(port, host, origin=None):
    connection = socket.create_connection(("127.0.0.1", port), timeout=3)
    key = base64.b64encode(os.urandom(16)).decode()
    request = (
        f"GET /ws HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\n"
        f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
    )
    if origin is not None:
        request += f"Origin: {origin}\r\n"
    connection.sendall((request + "\r\n").encode())
    stream = connection.makefile("rb")
    status = stream.readline().decode().split()[1]
    while True:
        line = stream.readline()
        if line == b"\r\n":
            break
        if not line:
            raise ConnectionError("incomplete WebSocket handshake")
    return connection, stream, int(status)


def frame(stream):
    header = stream.read(2)
    assert len(header) == 2, "missing WebSocket frame"
    opcode = header[0] & 15
    length = header[1] & 127
    if length == 126:
        length = struct.unpack("!H", stream.read(2))[0]
    elif length == 127:
        length = struct.unpack("!Q", stream.read(8))[0]
    return opcode, stream.read(length)


def send(connection, payload):
    data = payload.encode()
    mask = os.urandom(4)
    header = bytes([0x81, 0x80 | len(data)]) if len(data) < 126 else bytes([0x81, 0xFE]) + struct.pack("!H", len(data))
    connection.sendall(header + mask + bytes(value ^ mask[index % 4] for index, value in enumerate(data)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    environment = os.environ.copy()
    environment.pop("TS6_BRIDGE_PORT", None)
    environment.pop("TS6_UI_DIR", None)
    process = subprocess.Popen(
        [str(args.binary.resolve()), "--port", str(port), "--ui", str(ROOT / "client/apps/ui/dist")],
        env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
    )
    try:
        deadline = time.monotonic() + 10
        while True:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}", timeout=1) as response:
                    assert response.status == 200
                    assert b'lang="en"' in response.read()
                break
            except OSError:
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise RuntimeError("bridge did not become ready")
                time.sleep(0.05)
        for host, origin, expected in [
            (f"127.0.0.1:{port}", f"http://127.0.0.1:{port}", 101),
            (f"localhost:{port}", "http://localhost:5174", 101),
            (f"127.0.0.1:{port}", None, 101),
            (f"127.0.0.1:{port}", "https://untrusted.example", 403),
            (f"untrusted.example:{port}", f"http://untrusted.example:{port}", 403),
            (f"127.0.0.1:{port}", "null", 403),
        ]:
            connection, stream, actual = handshake(port, host, origin)
            try:
                assert actual == expected, "unexpected handshake result"
                if actual == 101:
                    opcode, payload = frame(stream)
                    assert opcode == 1 and json.loads(payload)["ev"] == "hello"
            finally:
                stream.close()
                connection.close()
        connection, stream, status = handshake(port, f"127.0.0.1:{port}")
        try:
            assert status == 101
            frame(stream)  # greeting
            send(connection, '{"op":"refresh"}')
            _, payload = frame(stream)
            event = json.loads(payload)
            assert event["ev"] == "log" and event["text"] == "not connected"
            send(connection, "x" * (16 * 1024 + 1))
            # Axum may close the connection without sending a close frame on an oversized input.
            try:
                opcode, _ = frame(stream)
                assert opcode == 8, "oversized command was not rejected"
            except (AssertionError, ConnectionError):
                assert stream.read(1) == b"", "connection remained open after oversized input"
        finally:
            stream.close()
            connection.close()
        print("PASS: HTTP assets, six Host/Origin cases, native refresh, and oversized input rejection.")
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    main()
