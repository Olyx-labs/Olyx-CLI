"""Regressions for account-free orchestration and explicit live opt-in."""
from contextlib import redirect_stdout
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

loader = importlib.machinery.SourceFileLoader('test_providers_runner', str(Path(__file__).with_name('test-providers')))
spec = importlib.util.spec_from_loader(loader.name, loader)
runner = importlib.util.module_from_spec(spec)
loader.exec_module(runner)


class TestProviders(unittest.TestCase):
    def test_all_registered_smoke_suites_exist(self):
        for name, _, _ in runner.SUITES:
            self.assertTrue((runner.ROOT / 'scripts' / f'smoke-{name}.py').is_file(), name)

    def checks(self):
        return [{'name': name, 'status': 'PASS'} for name, _, _ in runner.SUITES]

    def test_default_never_invokes_live_checks(self):
        with patch.object(runner, 'local_checks', return_value=(Path('oy'), self.checks())), patch.object(runner, 'live_checks') as live, redirect_stdout(io.StringIO()) as output:
            status = runner.main([])
        self.assertEqual(status, 0)
        live.assert_not_called()
        self.assertIn('No live provider calls', output.getvalue())

    def test_local_failures_continue_all_suites_and_strip_credentials(self):
        observed = []
        def run(name, command, environment, *args):
            observed.append((name, environment.copy()))
            return {'name': name, 'status': 'FAIL' if name == 'local' else 'PASS'}
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'oy'
            binary.touch()
            with patch.dict(os.environ, {'OPENAI_API_KEY': 'secret', 'CUSTOM_CREDENTIAL': 'secret', 'OLYX_TOKEN': 'secret'}), patch.object(runner, 'run_check', side_effect=run):
                _, checks = runner.local_checks(binary)
        self.assertEqual(len(checks), len(runner.SUITES) + 1)
        self.assertEqual(observed[-1][0], 'verification-tests')
        for _, env in observed:
            for key in ['OPENAI_API_KEY', 'CUSTOM_CREDENTIAL', 'OLYX_TOKEN']:
                self.assertNotIn(key, env)
        self.assertEqual(runner.coverage(checks)[0]['contract'], 'NOT_VERIFIED')

    def test_local_failure_prevents_live_spending(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'private.json'
            config.write_text('{}')
            with patch.object(runner, 'local_checks', return_value=(Path('oy'), [{'name': 'local', 'status': 'FAIL'}])), patch.object(runner, 'live_checks') as live, redirect_stdout(io.StringIO()):
                status = runner.main(['--live-config', str(config), '--alias', 'custom'])
        self.assertEqual(status, 1)
        live.assert_not_called()

    def test_required_live_skips_fail_and_report_is_sanitized(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / 'private.json'
            config.write_text('{"private": "secret"}')
            report = Path(directory) / 'report.json'
            with patch.object(runner, 'local_checks', return_value=(Path('oy'), self.checks())), patch.object(runner, 'live_checks', return_value={'summary': {'PASS': 0, 'FAIL': 0, 'SKIP': 1}}), redirect_stdout(io.StringIO()):
                status = runner.main(['--live-config', str(config), '--alias', 'private-alias', '--require-live', '--report', str(report)])
            self.assertEqual(status, 1)
            self.assertNotIn('secret', report.read_text())
            self.assertNotIn('private-alias', report.read_text())
            self.assertEqual(json.loads(report.read_text())['status'], 'FAIL')

    def test_existing_report_is_rejected_before_any_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / 'report.json'
            report.write_text('preserve')
            with patch.object(runner, 'local_checks') as local, self.assertRaises(SystemExit) as error:
                runner.main(['--report', str(report)])
            self.assertEqual(error.exception.code, 2)
            local.assert_not_called()
            self.assertEqual(report.read_text(), 'preserve')

    def test_subprocess_errors_never_enter_report(self):
        result = subprocess.CompletedProcess([], 1, 'private-secret', 'private-secret')
        with patch.object(runner.subprocess, 'run', return_value=result), redirect_stdout(io.StringIO()) as output:
            row = runner.run_check('local', ['command'], {})
        self.assertEqual(row['status'], 'FAIL')
        self.assertNotIn('private-secret', json.dumps(row) + output.getvalue())


if __name__ == '__main__':
    unittest.main()
