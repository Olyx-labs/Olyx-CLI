#!/usr/bin/env python3
"""Account-free plugin fixture. Echoes text and emits synthetic vectors, not inference."""
import argparse
import base64
import json
import os
import struct
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument('--mode', default='normal')
mode = parser.parse_args().mode
request = json.loads(sys.stdin.buffer.readline())
def emit(event):
    print(json.dumps(event), flush=True)
if mode == 'timeout':
    if os.environ.get('PLUGIN_PID_FILE'):
        with open(os.environ['PLUGIN_PID_FILE'], 'w') as pidfile:
            pidfile.write(str(os.getpid()))
    time.sleep(10)
if mode == 'oversize':
    print('x' * 1_048_577, flush=True)
    raise SystemExit()
emit({'type': 'hello', 'protocol_version': 2 if mode == 'version' else 1,
      'capabilities': ['chat', 'streaming', 'tools', 'embeddings']})
if mode in ['unavailable', 'rate_limited', 'authentication_failed']:
    emit({'type': 'error', 'code': mode})
    raise SystemExit()
if mode == 'malformed':
    print('not JSON', flush=True)
    raise SystemExit()
if mode == 'crash':
    raise SystemExit(3)
if request['protocol_version'] != 1:
    raise SystemExit(2)
body = request['request']
if request['operation'] == 'embeddings':
    inputs = body['input']
    count = 1 if isinstance(inputs, str) or (inputs and isinstance(inputs[0], int)) else len(inputs)
    vector = [float(i + 1) for i in range(body.get('dimensions', 2))]
    if body.get('encoding_format') == 'base64':
        vector = base64.b64encode(struct.pack('<' + 'f' * len(vector), *vector)).decode()
    output = {'model': request['model'], 'data': [{'index': i, 'embedding': vector} for i in range(count)],
              'usage': {'prompt_tokens': count, 'total_tokens': count}}
else:
    text = body['messages'][-1].get('content') or ''
    if mode == 'environment':
        text = json.dumps({'credential': os.environ.get('PLUGIN_TEST_KEY') == 'synthetic-plugin-key',
                           'unrelated_absent': 'PLUGIN_UNRELATED_SECRET' not in os.environ})
    message = {'role': 'assistant', 'content': text}
    finish = 'stop'
    if body.get('tools'):
        message = {'role': 'assistant', 'content': None, 'tool_calls': [{'id': 'fixture-call', 'type': 'function', 'function': {'name': body['tools'][0]['function']['name'], 'arguments': '{}'}}]}
        finish = 'tool_calls'
    output = {'choices': [{'index': 0, 'message': message, 'finish_reason': finish}],
              'usage': {'prompt_tokens': 2, 'completion_tokens': 3, 'total_tokens': 5}}
    if body.get('stream'):
        emit({'type': 'snapshot', 'output': output})
        if mode == 'slow-stream':
            with open(os.environ['PLUGIN_PID_FILE'], 'w') as pidfile:
                pidfile.write(str(os.getpid()))
            time.sleep(10)
        if mode == 'partial-failure':
            emit({'type': 'error', 'code': 'unavailable'})
            raise SystemExit()
        if mode == 'mismatch':
            output = {**output, 'choices': [{'index': 0, 'message': {'role': 'assistant', 'content': 'changed'}, 'finish_reason': 'stop'}]}
emit({'type': 'result', 'output': output})
if mode == 'after-result':
    emit({'type': 'snapshot', 'output': output})
