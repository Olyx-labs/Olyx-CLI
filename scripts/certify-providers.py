"""Opt-in, bounded live adapter checks. Reports contain only allowlisted metadata."""
import argparse
from datetime import datetime, timezone
import json
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import time
from urllib.parse import urlsplit

PROTOCOLS = {'openai'}


class CheckFailure(Exception):
    pass


def invoke(binary, config, *args, body=None, timeout=90):
    # Never print subprocess output: provider errors can echo credentials or account URLs.
    try:
        result = subprocess.run([str(binary), *args, '--config', str(config)],
                                input=json.dumps(body) if body is not None else None,
                                capture_output=True, text=True, encoding='utf-8', timeout=timeout)
    except subprocess.TimeoutExpired:
        raise CheckFailure('timeout') from None
    except OSError:
        raise CheckFailure('binary_unavailable') from None
    if result.returncode:
        raise CheckFailure('cli_failed')
    try:
        return [json.loads(line) for line in result.stdout.splitlines()] if body and body.get('stream') else json.loads(result.stdout)
    except (ValueError, TypeError):
        raise CheckFailure('invalid_cli_json') from None


def assert_output(output):
    choices = output.get('choices', [])
    if not choices or not isinstance(choices[0].get('message', {}).get('content'), str) or not choices[0]['message']['content'].strip():
        raise CheckFailure('missing_text')
    if choices[0].get('finish_reason') == 'length':
        raise CheckFailure('truncated_response')


