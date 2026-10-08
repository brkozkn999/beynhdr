#!/usr/bin/env python3
"""Minimal herdr API client over the Windows named pipe.

Usage:
  python herdr_api.py <method> [params-json]
  python herdr_api.py --file <params.json> <method>

Socket path comes from HERDR_API_PIPE (full pipe path) or is derived from
HERDR_SESSION (default "ln08").
"""
import json
import os
import sys

SESSION = os.environ.get("HERDR_SESSION", "ln08")
DEFAULT_SOCK = os.path.join(
    os.environ.get("APPDATA", r"C:\Users\pc\AppData\Roaming"),
    "herdr",
    "sessions",
    SESSION,
    "herdr.sock",
)
PIPE = os.environ.get("HERDR_API_PIPE") or ("\\\\.\\pipe\\" + DEFAULT_SOCK)


def request(method, params=None, req_id="py"):
    payload = {"id": req_id, "method": method, "params": params or {}}
    data = (json.dumps(payload) + "\n").encode("utf-8")
    with open(PIPE, "r+b", buffering=0) as f:
        f.write(data)
        f.flush()
        line = b""
        while not line.endswith(b"\n"):
            chunk = f.read(65536)
            if not chunk:
                break
            line += chunk
    return json.loads(line.decode("utf-8"))


if __name__ == "__main__":
    argv = sys.argv[1:]
    if argv and argv[0] == "--file":
        params = json.loads(open(argv[1], encoding="utf-8").read())
        method = argv[2]
    else:
        method = argv[0]
        params = json.loads(argv[1]) if len(argv) > 1 else {}
    resp = request(method, params)
    print(json.dumps(resp, indent=2, sort_keys=True))
    sys.exit(0 if "error" not in resp else 1)
