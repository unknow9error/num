#!/usr/bin/env python3
import json
import sys


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError as err:
        print(f"invalid connector payload: {err}", file=sys.stderr)
        return 2

    method = payload.get("method")
    if method != "echo.reply":
        print(f"unsupported method: {method}", file=sys.stderr)
        return 3

    args = payload.get("args") or []
    message = args[0] if args else ""
    egress = payload.get("egress") or {}
    request_id = egress.get("request_id", "no-request")

    print(json.dumps(f"python echo [{request_id}]: {message}"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
