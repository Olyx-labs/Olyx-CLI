"""Audit file-backed trace persistence with disposable SQLite databases."""
from contextlib import closing
import concurrent.futures
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def main():
    binary = Path(sys.argv[1]).resolve()
    calls = []
    blocked = threading.Event()
    release = threading.Event()

    class Provider(BaseHTTPRequestHandler):
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(body)
            if body['messages'][0]['content'] == 'hold-for-crash-test':
                blocked.set()
                release.wait(timeout=30)
            payload = json.dumps({'model': body['model'], 'choices': [{'message': {
                'role': 'assistant', 'content': body['messages'][0]['content']}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='olyx-storage-') as directory:
            root = Path(directory)
            config = {'database': 'data/traces.db', 'models': {'local': [{
                'base_url': f'http://127.0.0.1:{server.server_port}/v1', 'model': 'fixture'}]}}
            (root / 'olyx.json').write_text(json.dumps(config), encoding='utf-8')
            text = "Unicode 🦀 café 中文; '); DROP TABLE traces; --\nsecond line"
            request = {'model': 'local', 'messages': [{'role': 'user', 'content': text}]}
            (root / 'request.json').write_text(json.dumps(request), encoding='utf-8')
            environment = {key: value for key, value in os.environ.items() if not key.startswith('OLYX_')}

            def run(*args, success=True):
                result = subprocess.run([str(binary), *args], cwd=root, env=environment,
                                        capture_output=True, encoding='utf-8', timeout=30)
                assert (result.returncode == 0) == success, result.stderr
                return json.loads(result.stdout) if success else result

            def route(_):
                return run('route', '--input', 'request.json')['trace_id']

            first = route(0)
            original = run('debug', first)
            assert original['steps'][0]['input']['messages'][0]['content'] == text
            assert original['steps'][0]['output']['choices'][0]['message']['content'] == text
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                ids = list(pool.map(route, range(24)))
            assert len(set(ids + [first])) == 25
            run('replay', first)
            assert run('debug', first) == original
            database = root / 'data/traces.db'
            backup = root / 'backup.db'
            backup.touch(mode=0o600)
            with closing(sqlite3.connect(database)) as db:
                assert db.execute('PRAGMA integrity_check').fetchall() == [('ok',)]
                assert db.execute('PRAGMA foreign_key_check').fetchall() == []
                assert db.execute("SELECT count(*) FROM traces WHERE status!='completed'").fetchone()[0] == 0
                assert db.execute('SELECT count(*) FROM traces').fetchone()[0] == 26
                assert db.execute('SELECT count(*) FROM steps').fetchone()[0] == 26
                assert db.execute('SELECT count(*) FROM steps WHERE NOT json_valid(input) OR NOT json_valid(output)').fetchone()[0] == 0
                with closing(sqlite3.connect(backup)) as destination:
                    db.backup(destination)
                before = len(calls)
                db.execute('BEGIN IMMEDIATE')
                result = run('route', '--input', 'request.json', success=False)
                assert 'database is locked' in result.stderr.lower(), result.stderr
                assert len(calls) == before
                db.rollback()
                db.execute("UPDATE traces SET created_at='2000-01-01T00:00:00+00:00' WHERE id=?", (ids[0],))
                db.commit()
            # Opening a fresh CLI process expires the old trace and cascades its steps.
            run('debug', first)
            with closing(sqlite3.connect(database)) as db:
                assert db.execute('SELECT count(*) FROM traces').fetchone()[0] == 25
                assert db.execute('SELECT count(*) FROM steps').fetchone()[0] == 25
                assert db.execute('PRAGMA foreign_key_check').fetchall() == []
            # A fresh reader must not recover another process's live request.
            (root / 'held.json').write_text(json.dumps({'model': 'local', 'messages': [
                {'role': 'user', 'content': 'hold-for-crash-test'}]}), encoding='utf-8')
            held = subprocess.Popen([str(binary), 'route', '--input', 'held.json'],
                                    cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                assert blocked.wait(timeout=10), 'provider never received held request'
                with closing(sqlite3.connect(database)) as db:
                    pending = db.execute("SELECT id FROM traces WHERE status='pending'").fetchall()
                assert len(pending) == 1
                interrupted_id = pending[0][0]
                assert run('debug', interrupted_id)['status'] == 'pending'
                held.kill()
                held.communicate(timeout=10)
                before = len(calls)
                recovered = run('debug', interrupted_id)
                assert recovered['status'] == 'failed'
                assert recovered['metadata']['diagnostic']['code'] == 'request_interrupted'
                assert len(calls) == before, 'recovery resent the request'
                assert run('debug', first) == original
            finally:
                if held.poll() is None:
                    held.kill()
                held.communicate(timeout=10)
                release.set()
            config['database'] = str(backup)
            (root / 'olyx.json').write_text(json.dumps(config), encoding='utf-8')
            assert run('debug', first) == original
            assert run('debug', ids[0])['status'] == 'completed'
            assert database.stat().st_mode & 0o077 == 0
            assert database.parent.stat().st_mode & 0o077 == 0
        print('Storage audit passed: exact payloads, 24 concurrent writers, process reopen, replay, integrity, backup/restore, retention, locked-write failure and forced-termination recovery without disrupting active writers.')
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)


if __name__ == '__main__':
    main()
