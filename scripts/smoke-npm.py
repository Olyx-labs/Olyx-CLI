"""Run packed npm artifacts through npx using a disposable local registry."""
import argparse
import hashlib
import base64
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packages', type=Path, default=Path('dist/npm'))
    args = parser.parse_args()
    packages = {}
    for archive in args.packages.glob('*.tgz'):
        with tarfile.open(archive) as bundle:
            manifest = json.load(bundle.extractfile('package/package.json'))
        packages[manifest['name']] = (manifest, archive.read_bytes())
    assert 'olyx-cli' in packages, 'Pack the launcher first'
    version = packages['olyx-cli'][0]['version']
    calls = []

    class Registry(BaseHTTPRequestHandler):
        def do_GET(self):
            name = unquote(self.path.lstrip('/'))
            binary_response = name.startswith('tarballs/')
            if binary_response:
                name = unquote(name[len('tarballs/'):])
                if name not in packages:
                    self.send_error(404); return
                payload = packages[name][1]
            elif name in packages:
                manifest, data = packages[name]
                release = {**manifest, 'dist': {'tarball': f'http://127.0.0.1:{self.server.server_port}/tarballs/{name}',
                           'integrity': 'sha512-' + base64.b64encode(hashlib.sha512(data).digest()).decode()}}
                payload = json.dumps({'name': name, 'dist-tags': {'latest': version},
                                      'versions': {version: release}}).encode()
            else:
                self.send_error(404); return
            self.send_response(200)
            self.send_header('Content-Type', 'application/octet-stream' if binary_response else 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self):
            assert self.path == '/v1/chat/completions'
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(body)
            payload = json.dumps({'model': body['model'], 'choices': [{'message': {
                'role': 'assistant', 'content': body['messages'][0]['content']}, 'finish_reason': 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers(); self.wfile.write(payload)

        def log_message(self, *_): pass

    registry = ThreadingHTTPServer(('127.0.0.1', 0), Registry)
    worker = threading.Thread(target=registry.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='olyx-npx-') as directory:
            root = Path(directory)
            env = {key: value for key, value in os.environ.items() if not key.startswith(('OLYX_', 'npm_', 'NPM_'))}
            env.update(NPM_CONFIG_REGISTRY=f'http://127.0.0.1:{registry.server_port}',
                       NPM_CONFIG_CACHE=str(root / 'cache'), NPM_CONFIG_AUDIT='false',
                       NPM_CONFIG_FUND='false', NPM_CONFIG_IGNORE_SCRIPTS='true')
            # Prove this path does not require Cargo or Rust on PATH.
            env['PATH'] = os.pathsep.join(part for part in env['PATH'].split(os.pathsep) if 'cargo' not in part.lower() and 'rustup' not in part.lower())
            npx = shutil.which('npx')
            assert npx, 'Node.js/npm are required for npm packaging tests'
            command = [npx, '--yes', f'olyx-cli@{version}']
            def run(*arguments, body=None, good=True):
                result = subprocess.run([*command, *arguments], cwd=root, env=env,
                                        input=body, encoding='utf-8', capture_output=True, timeout=60)
                assert (result.returncode == 0) == good, result.stderr
                return result
            assert run('--version').stdout.strip() == f'oy {version}'
            assert run('not-a-command', good=False).returncode == 2
            config = {'database': 'traces.db', 'models': {'local': [{
                'base_url': f'http://127.0.0.1:{registry.server_port}/v1', 'model': 'fixture'}]}}
            (root / 'olyx.json').write_text(json.dumps(config), encoding='utf-8')
            text = 'npx preserves 🦀 中文 and quoted "arguments"'
            request = json.dumps({'model': 'local', 'messages': [{'role': 'user', 'content': text}]})
            routed = json.loads(run('route', '--input', '-', body=request).stdout)
            assert routed['output']['choices'][0]['message']['content'] == text
            trace = json.loads(run('debug', routed['trace_id']).stdout)
            assert trace['status'] == 'completed'
            assert json.loads(run('replay', routed['trace_id']).stdout)['status'] == 'completed'
            assert len(calls) == 2
            # Launch the installed JS shim directly to verify its signal forwarding.
            shim = next((root / 'cache/_npx').glob('*/node_modules/olyx-cli/bin/oy.cjs'))
            server = subprocess.Popen([shutil.which('node'), str(shim), 'serve', '--local', '--port', '0'],
                                      cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding='utf-8')
            try:
                assert 'Olyx listening on' in server.stderr.readline()
                server.send_signal(signal.SIGTERM)
                server.communicate(timeout=10)
                assert server.returncode == 0, 'launcher did not preserve graceful shutdown'
            finally:
                if server.poll() is None: server.kill(); server.communicate(timeout=10)
        print('Packed npm smoke passed: fresh npx install without hooks/Rust, version, exit codes, Unicode stdin, routing, traces, replay and server startup (POSIX signal forwarding).')
    finally:
        registry.shutdown(); registry.server_close(); worker.join(timeout=5)


if __name__ == '__main__': main()
