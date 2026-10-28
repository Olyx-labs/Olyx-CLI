"""Exercise the current local command surface without provider calls."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="olyx-standalone-") as directory:
    def run(*args):
        return subprocess.run([binary, *args], cwd=directory, capture_output=True, encoding='utf-8', check=True, timeout=5).stdout
    help_text = run("help")
    for command in ["init", "serve", "route", "replay", "debug", "check", "doctor", "inspect"]:
        assert f"oy {command}" in help_text, command
        assert f"oy {command}" in run("help", command), command
    before = json.loads(run("inspect"))
    assert before["config_exists"] is False
    run("init", "--provider", "openai")
    assert json.loads(run("inspect"))["config_exists"] is True
    run("check", "olyx.json")
    preview = json.loads(run("route", "--input", "request.json", "--dry-run"))
    assert preview["local"] is True
    assert preview["request"]["model"] == "fast"
print("Standalone smoke passed: help, inspect, init, configuration, and request preview.")
