"""Regression probes for provider contracts, SSE, accounting, cancellation and comparison. Local mocks only."""
import json, os, socket, sqlite3, subprocess, tempfile, threading, time, http.client, sys
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request,urlopen
from urllib.error import HTTPError
BIN=str(Path(sys.argv[1]).resolve())
ROOT=Path(tempfile.mkdtemp(prefix='olyx-edge-regression-'))
results=[];calls=[];fixtures={}
def completion(message=None, **extra):
 return dict(choices=[dict(index=0,message=message if message is not None else {'role':'assistant','content':'ok'},finish_reason='stop')],usage={'prompt_tokens':2,'completion_tokens':3,'total_tokens':5},**extra)
def sse(*events,sep='\n\n'):
 return ''.join('data: '+(e if isinstance(e,str) else json.dumps(e))+sep for e in events).encode()
fixtures['ok']=completion()
fixtures['null_content']=completion({'role':'assistant','content':None})
fixtures['role_user']=completion({'role':'user','content':'wrong role'})
fixtures['tool_null']=completion({'role':'assistant','content':None,'tool_calls':[None]})
fixtures['tool_bad_args']=completion({'role':'assistant','content':None,'tool_calls':[{'id':'c','type':'function','function':{'name':'lookup','arguments':{'q':'x'}}}]})
fixtures['tool_missing_name']=completion({'role':'assistant','content':None,'tool_calls':[{'id':'c','type':'function','function':{'arguments':'{}'}}]})
fixtures['content_bad_parts']=completion({'role':'assistant','content':[42]})
fixtures['usage_negative']=completion();fixtures['usage_negative']['usage']={'prompt_tokens':-3,'completion_tokens':2,'total_tokens':-1}
fixtures['usage_overflow']=completion();fixtures['usage_overflow']['usage']={'prompt_tokens':18446744073709551615,'completion_tokens':1}
fixtures['usage_reported_total']=completion();fixtures['usage_reported_total']['usage']['total_tokens']=9
fixtures['usage_string']=completion();fixtures['usage_string']['usage']='not-a-usage-object'
fixtures['choice_indices']={'choices':[{'index':1,'message':{'role':'assistant','content':'choice one'},'finish_reason':'stop'},{'index':0,'message':{'role':'assistant','content':'choice zero'},'finish_reason':'stop'}]}
fixtures['duplicate_choice_indices']={'choices':[{'index':0,'message':{'content':'one'}},{'index':0,'message':{'content':'two'}}]}
fixtures['error_and_choices']=completion();fixtures['error_and_choices']['error']={'message':'upstream error','code':'test_error'}
fixtures['tool_truncated']=completion({'role':'assistant','content':None,'tool_calls':[{'id':'c','type':'function','function':{'name':'lookup','arguments':'{"q":'}}]});fixtures['tool_truncated']['choices'][0]['finish_reason']='length'
fixtures['tool_stream_truncated']=sse({'choices':[{'index':0,'delta':{'tool_calls':[{'index':0,'id':'c','type':'function','function':{'name':'lookup','arguments':'{"q":'}}]},'finish_reason':None}]},{'choices':[{'index':0,'delta':{},'finish_reason':'length'}]},'[DONE]')
fixtures['stream_usage']=sse({'choices':[{'index':0,'delta':{'content':'ok'},'finish_reason':'stop'}]},{'choices':[],'usage':{'prompt_tokens':2,'completion_tokens':3,'total_tokens':9}},'[DONE]')
fixtures['sse_crlf']=sse({'choices':[{'index':0,'delta':{'content':'héllo 🦀'},'finish_reason':'stop'}]},'[DONE]',sep='\r\n\r\n')
fixtures['sse_cr']=sse({'choices':[{'index':0,'delta':{'content':'hello'},'finish_reason':'stop'}]},'[DONE]',sep='\r\r')
fixtures['stream_tools_bad_args']=sse({'choices':[{'index':0,'delta':{'tool_calls':[{'index':0,'id':'c','type':'function','function':{'name':'lookup','arguments':42}}]},'finish_reason':'tool_calls'}]},'[DONE]')
fixtures['sse_bom']=b'\xef\xbb\xbf'+sse({'choices':[{'index':0,'delta':{'content':'A'},'finish_reason':None}]},{'choices':[{'index':0,'delta':{'content':'B'},'finish_reason':'stop'}]},'[DONE]')
fixtures['sse_multiline']=b'data: {"choices":\ndata: [{"index":0,"delta":{"content":"ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
fixtures['embed_wrong_model']={'model':'unexpected-vector-space','data':[{'index':0,'embedding':[1.0,2.0]}],'usage':{'prompt_tokens':2,'total_tokens':2}}
fixtures['embed_bad_usage']={'model':'embed-model','data':[{'index':0,'embedding':[1.0,2.0]}],'usage':{'prompt_tokens':-2,'total_tokens':-2}}
fixtures['embed_nan_base64']={'model':'embed-model','data':[{'index':0,'embedding':'AADAfw=='}]}
fixtures['embed_huge_index']={'model':'embed-model','data':[{'index':18446744073709551615,'embedding':[1.0,2.0]}]}
fixtures['embed_allow_version']={'model':'embed-model-v2','data':[{'index':0,'embedding':[1.0,2.0]}],'usage':{'prompt_tokens':2,'total_tokens':2}}
fixtures['embed_missing_model']={'data':[{'index':0,'embedding':[1.0,2.0]}]}
fixtures['slow']=completion()
fixtures['slow_sse']=sse({'choices':[{'index':0,'delta':{'content':'slow'},'finish_reason':'stop'}]},'[DONE]')
class Mock(BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  name=self.path.split('/')[2];calls.append({'fixture':name,'body':body})
  if name in ['slow','slow_sse']:time.sleep(1.5)
  if name=='reject':self.send_response(503);self.end_headers();return
  value=fixtures[name]
  self.send_response(200);self.send_header('Content-Type','text/event-stream' if isinstance(value,bytes) else 'application/json');self.end_headers()
  wire=value if isinstance(value,bytes) else json.dumps(value).encode()
  try:
   if name=='sse_crlf':
    for b in wire:self.wfile.write(bytes([b]));self.wfile.flush()
   else:self.wfile.write(wire)
  except (BrokenPipeError,ConnectionResetError):pass
mock=ThreadingHTTPServer(('127.0.0.1',0),Mock);threading.Thread(target=mock.serve_forever,daemon=True).start()
models={}
for name in [*fixtures,'reject']:
 protocol='openai'
 models[name]=[{'protocol':protocol,'base_url':f'http://127.0.0.1:{mock.server_port}/{protocol}/{name}','model':'embed-model' if name.startswith('embed_') else 'test-model','api_key_env':'EDGE_KEY'}]
models['embed_allow_version'][0]['embedding_response_models']=['embed-model-v2']
models['embed_space_fallback']=[dict(models['embed_allow_version'][0]),dict(models['embed_wrong_model'][0])]
config=ROOT/'olyx.json';db=ROOT/'traces.db';config.write_text(json.dumps({'database':str(db),'models':models,'policy':{'max_output_tokens':1024}}))
env=dict(os.environ,EDGE_KEY='synthetic-provider-key',OLYX_SERVER_API_KEY='synthetic-server-key')
s=socket.socket();s.bind(('127.0.0.1',0));port=s.getsockname()[1];s.close()
log=(ROOT/'gateway.log').open('w');server=subprocess.Popen([BIN,'serve','--local','--port',str(port),'--max-concurrency','2','--config',str(config)],env=env,stdout=log,stderr=log)
def request(name,stream=False,extra=None,path='/v1/chat/completions',auth='Bearer synthetic-server-key',raw=None,content_type='application/json'):
 body={'model':name,'messages':[{'role':'user','content':'edge test'}],'stream':stream};body.update(extra or {})
 req=Request(f'http://127.0.0.1:{port}{path}',data=raw if raw is not None else json.dumps(body).encode(),headers={'Authorization':auth,'Content-Type':content_type})
 try:
  with urlopen(req,timeout=8) as r:wire=r.read().decode();status=r.status;trace=r.headers.get('X-Olyx-Trace-Id')
 except HTTPError as e:wire=e.read().decode();status=e.code;trace=e.headers.get('X-Olyx-Trace-Id')
 except Exception as e:return {'status':'connection_error','exception':type(e).__name__+': '+str(e)}
 try:body=json.loads(wire)
 except ValueError:body=None
 return {'status':status,'trace':trace,'body':body,'wire':wire}
def run(name,description,check,**kwargs):
 start=time.monotonic();r=request(name,**kwargs)
 try:ok=check(r)
 except Exception:ok=False
 row={'case':len(results)+1,'name':description,'fixture':name,'result':'PASS' if ok else 'ISSUE','elapsed_ms':round((time.monotonic()-start)*1000),'observed':r}
 results.append(row);(ROOT/'results.json').write_text(json.dumps(results,indent=2));print(f"{row['case']:02} {row['result']} {description}",flush=True);return r
try:
 for _ in range(100):
  try:urlopen(f'http://127.0.0.1:{port}/healthz',timeout=.2);break
  except Exception:time.sleep(.05)
 else:raise RuntimeError('Gateway failed to start: '+(ROOT/'gateway.log').read_text())
 for name,desc in [('role_user','Non-assistant role in completion'),('tool_null','Null tool-call entry'),('tool_bad_args','Object instead of tool argument string'),('tool_missing_name','Tool call without function name'),('content_bad_parts','Malformed multimodal response parts'),('usage_negative','Negative token counters'),('usage_string','Scalar usage instead of usage object'),('duplicate_choice_indices','Duplicate completion choice indices'),('error_and_choices','Error payload alongside completion choices')]:
  run(name,desc,lambda r:r['status']==502)
 run('null_content','Legitimate null content remains accepted',lambda r:r['status']==200)
 run('usage_overflow','Token counter overflow is rejected without disconnect',lambda r:r['status']==502)
 run('usage_reported_total','Reported total tokens preserved over inferred sum',lambda r:r['status']==200 and r['body']['usage']['total_tokens']==9)
 run('choice_indices','Out-of-order choice indices remain attached to their content',lambda r:r['status']==200 and [(c['index'],c['message']['content']) for c in r['body']['choices']]==[(1,'choice one'),(0,'choice zero')])
 run('tool_truncated','Truncated tool output keeps length finish reason',lambda r:r['status']==200 and r['body']['choices'][0]['finish_reason']=='length')
 run('tool_stream_truncated','Streamed truncated tool output keeps length finish reason',lambda r:'"finish_reason":"length"' in r.get('wire',''),stream=True)
 run('stream_usage','SSE preserves reported token total',lambda r:'"total_tokens":9' in r.get('wire',''),stream=True,extra={'stream_options':{'include_usage':True}})
 run('sse_crlf','CRLF SSE and byte-fragmented Unicode',lambda r:r['status']==200 and 'héllo 🦀' in r['wire'] and '[DONE]' in r['wire'],stream=True)
 run('sse_cr','CR-only SSE line endings',lambda r:r['status']==200 and '[DONE]' in r.get('wire',''),stream=True)
 run('stream_tools_bad_args','Non-string streamed tool arguments rejected',lambda r:'"error"' in r.get('wire','') and '[DONE]' not in r['wire'],stream=True)
 for name,desc,extra in [('embed_wrong_model','Embedding identity mismatch',{}),('embed_bad_usage','Negative embedding usage',{}),('embed_nan_base64','NaN in base64 embedding',{'encoding_format':'base64'}),('embed_huge_index','Out-of-range embedding index',{})]:
  raw=json.dumps(dict(model=name,input='query',**extra)).encode()
  run(name,desc,lambda r:r['status']==502,path='/v1/embeddings',raw=raw)
 for name,desc,expected in [('embed_allow_version','Explicit approved embedding version is accepted',200),('embed_missing_model','Embedding responses must report model identity',502),('embed_space_fallback','Embedding replica identities must use the same approved identifiers',400)]:
  run(name,desc,lambda r,expected=expected:r['status']==expected,path='/v1/embeddings',raw=json.dumps({'model':name,'input':'query'}).encode())
 run('ok','Lowercase bearer authentication scheme',lambda r:r['status']==200,auth='bearer synthetic-server-key')
 run('sse_bom','Leading UTF-8 BOM does not drop the first content delta',lambda r:'AB' in ''.join(json.loads(line[6:]).get('choices',[{}])[0].get('delta',{}).get('content','') for line in r.get('wire','').splitlines() if line.startswith('data: {') and json.loads(line[6:]).get('choices')),stream=True)
 run('sse_multiline','Multiline SSE data fields',lambda r:r['status']==200 and '[DONE]' in r.get('wire','') and '"content":"ok"' in r['wire'],stream=True)
 run('ok','Mixed-case bearer authentication scheme',lambda r:r['status']==200,auth='bEaReR synthetic-server-key')
 run('ok','JSON content type with charset',lambda r:r['status']==200,content_type='application/json; charset=utf-8')
 run('ok','Null request envelope rejected locally',lambda r:r['status']==400,raw=b'null')
 # Verify comparison behavior when the middle alias fails.
 r=request('ok');source=r['trace'];before=len(calls)
 p=subprocess.run([BIN,'replay',source,'--compare','ok,reject,null_content','--config',str(config)],env=env,encoding='utf-8',capture_output=True,timeout=10)
 seen=[c['fixture'] for c in calls[before:]]
 results.append({'case':len(results)+1,'name':'Comparison isolates failed aliases and continues later candidates','result':'PASS' if 'null_content' in seen and p.returncode != 0 and json.loads(p.stdout)['status']=='partial' and json.loads(p.stdout)['comparison']['succeeded']==2 and json.loads(p.stdout)['comparison']['failed']==1 else 'ISSUE','observed':{'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'provider_calls':seen}})
 # Drop an HTTP connection while a nonstream upstream request is running.
 before=sqlite3.connect(db).execute('SELECT coalesce(max(rowid),0) FROM traces').fetchone()[0]
 conn=http.client.HTTPConnection('127.0.0.1',port,timeout=4)
 body=json.dumps({'model':'slow','messages':[{'role':'user','content':'cancel me'}]})
 conn.request('POST','/v1/chat/completions',body,{'Authorization':'Bearer synthetic-server-key','Content-Type':'application/json'})
 time.sleep(.2);conn.close();time.sleep(2)
 states=sqlite3.connect(db).execute('SELECT id,status FROM traces WHERE rowid>?',(before,)).fetchall()
 results.append({'case':len(results)+1,'name':'Client disconnect leaves no permanently pending trace','result':'PASS' if states and all(s[1]!='pending' for s in states) else 'ISSUE','observed':states})
 # A disconnected SSE client must cancel the detached worker even before first content.
 before=sqlite3.connect(db).execute('SELECT coalesce(max(rowid),0) FROM traces').fetchone()[0]
 conn=http.client.HTTPConnection('127.0.0.1',port,timeout=4)
 conn.request('POST','/v1/chat/completions',json.dumps({'model':'slow_sse','stream':True,'messages':[{'role':'user','content':'cancel stream'}]}),{'Authorization':'Bearer synthetic-server-key','Content-Type':'application/json'})
 for _ in range(100):
  states=sqlite3.connect(db).execute('SELECT id,status FROM traces WHERE rowid>?',(before,)).fetchall()
  if states:break
  time.sleep(.005)
 conn.close()
 for _ in range(100):
  states=sqlite3.connect(db).execute('SELECT id,status FROM traces WHERE rowid>?',(before,)).fetchall()
  if states and all(row[1]=='failed' for row in states):break
  time.sleep(.005)
 results.append({'case':len(results)+1,'name':'SSE disconnect before first content promptly finalizes the trace','result':'PASS' if states and all(row[1]=='failed' for row in states) else 'ISSUE','observed':states})
 # SIGINT must persist cancellation before the CLI runtime exits (route and replay).
 if os.name == 'posix':
  import signal
  source=request('slow_sse',stream=True)['trace']
  for operation in ['route','replay']:
   before=sqlite3.connect(db).execute('SELECT coalesce(max(rowid),0) FROM traces').fetchone()[0]
   args=[BIN,operation,*(['--input','-'] if operation=='route' else [source]),'--config',str(config)]
   process=subprocess.Popen(args,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,encoding='utf-8')
   if operation=='route':
    process.stdin.write(json.dumps({'model':'slow_sse','stream':True,'messages':[{'role':'user','content':'interrupt me'}]}));process.stdin.close();process.stdin=None
   states=[]
   for _ in range(200):
    with sqlite3.connect(db) as connection:
     states=connection.execute('SELECT id,status FROM traces WHERE rowid>?',(before,)).fetchall()
    if states:break
    time.sleep(.005)
   process.send_signal(signal.SIGINT)
   stdout,stderr=process.communicate(timeout=10)
   with sqlite3.connect(db) as connection:
    states=connection.execute("SELECT id,status,json_extract(metadata,'$.diagnostic.code') FROM traces WHERE rowid>?",(before,)).fetchall()
   passed=process.returncode==130 and states and all(row[1]=='failed' and row[2]=='request_cancelled' for row in states)
   results.append({'case':len(results)+1,'name':operation+' SIGINT persists terminal cancellation before exit','result':'PASS' if passed else 'ISSUE','observed':{'exit':process.returncode,'traces':states,'stderr':stderr}})
 # Health and concurrency controls after malformed requests and cancellation.
 run('ok','Gateway remains functional after adversarial requests',lambda r:r['status']==200)
 (ROOT/'results.json').write_text(json.dumps(results,indent=2))
 print('ARTIFACTS '+str(ROOT),flush=True)
finally:
 server.terminate()
 try:server.wait(timeout=5)
 except subprocess.TimeoutExpired:server.kill();server.wait()
 log.close();mock.shutdown();mock.server_close()

failures=[r for r in results if r['result']!='PASS']
print(f"Edge regression: {len(results)-len(failures)}/{len(results)} passed; report: {ROOT / 'results.json'}")
sys.exit(1 if failures else 0)
