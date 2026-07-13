
from __future__ import annotations
import json
import sys
from typing import Any


def handle(message: dict[str, Any]) -> dict[str, Any]:
    request_id = message.get("id")
    method = message.get("method")
    if method == "describe":
        result = {"name": "python-adapter-host", "version": "0.1.0", "methods": ["describe", "health", "invoke"]}
    elif method == "health":
        result = {"status": "healthy"}
    elif method == "invoke":
        result = {"echo": message.get("params", {})}
    else:
        return {"id": request_id, "error": {"code": "unknown_method", "message": str(method)}}
    return {"id": request_id, "result": result}


def main() -> None:
    for line in sys.stdin:
        try:
            response = handle(json.loads(line))
        except Exception as exc:  # protocol boundary: return structured error
            response = {"id": None, "error": {"code": "malformed_request", "message": str(exc)}}
        print(json.dumps(response, separators=(",", ":")), flush=True)

if __name__ == "__main__":
    main()
