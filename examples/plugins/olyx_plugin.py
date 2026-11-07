"""Minimal Olyx v1 chat adapter helper. Python standard library only."""
import json
import sys


def serve(chat):
    """Call chat(model, request) and emit protocol events; never print raw exceptions."""
    def emit(event):
        print(json.dumps(event, allow_nan=False), flush=True)

    emit({"type": "hello", "protocol_version": 1, "capabilities": ["chat"]})
    try:
        envelope = json.loads(sys.stdin.buffer.readline())
        if envelope.get("protocol_version") != 1 or envelope.get("operation") != "chat":
            raise ValueError("unsupported protocol or operation")
        request = envelope["request"]
        if request.get("stream") or request.get("tools"):
            raise ValueError("unsupported capability")
        output = chat(envelope["model"], request)
        emit({"type": "result", "output": output})
    except Exception:
        # Provider exceptions can contain keys or request data. Keep them private.
        emit({"type": "error", "code": "unavailable"})
        raise SystemExit(1)
