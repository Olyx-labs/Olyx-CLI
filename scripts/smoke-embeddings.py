"""Exercise RAG embeddings without paid API calls or a cloud account."""
import base64
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
from urllib.error import HTTPError

binary = str(Path(sys.argv[1]).resolve())
calls = []
class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        assert self.headers.get('Authorization') == 'Bearer provider-secret' or self.headers.get('api-key') == 'provider-secret'
        calls.append((self.path, body))
        if self.path == '/v1/chat/completions':
            assert body['model'] == 'answer-v1'
            assert 'source.md' in body['messages'][1]['content']
            payload = json.dumps({'choices': [{'message': {'role': 'assistant', 'content': 'Back up the trace database [1].'}, 'finish_reason': 'stop'}], 'usage': {'prompt_tokens': 10, 'completion_tokens': 5}}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        status = 200
        if self.path.startswith('/retry/'):
            status, output = 503, {'error': {'message': 'unavailable'}}
        elif self.path.startswith('/deny/'):
            status, output = 401, {'error': {'message': 'bad key'}}
        else:
            assert body['model'] == 'embedding-v1'
            inputs = body['input']
            count = 1 if isinstance(inputs, str) or isinstance(inputs[0], int) else len(inputs)
            data = []
            for i in range(count):
                embedding = [0.5, 0.25]
                if body.get('encoding_format') == 'base64': embedding = base64.b64encode(struct.pack('<ff', *embedding)).decode()
                data.append({'object': 'embedding', 'index': i, 'embedding': embedding})
            output = {'object': 'list', 'model': 'embedding-v1', 'data': list(reversed(data)), 'usage': {'prompt_tokens': 6, 'total_tokens': 6}}
            if self.path.startswith('/malformed/'): output['data'] = [{'index': 0, 'embedding': [1]}]
            if self.path.startswith('/oversize/'): output['padding'] = 'x' * 2048
        payload = json.dumps(output).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

provider = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=provider.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory() as directory:
    config = str(Path(directory) / 'olyx.json')
    base = f'http://127.0.0.1:{provider.server_port}'
    def candidate(path, model='embedding-v1', protocol='openai'):
        return {'base_url': base + path, 'model': model, 'protocol': protocol, 'api_key_env': 'EMBED_KEY'}
    routes = {
        'answer': [candidate('/v1', model='answer-v1')],
        'embed': [candidate('/v1')],
        'fallback': [candidate('/retry'), candidate('/v1')],
        'denied': [candidate('/deny'), candidate('/v1')],
        'mismatch': [candidate('/retry'), candidate('/v1', model='different-vector-space')],
        'malformed': [candidate('/malformed')], 'oversize': [candidate('/oversize')],
    }
    Path(config).write_text(json.dumps({'models': routes, 'database': str(Path(directory) / 'traces.db'), 'policy': {'max_embedding_response_bytes': 1024}}))
    env = {**os.environ, 'EMBED_KEY': 'provider-secret', 'OLYX_SERVER_API_KEY': 'application-secret', 'OLYX_CONFIG_DIR': str(Path(directory) / 'auth')}
    def cli(*args):
        result = subprocess.run([binary, *args, '--config', config], env=env, capture_output=True, encoding='utf-8', timeout=20)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)
    def start():
        process = subprocess.Popen([binary, 'serve', '--local', '--config', config, '--port', '0'], env=env, stderr=subprocess.PIPE, stdout=subprocess.PIPE, encoding='utf-8')
        line = process.stderr.readline()
        match = re.search(r'Olyx listening on (http://[^;]+)', line)
        assert match, line
        return process, match.group(1)
    server, endpoint = start()
    def request(body, authorized=True):
        headers = {'Content-Type': 'application/json'}
        if authorized: headers['Authorization'] = 'Bearer application-secret'
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        try:
            with urlopen(Request(endpoint + '/v1/embeddings', data=data, headers=headers), timeout=10) as response:
                return response.status, response.headers, json.load(response)
        except HTTPError as e:
            return e.code, e.headers, json.load(e)
    try:
        assert request({'model': 'embed', 'input': 'query'}, authorized=False)[0] == 401
        for invalid in [b'{', {}, {'model': 'embed', 'input': []}, {'model': 'embed', 'input': ['good', '']}, {'model': 'embed', 'input': 'text', 'stream': True}, {'model': 'missing', 'input': 'text'}, {'model': 'embed', 'input': 'text', 'dimensions': 0}, {'model': 'mismatch', 'input': 'text'}]:
            assert request(invalid)[0] == 400, invalid
        assert not calls, calls
        status, headers, output = request({'model': 'embed', 'input': ['document', 'query'], 'dimensions': 2})
        assert status == 200 and [row['index'] for row in output['data']] == [0, 1], output
        trace = headers['X-Olyx-Trace-Id']
        debug = cli('debug', trace)
        assert debug['status'] == 'completed'
        assert debug['metadata']['endpoint'] == '/v1/embeddings'
        assert debug['steps'][0]['output']['embedding_count'] == 2
        assert 'embedding' not in debug['steps'][0]['output']
        before = len(calls)
        replay = cli('replay', trace)
        assert len(calls) == before + 1
        replay_trace = replay['comparison']['replays'][0]
        assert replay_trace['metadata']['endpoint'] == '/v1/embeddings'
        cli('replay', replay_trace['external_id'])
        assert len(calls) == before + 2
        for inputs in [[1, 2], [[1], [2, 3]]]:
            assert request({'model': 'embed', 'input': inputs})[0] == 200
        status, _, encoded = request({'model': 'embed', 'input': 'query', 'encoding_format': 'base64'})
        assert status == 200 and len(base64.b64decode(encoded['data'][0]['embedding'])) == 8
        before = len(calls)
        assert request({'model': 'fallback', 'input': 'query'})[0] == 200
        assert [path for path, _ in calls[before:]] == ['/retry/embeddings', '/v1/embeddings']
        before = len(calls)
        assert request({'model': 'denied', 'input': 'query'})[0] == 502
        assert len(calls) == before + 1
        assert request({'model': 'malformed', 'input': 'query', 'dimensions': 2})[0] == 502
        assert request({'model': 'oversize', 'input': 'query'})[0] == 502
        assert request({'model': 'embed', 'input': 'x' * 262145})[0] == 413
        assert cli('debug', trace)['status'] == 'completed'
        server.terminate();server.communicate(timeout=15)
        server, endpoint = start()
        assert cli('debug', trace)['steps'][0]['output']['dimensions'] == 2
        documents = Path(directory) / 'documents'
        documents.mkdir()
        (documents / 'source.md').write_text('Back up the trace database regularly.')
        example = Path(__file__).resolve().parents[1] / 'examples' / 'rag.py'
        index = Path(directory) / 'vectors.json'
        for args in [('index', str(documents)), ('ask', 'How do I back up traces?')]:
            result = subprocess.run([sys.executable, str(example), *args, '--base-url', endpoint + '/v1', '--index', str(index)], env=env, capture_output=True, encoding='utf-8', timeout=20)
            assert result.returncode == 0, result.stderr
        assert 'Back up the trace database [1]' in result.stdout
        assert json.loads(index.read_text())['embedding_model'] == 'embedding-v1'

    finally:
        server.terminate();server.communicate(timeout=15)
provider.shutdown()
print('Embeddings smoke passed: text/token batches, base64, same-model fallback, validation, response limits, traces, replay and persistence.')
