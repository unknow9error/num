#!/usr/bin/env python3
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "generated"))

from connectors import num_connector_egress_context_from_json


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
    raw_egress = payload.get("egress")
    context = num_connector_egress_context_from_json(
        raw_egress if isinstance(raw_egress, dict) else None
    )
    request_id = context.request_id if context else "no-request"

    print(json.dumps(f"python echo [{request_id}]: {message}"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
