"""Exercise certification with a real CLI and an account-free local provider."""
import importlib.util
import json
import os
from pathlib import Path
import threading
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

spec = importlib.util.spec_from_file_location('certification', Path(__file__).with_name('certify-providers.py'))
certification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(certification)


class Certification(unittest.TestCase):
    def config(self, url='https://provider.example/v1'):
        return {'models': {'private-account-alias': [{'protocol': 'openai', 'base_url': url,
                'model': 'private-deployment', 'api_key_env': 'CERT_TEST_KEY'}]}}

    def run_check(self, config, **kwargs):
        return certification.certify(Path('missing-binary'), config, ['private-account-alias'], **kwargs)

    def test_missing_credentials_skip_without_launching_binary(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(certification.subprocess, 'run') as run:
            report = self.run_check(self.config())
        run.assert_not_called()
        self.assertEqual(report['summary'], {'PASS': 0, 'FAIL': 0, 'SKIP': 1})
        self.assertEqual(report['providers'][0]['reason'], 'missing_credentials')

    def test_fallback_cannot_certify_wrong_provider(self):
        config = self.config()
        config['models']['private-account-alias'] *= 2
        self.assertEqual(self.run_check(config)['providers'][0]['reason'], 'single_candidate_required')

    def test_endpoint_credentials_and_loopback_are_rejected(self):
        for url in ['https://key:secret@provider.example', 'https://provider.example?key=secret', 'http://provider.example', 'http://127.0.0.1:1234']:
            with self.subTest(url=url):
                self.assertEqual(self.run_check(self.config(url))['providers'][0]['reason'], 'invalid_endpoint')

    def test_errors_never_include_subprocess_output(self):
        result = certification.subprocess.CompletedProcess([], 1, 'private-secret', 'private-secret')
        with patch.dict(os.environ, {'CERT_TEST_KEY': 'private-secret'}), patch.object(certification.subprocess, 'run', return_value=result):
            report = self.run_check(self.config())
        serialized = json.dumps(report)
        for sensitive in ['private-secret', 'private-account-alias', 'private-deployment', 'provider.example', 'CERT_TEST_KEY']:
            self.assertNotIn(sensitive, serialized)
        self.assertEqual(report['summary']['FAIL'], 1)
        self.assertEqual(report['providers'][0]['checks'][0]['reason'], 'cli_failed')

    def test_timeout_is_sanitized(self):
        with patch.dict(os.environ, {'CERT_TEST_KEY': 'secret'}), patch.object(certification.subprocess, 'run', side_effect=certification.subprocess.TimeoutExpired(['secret'], 1, stderr='secret')):
            report = self.run_check(self.config())
        self.assertNotIn('secret', json.dumps(report))
        self.assertEqual(report['providers'][0]['checks'][0]['reason'], 'timeout')

    def test_require_live_does_not_accept_all_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'config.json'
            config.write_text(json.dumps(self.config()))
            report = Path(directory) / 'report.json'
            environment = {k: v for k, v in os.environ.items() if k != 'CERT_TEST_KEY'}
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('certify-providers.py')),
                                     '--config', str(config), '--alias', 'private-account-alias',
                                     '--report', str(report), '--require-live'],
                                    env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(report.read_text())['summary']['SKIP'], 1)
            self.assertNotIn('private-account-alias', result.stdout + result.stderr)

    def test_existing_configuration_cannot_be_overwritten_by_report(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'config.json'
            original = json.dumps(self.config())
            config.write_text(original)
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('certify-providers.py')),
                                     '--config', str(config), '--alias', 'private-account-alias',
                                     '--report', str(config)],
                                    env={k: v for k, v in os.environ.items() if k != 'CERT_TEST_KEY'},
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(config.read_text(), original)

    def test_real_cli_local_contract_and_simulated_label(self):
        binary = Path(os.environ.get('OLYX_TEST_BINARY', str(Path(__file__).resolve().parents[1] / 'target/debug/oy'))).resolve()
        if not binary.is_file():
            self.skipTest('Build the CLI or set OLYX_TEST_BINARY for subprocess integration')
        calls = []
        class Provider(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                calls.append(request)
                message = {'role': 'assistant', 'content': 'OLYX_CERT_OK'}
                response = {'choices': [{'index': 0, 'message': message, 'finish_reason': 'stop'}], 'usage': {'prompt_tokens': 3, 'completion_tokens': 4, 'total_tokens': 7}}
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream' if request.get('stream') else 'application/json')
                self.end_headers()
                if request.get('stream'):
                    response['choices'][0]['delta'] = response['choices'][0].pop('message')
                    wire = 'data: ' + json.dumps(response) + '\n\ndata: [DONE]\n\n'
                else:
                    wire = json.dumps(response)
                self.wfile.write(wire.encode())
        server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with patch.dict(os.environ, {'CERT_TEST_KEY': 'synthetic-only'}):
                report = certification.certify(binary, self.config(f'http://127.0.0.1:{server.server_port}/v1'), ['private-account-alias'], allow_loopback=True)
            self.assertEqual(report['summary'], {'PASS': 1, 'FAIL': 0, 'SKIP': 0}, report)
            self.assertEqual(report['providers'][0]['verification'], 'not-live-verified')
            self.assertEqual(report['environment'], 'simulated')
            self.assertEqual(len(calls), 3)  # Chat, stream and replay; debug is offline.
        finally:
            server.shutdown()
            server.server_close()
            thread.join(5)


if __name__ == '__main__':
    unittest.main()
