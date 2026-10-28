"""Verify the built image's entrypoint, authentication, health and writable volume."""
import json
import subprocess
import sys
import time
from urllib.request import Request, urlopen
from urllib.error import HTTPError

image = sys.argv[1] if len(sys.argv) > 1 else 'olyx:test'
def docker(*args):
    return subprocess.check_output(['docker', *args], encoding='utf-8').strip()
volume = docker('volume', 'create')
container = None
try:
    container = docker('run', '-d', '--read-only', '--tmpfs', '/tmp', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges', '-p', '127.0.0.1::8787', '-v', volume + ':/data', '-e', 'OLYX_SERVER_API_KEY=local-container-test', image)
    base = 'http://127.0.0.1:8787'
    for _ in range(50):
        response = subprocess.run(['docker', 'exec', container, 'curl', '--fail', '--silent', base + '/readyz'], capture_output=True)
        if response.returncode == 0: break
        time.sleep(.2)
    else: raise AssertionError(docker('logs', container))
    assert docker('exec', container, 'curl', '--silent', '--output', '/dev/null', '--write-out', '%{http_code}', base + '/v1/models') == '401'
    models = docker('exec', container, 'curl', '--fail', '--silent', '--header', 'Authorization: Bearer local-container-test', base + '/v1/models')
    assert json.loads(models)['data'][0]['id'] == 'fast'
    assert docker('exec', container, 'curl', '--silent', '--output', '/dev/null', '--write-out', '%{http_code}', '--header', 'Authorization: Bearer local-container-test', '--header', 'Content-Type: application/json', '--data', '{"model":"missing","input":"query"}', base + '/v1/embeddings') == '400'
    assert docker('exec', container, 'id', '-u') == '10001'
    docker('exec', container, 'test', '-f', '/data/traces.db')
    docker('exec', container, 'oy', 'debug', '--config', '/etc/olyx/olyx.json')
    docker('stop', '--time', '20', container)
    assert json.loads(docker('inspect', container))[0]['State']['ExitCode'] == 0
    docker('start', container)
    docker('exec', container, 'test', '-f', '/data/traces.db')
    print('Container smoke passed: non-root, read-only root, volume, health, authentication, debug and restart.')
finally:
    if container: docker('rm', '-f', container)
    docker('volume', 'rm', volume)
