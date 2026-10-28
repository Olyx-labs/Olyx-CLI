"""Exercise actionable diagnostics with local mocks; no inference billing."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError
from urllib.request import Request, urlopen

binary = str(Path(sys.argv[1]).resolve())
calls = []

class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass
    def do_HEAD(self):
        assert self.headers.get('Authorization') is None
        calls.append('HEAD')
        self.send_response(405)
        self.end_headers()
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        calls.append(body)
        if body.get('model') == 'malformed':
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"choices":[{}]}')
        elif body.get('model') == 'unfinished':
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            self.wfile.write(b'data: {"choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":null}]}\n\ndata: [DONE]\n\n')
        else:
            self.send_response(401)
            self.end_headers()
            self.wfile.write(b'{"error":{"message":"sensitive-upstream-content"}}')

provider = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=provider.serve_forever, daemon=True).start()
try:
    with tempfile.TemporaryDirectory() as folder:
        config = Path(folder) / 'olyx.json'
        database = Path(folder) / 'traces.db'
        config.write_text(json.dumps({'database': str(database), 'models': {
            alias: [{'base_url': f'http://127.0.0.1:{provider.server_port}/v1', 'model': model, 'api_key_env': 'DIAGNOSTICS_TEST_KEY'}]
            for alias, model in [('fast', 'test'), ('malformed', 'malformed'), ('unfinished', 'unfinished')]
        }}))
        env = dict(os.environ, DIAGNOSTICS_TEST_KEY='provider-secret', OLYX_SERVER_API_KEY='server-secret')
        def cli(*args, good=True, environment=env):
            result = subprocess.run([binary, *args, '--config', str(config)], env=environment, capture_output=True, encoding='utf-8', timeout=20)
            assert (result.returncode == 0) == good, result.stderr
            assert 'provider-secret' not in result.stdout + result.stderr
            return result
        report = json.loads(cli('doctor').stdout)
        assert report['ok'] and not calls and not database.exists()
        missing = dict(env)
        missing.pop('DIAGNOSTICS_TEST_KEY')
        assert not json.loads(cli('doctor', good=False, environment=missing).stdout)['ok']
        assert json.loads(cli('doctor', '--probe').stdout)['ok']
        assert calls == ['HEAD']
        listener = socket.socket()
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
        listener.close()
        server = subprocess.Popen([binary, 'serve', '--local', '--port', str(port), '--config', str(config)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            for _ in range(100):
                try:
                    with urlopen(f'http://127.0.0.1:{port}/readyz', timeout=1) as response:
                        assert json.load(response)['checks']['provider_credentials'] == 'not_validated'
                    break
                except OSError:
                    time.sleep(.05)
            else:
                raise AssertionError('server not ready')
            for path, body in [('/v1/chat/completions', {'model': 'fast', 'messages': [{'role': 'user', 'content': 'hello'}]}), ('/v1/embeddings', {'model': 'fast', 'input': 'hello'})]:
                request = Request(f'http://127.0.0.1:{port}{path}', data=json.dumps(body).encode(), headers={'Authorization': 'Bearer server-secret', 'Content-Type': 'application/json'})
                try:
                    urlopen(request)
                    raise AssertionError('expected provider rejection')
                except HTTPError as error:
                    assert error.code == 502
                    raw = error.read().decode()
                    assert 'sensitive-upstream-content' not in raw
                    result = json.loads(raw)['error']
                    assert result['code'] == 'provider_authentication_failed'
                    assert result['upstream_status'] == 401 and not result['retryable']
                    assert result['fallback'] == 'stopped_non_retryable'
                    assert error.headers['X-Olyx-Trace-Id'] == result['trace_id']
                    debug = cli('debug', result['trace_id'])
                    assert debug.stderr.startswith('Failure: provider_authentication_failed')
                    assert json.loads(debug.stdout)['diagnostic'] == result
            for model, stream in [('malformed', False), ('unfinished', True)]:
                body = {'model': model, 'messages': [{'role': 'user', 'content': 'hello'}], 'stream': stream}
                request = Request(f'http://127.0.0.1:{port}/v1/chat/completions', data=json.dumps(body).encode(), headers={'Authorization': 'Bearer server-secret', 'Content-Type': 'application/json'})
                if stream:
                    with urlopen(request, timeout=10) as response:
                        raw = response.read().decode()
                    assert 'provider_stream_incomplete' in raw and '[DONE]' not in raw, raw
                    assert 'partial' in raw
                else:
                    try:
                        urlopen(request, timeout=10)
                        raise AssertionError('malformed success accepted')
                    except HTTPError as error:
                        result = json.loads(error.read())['error']
                        assert error.code == 502 and result['code'] == 'invalid_provider_response'
                        assert result['fallback'] == 'stopped_response_started'
                trace = next(t for t in json.loads(cli('debug').stdout)['data'] if t['steps'] and t['steps'][0]['model'] == model)
                assert trace['status'] == 'failed' and len(trace['steps']) == 1
                assert not trace['steps'][0]['allowed']
                if stream:
                    assert trace['steps'][0]['output']['partial_output']['choices'][0]['message']['content'] == 'partial'
        finally:
            server.terminate()
            server.wait(timeout=10)
finally:
    provider.shutdown()
    provider.server_close()
print('Diagnostics smoke passed: offline doctor, missing credentials, credential-free probes, stable chat/embedding errors, trace headers, failure-first debug, malformed successes and incomplete SSE.')
