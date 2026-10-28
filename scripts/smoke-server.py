"""Exercise the application endpoint against a local provider; no paid calls."""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
from urllib.parse import urlsplit
from urllib.error import HTTPError

binary = str(Path(sys.argv[1]).resolve())
calls = []
slow_started = threading.Event()
slow_release = threading.Event()

class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        assert self.headers['Authorization'] == 'Bearer provider-secret'
        calls.append(body)
        prompt = body['messages'][0]['content']
        if prompt == 'slow':
            slow_started.set()
            assert slow_release.wait(15)
        if prompt == 'reject':
            self.send_response(401)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream' if body.get('stream') else 'application/json')
        self.end_headers()
        if body.get('stream'):
            frames = [
                {'choices': [{'index': 0, 'delta': {'role': 'assistant', 'content': 'Hel'}}]},
                {'choices': [{'index': 0, 'delta': {'content': 'lo 🦀'}}]},
                {'choices': [{'index': 0, 'delta': {}, 'finish_reason': 'stop'}], 'usage': {'prompt_tokens': 3, 'completion_tokens': 2}},
            ]
            for frame in frames:
                self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
                self.wfile.flush()
            if prompt != 'truncate':
                self.wfile.write(b'data: [DONE]\n\n')
        else:
            self.wfile.write(json.dumps({'choices': [{'message': {'role': 'assistant', 'content': 'Hello 🦀'}, 'finish_reason': 'stop'}], 'usage': {'prompt_tokens': 3, 'completion_tokens': 2}}).encode())

