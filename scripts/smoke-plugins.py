"""End-to-end account-free subprocess adapter contract and runtime regressions."""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import time
from http.client import HTTPConnection
from urllib.request import Request, urlopen

binary = str(Path(sys.argv[1]).resolve())
root = Path(__file__).resolve().parents[1]
def process_alive(pid):
    if sys.platform.startswith('linux'):
        try:
            # kill(pid, 0) also succeeds for zombies: those processes have exited.
            state = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
            if state in {'Z', 'X'}:
                return False
        except FileNotFoundError:
            return False
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False

checks = 0
with tempfile.TemporaryDirectory(prefix='olyx-plugin-contract-') as directory:
    config = Path(directory) / 'olyx.json'
    adapter = {'protocol': 'plugin', 'model': 'fixture-model', 'plugin': {
        'command': sys.executable, 'args': [str(root / 'examples/plugins/echo.py')],
        'capabilities': ['chat', 'streaming', 'tools', 'embeddings'], 'timeout_ms': 5000}}
    current = {'database': str(Path(directory) / 'traces.db'), 'models': {'echo': [adapter]}}
    environment = {**os.environ, 'OLYX_SERVER_API_KEY': 'synthetic-plugin-server-key',
                   'PLUGIN_TEST_KEY': 'synthetic-plugin-key', 'PLUGIN_UNRELATED_SECRET': 'must-not-propagate'}
    def write():
        config.write_text(json.dumps(current))
    def cli(*args, body=None, good=True):
        result = subprocess.run([binary, *args, '--config', str(config)], input=json.dumps(body) if body is not None else None,
                                env=environment, capture_output=True, encoding='utf-8', timeout=15)
        assert (result.returncode == 0) == good, result.stderr
        return result
    def chat(stream=False, **extras):
        return {'model': 'echo', 'messages': [{'role': 'user', 'content': 'hello 🦀'}], 'stream': stream, **extras}
    def mode(name):
        adapter['plugin']['args'] = [str(root / 'examples/plugins/echo.py'), '--mode', name]
        write()
    write()
    cli('check', str(config)); checks += 1
    routed = json.loads(cli('route', '--input', '-', body=chat()).stdout)
    assert routed['output']['choices'][0]['message']['content'] == 'hello 🦀'; checks += 1
    original = json.loads(cli('debug', routed['trace_id']).stdout)
    events = [json.loads(line) for line in cli('route', '--input', '-', body=chat(True)).stdout.splitlines()]
    assert events[0]['type'] == 'snapshot' and events[-1]['type'] == 'completed'; checks += 1
    replay = json.loads(cli('replay', routed['trace_id']).stdout)
    assert replay['status'] == 'completed' and json.loads(cli('debug', routed['trace_id']).stdout) == original; checks += 1
    tool = json.loads(cli('route', '--input', '-', body=chat(tools=[{'type': 'function', 'function': {'name': 'lookup', 'parameters': {'type': 'object'}}}])).stdout)
    assert tool['output']['choices'][0]['message']['tool_calls'][0]['function']['name'] == 'lookup'; checks += 1
    server = subprocess.Popen([binary, 'serve', '--local', '--config', str(config), '--port', '0'], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding='utf-8')
    try:
        match = re.search(r'Olyx listening on (http://[^;]+)', server.stderr.readline())
        assert match
        endpoint = match.group(1)
        def http(path, body):
            with urlopen(Request(endpoint + path, data=json.dumps(body).encode(), headers={'Authorization': 'Bearer synthetic-plugin-server-key', 'Content-Type': 'application/json'}), timeout=10) as response:
                return json.load(response), response.headers['X-Olyx-Trace-Id']
        response, _ = http('/v1/chat/completions', chat())
        assert response['choices'][0]['message']['content'] == 'hello 🦀'; checks += 1
        vectors, trace = http('/v1/embeddings', {'model': 'echo', 'input': ['first', 'second'], 'dimensions': 3})
        assert len(vectors['data']) == 2 and vectors['data'][0]['embedding'] == [1.0, 2.0, 3.0]; checks += 1
        assert json.loads(cli('replay', trace).stdout)['status'] == 'completed'; checks += 1
    finally:
        server.terminate(); server.communicate(timeout=10)
    adapter['plugin']['capabilities'] = ['chat']; write()
    assert 'plugin_capability_unsupported' in cli('route', '--input', '-', body=chat(True), good=False).stderr; checks += 1
    adapter['plugin']['capabilities'] += ['streaming', 'tools', 'embeddings']
    for name, code in [('version', 'plugin_version_mismatch'), ('malformed', 'plugin_invalid_event'), ('crash', 'plugin_exit_failed'), ('after-result', 'plugin_event_after_result'), ('oversize', 'plugin_event_too_large')]:
        mode(name)
        assert code in cli('route', '--input', '-', body=chat(), good=False).stderr; checks += 1
    adapter['plugin']['timeout_ms'] = 150; mode('timeout')
    assert 'plugin_timeout' in cli('route', '--input', '-', body=chat(), good=False).stderr; checks += 1
    adapter['plugin']['timeout_ms'] = 5000
    for name, code in [('partial-failure', 'plugin_unavailable'), ('mismatch', 'plugin_stream_result_mismatch')]:
        mode(name)
        assert code in cli('route', '--input', '-', body=chat(True), good=False).stderr; checks += 1
    # HTTP client disconnect must cancel the future and kill the running child.
    pidfile = Path(directory) / 'child.pid'
    environment['PLUGIN_PID_FILE'] = str(pidfile)
    adapter['plugin']['timeout_ms'] = 20000
    adapter['plugin']['env'] = ['PLUGIN_PID_FILE']; mode('slow-stream')
    server = subprocess.Popen([binary, 'serve', '--local', '--config', str(config), '--port', '0'], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding='utf-8')
    try:
        match = re.search(r'Olyx listening on http://([^;]+)', server.stderr.readline())
        assert match
        connection = HTTPConnection(match.group(1), timeout=10)
        connection.request('POST', '/v1/chat/completions', json.dumps(chat(True)), {'Authorization': 'Bearer synthetic-plugin-server-key', 'Content-Type': 'application/json'})
        response = connection.getresponse()
        assert response.status == 200
        for _ in range(100):
            if pidfile.exists(): break
            time.sleep(.02)
        assert pidfile.exists()
        pid = int(pidfile.read_text())
        if connection.sock is not None:
            connection.sock.shutdown(socket.SHUT_RDWR)
        response.close(); connection.close()
        gone = False
        # Less than the fixture's ten-second sleep and twenty-second plugin timeout:
        # a pass must come from cancellation, not normal completion or timeout.
        for _ in range(250):
            if not process_alive(pid): gone = True; break
            time.sleep(.02)
        assert gone, f'Cancelled plugin child remained alive: pid={pid}'
        checks += 1
    finally:
        server.terminate(); server.communicate(timeout=10)
    adapter['plugin']['timeout_ms'] = 5000
    adapter['plugin']['env'] = []
    adapter['api_key_env'] = 'PLUGIN_TEST_KEY'; mode('environment')
    result = json.loads(cli('route', '--input', '-', body=chat()).stdout)
    assert json.loads(result['output']['choices'][0]['message']['content']) == {'credential': True, 'unrelated_absent': True}; checks += 1
    adapter.pop('api_key_env'); mode('unavailable')
    second = json.loads(json.dumps(adapter)); second['plugin']['args'] = [str(root / 'examples/plugins/echo.py')]
    current['models']['echo'] = [adapter, second]; write()
    result = json.loads(cli('route', '--input', '-', body=chat()).stdout)
    debug = json.loads(cli('debug', result['trace_id']).stdout)
    assert [step['allowed'] for step in debug['steps']] == [False, True]; checks += 1
    mode('partial-failure')
    cli('route', '--input', '-', body=chat(True), good=False)
    latest = json.loads(cli('debug').stdout)['data'][0]['external_id']
    debug = json.loads(cli('debug', latest).stdout)
    assert debug['status'] == 'failed' and len(debug['steps']) == 1; checks += 1
print(f'Plugin contract smoke passed: {checks} checks; chat, tools, streaming, HTTP, embeddings, replay, failure isolation and credentials.')