def certify(binary, configuration, aliases, allow_loopback=False, timeout=90):
    report = {'schema_version': 1, 'created_at': datetime.now(timezone.utc).isoformat(),
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest() if binary.is_file() else None,
              'suite_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'environment': 'simulated' if allow_loopback else 'live',
              'scope': ['chat', 'streaming', 'trace_persistence', 'replay'], 'providers': []}
    for number, alias in enumerate(aliases, 1):
        # Deliberately exclude aliases, model/deployment IDs, hosts, paths, outputs,
        # trace IDs, error messages, credentials, and account identity from reports.
        row = {'route_number': number, 'protocol': 'unknown', 'status': 'SKIP', 'reason': 'invalid_route', 'verification': 'not-live-verified', 'checks': []}
        report['providers'].append(row)
        candidates = configuration.get('models', {}).get(alias)
        if not isinstance(candidates, list) or len(candidates) != 1:
            row['reason'] = 'single_candidate_required'
            continue
        provider = candidates[0]
        protocol = provider.get('protocol', 'openai')
        if protocol not in PROTOCOLS:
            continue
        row['protocol'] = protocol
        url = urlsplit(provider.get('base_url', ''))
        loopback = url.hostname in {'localhost', '127.0.0.1', '::1'}
        if url.username or url.password or url.query or url.fragment or not url.hostname or (url.scheme != 'https' and not (allow_loopback and loopback and url.scheme == 'http')) or (loopback and not allow_loopback):
            row['reason'] = 'invalid_endpoint'
            continue
        if not os.environ.get(provider.get('api_key_env', '')):
            row['reason'] = 'missing_credentials'
            continue
        row['status'] = 'PASS'
        row.pop('reason')
        with tempfile.TemporaryDirectory(prefix='olyx-certification-') as directory:
            config = Path(directory) / 'olyx.json'
            # No fallbacks: a pass must belong to this candidate, never another provider.
            config.write_text(json.dumps({'database': str(Path(directory) / 'traces.db'),
                                          'models': {'certification': [provider]},
                                          'policy': {'max_output_tokens': 128}}), encoding='utf-8')
            config.chmod(0o600)
            source = None
            original = None
            chat_output = None
            for case in report['scope']:
                start = time.monotonic()
                check = {'name': case, 'status': 'PASS'}
                try:
                    if case in {'chat', 'streaming'}:
                        body = {'model': 'certification', 'messages': [{'role': 'user', 'content': 'Reply briefly with OLYX_CERT_OK.'}], 'max_tokens': 128}
                        if case == 'streaming':
                            body['stream'] = True
                        result = invoke(binary, config, 'route', '--input', '-', body=body, timeout=timeout)
                        if case == 'streaming':
                            if not isinstance(result, list) or not result or result[-1].get('type') != 'completed' or not any(e.get('type') == 'snapshot' for e in result):
                                raise CheckFailure('incomplete_stream')
                            assert_output(result[-1]['output'])
                            saved = invoke(binary, config, 'debug', result[-1]['trace_id'], timeout=timeout)
                            if saved.get('status') != 'completed' or saved['steps'][-1]['output'] != result[-1]['output']:
                                raise CheckFailure('stream_trace_mismatch')
                        else:
                            assert_output(result['output'])
                            source = result['trace_id']
                            chat_output = result['output']
                    elif not source:
                        check.update(status='SKIP', reason='chat_failed')
                    elif case == 'trace_persistence':
                        original = invoke(binary, config, 'debug', source, timeout=timeout)
                        if original.get('status') != 'completed' or not original.get('steps') or original['steps'][-1]['output'] != chat_output:
                            raise CheckFailure('trace_not_completed')
                    else:
                        replay = invoke(binary, config, 'replay', source, timeout=timeout)
                        if replay.get('status') != 'completed':
                            raise CheckFailure('replay_not_completed')
                        replay = replay['comparison']['replays'][0]
                        if replay.get('status') != 'completed' or replay.get('external_id') == source or not replay.get('external_id'):
                            raise CheckFailure('replay_not_completed')
                        assert_output(replay['steps'][-1]['output'])
                        if original is None or original != invoke(binary, config, 'debug', source, timeout=timeout):
                            raise CheckFailure('source_trace_changed')
                except CheckFailure as error:
                    check.update(status='FAIL', reason=str(error))
                    row['status'] = 'FAIL'
                except (KeyError, IndexError, TypeError, ValueError):
                    check.update(status='FAIL', reason='unexpected_cli_shape')
                    row['status'] = 'FAIL'
                check['elapsed_ms'] = round((time.monotonic() - start) * 1000)
                row['checks'].append(check)
        row['verification'] = 'live-verified' if row['status'] == 'PASS' and not allow_loopback else 'not-live-verified'
    report['summary'] = {status: sum(p['status'] == status for p in report['providers']) for status in ['PASS', 'FAIL', 'SKIP']}
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default='target/release/oy')
    parser.add_argument('--config', required=True, help='Configuration with exactly one candidate per selected alias')
    parser.add_argument('--alias', action='append', required=True, help='Explicitly opt in this alias; repeat to test others')
    parser.add_argument('--report', default='provider-certification.json')
    parser.add_argument('--require-live', action='store_true', help='Fail if any selected route is skipped or simulated')
    parser.add_argument('--allow-loopback', action='store_true', help='Test the harness locally; never marks a report live-verified')
    args = parser.parse_args()
    try:
        config = json.loads(Path(args.config).read_text(encoding='utf-8'))
        if not isinstance(config, dict):
            raise ValueError()
    except (OSError, ValueError):
        parser.exit(2, 'Cannot read configuration JSON.\n')
    try:
        report = certify(Path(args.binary).resolve(), config, args.alias, args.allow_loopback)
        destination = Path(args.report)
        # Do not replace config or an existing file with a sanitized report.
        with destination.open('x', encoding='utf-8') as output:
            output.write(json.dumps(report, indent=2) + '\n')
    except (OSError, ValueError, AttributeError, TypeError):
        parser.exit(2, 'Cannot certify configuration or create a new report file.\n')
    print(json.dumps(report['summary']))
    return 1 if report['summary']['FAIL'] or (args.require_live and (report['summary']['SKIP'] or args.allow_loopback)) else 0


if __name__ == '__main__':
    raise SystemExit(main())