provider = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=provider.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory() as directory:
    config = str(Path(directory) / 'olyx.json')
    env = {**os.environ, 'OPENAI_API_KEY': 'provider-secret', 'OLYX_SERVER_API_KEY': 'application-secret'}
    def cli(*args, success=True):
        result = subprocess.run([binary, *args, '--config', config], env=env, capture_output=True, encoding='utf-8', timeout=20)
        assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
        return result
    cli('init', '--provider', 'openai', '--model', 'mock-model', '--base-url', f'http://127.0.0.1:{provider.server_port}/v1')
    assert 'secret' not in Path(config).read_text()
    original = Path(config).read_bytes()
    cli('init', success=False)
    assert Path(config).read_bytes() == original
    cli('check', config)
    no_key = {k: v for k, v in env.items() if k != 'OLYX_SERVER_API_KEY'}
    unsafe = subprocess.run([binary, 'serve', '--config', config, '--host', '0.0.0.0'], env=no_key, capture_output=True, timeout=10)
    assert unsafe.returncode != 0 and b'OLYX_SERVER_API_KEY is required' in unsafe.stderr
    server = subprocess.Popen([binary, 'serve', '--host', '0.0.0.0', '--config', config, '--port', '0', '--max-concurrency', '1'], env=env, stderr=subprocess.PIPE, stdout=subprocess.PIPE, encoding='utf-8')
    try:
        line = server.stderr.readline()
        match = re.search(r'Olyx listening on (http://[^;]+)', line)
        assert match, line
        base = match.group(1).replace('0.0.0.0', '127.0.0.1')
        def request(path, body=None, key=True):
            headers = {'Content-Type': 'application/json'}
            if key: headers['Authorization'] = 'Bearer application-secret'
            data = None if body is None else (body if isinstance(body, bytes) else json.dumps(body).encode())
            try:
                with urlopen(Request(base + path, data=data, headers=headers), timeout=15) as response:
                    return response.status, response.headers, response.read()
            except HTTPError as error:
                return error.code, error.headers, error.read()
        assert request('/healthz', key=False)[0] == 200
        for path in ['/healthz', '/readyz', '/v1/models', '/missing']:
            assert request(path, key=False)[1].get('X-Content-Type-Options') == 'nosniff'
        assert request('/readyz', key=False)[0] == 200
        assert request('/v1/models', key=False)[0] == 401
        # Authentication must reject requests before waiting for their JSON bodies.
        address = urlsplit(base)
        for path in ['/v1/chat/completions', '/v1/embeddings']:
            for authorization in ['', 'Authorization: Bearer wrong-key\r\n']:
                with socket.create_connection((address.hostname, address.port), timeout=2) as connection:
                    connection.sendall((f'POST {path} HTTP/1.1\r\nHost: {address.netloc}\r\n'
                        'Content-Type: application/json\r\nContent-Length: 100\r\n'
                        f'{authorization}Connection: close\r\n\r\n' + '{').encode())
                    try:
                        response = connection.recv(4096)
                    except TimeoutError:
                        raise AssertionError(f'{path} waited for an unauthenticated request body') from None
                    assert response.startswith(b'HTTP/1.1 401'), response
        assert calls == [], 'Unauthenticated requests reached the provider'
        assert {m['id'] for m in json.loads(request('/v1/models')[2])['data']} == {'fast', 'mock-model'}
        body = {'model': 'fast', 'messages': [{'role': 'user', 'content': 'Hello'}]}
        status, headers, raw = request('/v1/chat/completions', body)
        output = json.loads(raw)
        assert status == 200 and output['object'] == 'chat.completion', raw
        assert output['usage']['total_tokens'] == 5
        trace = headers['X-Olyx-Trace-Id']
        assert output['id'] == 'chatcmpl-' + trace
        assert 'Hello' in cli('debug', trace).stdout
        cli('replay', trace)
        status, headers, raw = request('/v1/chat/completions', {**body, 'stream': True, 'stream_options': {'include_usage': True}})
        assert status == 200 and 'text/event-stream' in headers['Content-Type'], raw
        frames = [line[6:] for line in raw.decode().splitlines() if line.startswith('data: ')]
        assert frames[-1] == '[DONE]', frames
        chunks = [json.loads(frame) for frame in frames[:-1]]
        assert ''.join(c['delta'].get('content', '') for chunk in chunks for c in chunk['choices']) == 'Hello 🦀', chunks
        assert len({chunk['id'] for chunk in chunks}) == 1
        assert len({chunk['created'] for chunk in chunks}) == 1
        assert chunks[-1]['choices'] == [] and chunks[-1]['usage']['total_tokens'] == 5
        assert chunks[-2]['choices'][0]['finish_reason'] == 'stop'
        assert 'Hello' in cli('debug', headers['X-Olyx-Trace-Id']).stdout
        for invalid in [b'{', {'messages': []}, {**body, 'model': 'missing'}]:
            assert request('/v1/chat/completions', invalid)[0] == 400
        assert request('/v1/chat/completions', b' ' * 262145)[0] == 413
        assert len(calls) == 3, calls
        status, _, raw = request('/v1/chat/completions', {**body, 'stream': True, 'messages': [{'role': 'user', 'content': 'reject'}]})
        assert status == 502 and 'provider_authentication_failed' in raw.decode(), raw
        status, _, raw = request('/v1/chat/completions', {**body, 'stream': True, 'messages': [{'role': 'user', 'content': 'truncate'}]})
        assert status == 200 and b'provider_stream_incomplete' in raw and b'[DONE]' not in raw, raw
        slow_result = []
        slow_thread = threading.Thread(target=lambda: slow_result.append(request('/v1/chat/completions', {**body, 'messages': [{'role': 'user', 'content': 'slow'}]})))
        slow_thread.start()
        assert slow_started.wait(10)
        assert request('/v1/chat/completions', body)[0] == 429
        slow_release.set()
        slow_thread.join(15)
        assert slow_result[0][0] == 200
        assert 'Hello' in cli('debug', trace).stdout
    finally:
        server.terminate()
        _, errors = server.communicate(timeout=20)
        if errors: print(errors, file=sys.stderr)
        assert server.returncode == 0
provider.shutdown()
print('Server smoke passed: init, authentication, models, completion, SSE, traces, replay, validation and shutdown.')
