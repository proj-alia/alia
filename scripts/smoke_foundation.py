#!/usr/bin/env python3
"""Exercise the compiled service against the isolated Compose development database."""
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def main():
    settings = dict(line.split("=", 1) for line in (ROOT / ".env").read_text().splitlines()
                    if line.strip() and not line.startswith("#"))
    environment = os.environ.copy()
    environment["ALIA_INSTANCE_TOKEN"] = settings["ALIA_INSTANCE_TOKEN"]
    environment["DATABASE_URL"] = (
        f"postgres://alia:{settings['ALIA_DB_PASSWORD']}@127.0.0.1:"
        f"{settings.get('ALIA_DB_PORT', '55433')}/alia")
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    environment["ALIA_BIND"] = f"127.0.0.1:{port}"
    base = f"http://127.0.0.1:{port}"
    process = subprocess.Popen([str(ROOT / "target/debug/alia")], cwd=ROOT,
                               env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    def request(path, token=None):
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        try:
            with urlopen(Request(base + path, headers=headers), timeout=3) as response:
                return response.status, response.read()
        except HTTPError as error:
            return error.code, error.read()

    try:
        for _ in range(60):
            if process.poll() is not None:
                raise AssertionError("service exited during startup; check local deployment configuration")
            try:
                status, body = request("/healthz")
                if status == 200:
                    break
            except (URLError, TimeoutError):
                pass
            time.sleep(0.1)
        else:
            raise AssertionError("service did not become ready")
        assert json.loads(body) == {"status": "ready"}
        assert request("/api/v1/state")[0] == 401
        status, body = request("/api/v1/state", settings["ALIA_INSTANCE_TOKEN"])
        assert status == 409
        assert json.loads(body)["error"]["code"] == "not_initialized"
        status, body = request("/")
        assert status == 200 and "Alia" in body.decode()
        print("Foundation smoke PASS: PostgreSQL readiness, API authentication, honest uninitialized state, static WebUI")
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
        try:
            process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
        if process.returncode != 0:
            raise AssertionError("service did not shut down gracefully on SIGTERM")


if __name__ == "__main__":
    main()
