#!/usr/bin/env python3
"""Build and test an isolated Compose deployment with synthetic data, then remove it.

HTTP headers simulate a host TLS proxy; this does not certify public TLS/DNS.
Only this script's unique test project and synthetic volume are removed.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent


def main():
    project = "chronexus-server-qa-" + uuid.uuid4().hex[:8]
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    origin = "https://sync.example.test:18443"
    env = dict(os.environ, CHRONEXUS_PUBLIC_ORIGIN=origin,
               CHRONEXUS_BIND_ADDRESS="127.0.0.1", CHRONEXUS_HOST_PORT=str(port),
               CHRONEXUS_INTERNAL_PORT="8181", CHRONEXUS_NETWORK_SUBNET="172.30.252.0/24",
               CHRONEXUS_NETWORK_GATEWAY="172.30.252.1", CHRONEXUS_TRUSTED_PROXIES="172.30.252.1/32")
    command = ["docker", "compose", "-p", project]

    def compose(*args, capture=False):
        return subprocess.run(command + list(args), cwd=ROOT, env=env, check=True,
                              stdout=subprocess.PIPE if capture else None,
                              text=capture).stdout

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def request(method, path, payload=None, token=None, proto="https"):
        headers = {"Host": "sync.example.test:18443", "X-Forwarded-Proto": proto,
                   "X-Forwarded-For": "198.51.100.42", "Content-Type": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        data = json.dumps(payload).encode() if payload is not None else None
        req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data, headers=headers, method=method)
        try:
            response = opener.open(req, timeout=10)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            content = response.read()
            assert response.headers["Cache-Control"] == "no-store"
            return response.status, json.loads(content) if content else None

    try:
        compose("up", "-d", "--build", "--wait", "--wait-timeout", "45")
        container = compose("ps", "-q", "server", capture=True).strip()
        config = json.loads(subprocess.check_output(["docker", "inspect", container]))[0]
        assert config["Config"]["User"] == "10001:10001"
        assert config["HostConfig"]["ReadonlyRootfs"]
        bindings = config["HostConfig"]["PortBindings"]
        assert list(bindings) == ["8181/tcp"]
        assert bindings["8181/tcp"] == [{"HostIp": "127.0.0.1", "HostPort": str(port)}]
        assert request("GET", "/api/v1/meta")[0] == 200
        assert request("GET", "/api/v1/meta", proto="http")[0] == 403
        code = compose("exec", "-T", "server", "chronexus-server", "invite", capture=True).strip()
        payload = dict(inviteCode=code, username="qa_player", displayName="QA Player",
                       password="synthetic-password-123", device="QA desktop")
        status, signed_in = request("POST", "/api/v1/auth/redeem", payload)
        assert status == 200
        token = signed_in["token"]
        assert request("POST", "/api/v1/auth/redeem", payload)[0] == 410
        assert request("GET", "/api/v1/auth/sessions", token=token)[0] == 200
        compose("up", "-d", "--no-build", "--force-recreate", "--wait", "--wait-timeout", "45")
        assert request("GET", "/api/v1/auth/sessions", token=token)[0] == 200
        container = compose("ps", "-q", "server", capture=True).strip()
        # Inspect bytes in memory only: no database or credentials enter the checkout.
        raw = subprocess.check_output(["docker", "exec", container, "sh", "-c",
            "cat /var/lib/chronexus/chronexus.sqlite3 /var/lib/chronexus/chronexus.sqlite3-wal"])
        for value in [code, token, payload["password"]]:
            assert value.encode() not in raw
        assert request("POST", "/api/v1/auth/logout", token=token)[0] == 204
        assert request("GET", "/api/v1/auth/sessions", token=token)[0] == 401
        logs = compose("logs", "--no-color", "server", capture=True)
        for value in [code, token, payload["password"]]:
            assert value not in logs
        print("PASS: Compose build/health, non-root/read-only, alternate ports, trusted proxy gate, auth, recreation persistence, hash-only storage and secret-free logs.")
    finally:
        compose("down", "--volumes", "--remove-orphans")


if __name__ == "__main__":
    main()
