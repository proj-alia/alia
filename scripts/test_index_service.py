#!/usr/bin/env python3
"""Real service/CPU encoder, synthetic sources, restart and missing-asset fail-closed.
No model keys, fake clock, manual vector/job insertion, or production database writes.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import signal
import shutil
import tempfile
import socket
import subprocess
import time
import uuid
from urllib.request import Request, urlopen
from urllib.error import URLError
from urllib.parse import urlencode
ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--web',action='store_true');parser.add_argument('--output',type=Path);args=parser.parse_args()
    args.output=args.output or ROOT/'evals/results'/('index-service-ui-v1.json' if args.web else 'index-service-v1.json')
    if args.output.exists():parser.error('Report already exists; choose a fresh --output path to preserve evidence')
    subprocess.run(['cargo','build','--locked','--features','local-embeddings','--bin','alia','--example','verify_index_worker'],cwd=ROOT,check=True)
    # Other integration-test commands can rebuild target/debug/alia without the
    # feature. Pin these executables for both service processes and the probe.
    with tempfile.TemporaryDirectory(prefix='alia-index-service-') as directory:
        application=Path(directory)/'alia';probe_binary=Path(directory)/'verify_index_worker'
        shutil.copy2(ROOT/'target/debug/alia',application)
        shutil.copy2(ROOT/'target/debug/examples/verify_index_worker',probe_binary)
        run(args,application,probe_binary)

def run(args,application,probe_binary):
    values=dict(line.split('=',1) for line in (ROOT/'.env').read_text().splitlines() if line.strip() and not line.startswith('#'))
    database='alia_semantic_test_'+uuid.uuid4().hex
    env=os.environ.copy()
    for key in ('DEEPSEEK_API_KEY','OPENCODE_API_KEY','ALIA_MODEL_PROVIDER','ALIA_MODEL_ENDPOINT','ALIA_MODEL','ALIA_TOKENIZER_PATH'):
        env.pop(key,None)
    env.update(DATABASE_URL=f"postgres://alia:{values['ALIA_DB_PASSWORD']}@127.0.0.1:{values.get('ALIA_DB_PORT','55433')}/{database}",ALIA_INSTANCE_TOKEN=secrets.token_urlsafe(40),ALIA_EMBEDDING_DIR=str(ROOT/'data/models/multilingual-e5-small'),ALIA_WEB_DIR=str(ROOT/'web/dist'))
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
    env['ALIA_BIND']=f'127.0.0.1:{port}';base=f'http://127.0.0.1:{port}'
    psql=['sudo','-n','docker','compose','exec','-T','db','psql','-U','alia','-d','postgres','-v','ON_ERROR_STOP=1','-c']
    subprocess.run(psql+[f'CREATE DATABASE {database}'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
    def probe(phase):
        result=subprocess.run([str(probe_binary),phase,env['ALIA_EMBEDDING_DIR']],cwd=ROOT,env=env,capture_output=True,text=True,timeout=90)
        if result.returncode:raise RuntimeError('Index probe failed; private stderr withheld')
        return json.loads(result.stdout)
    def get(path):
        with urlopen(Request(base+path,headers={'Authorization':'Bearer '+env['ALIA_INSTANCE_TOKEN']}),timeout=3) as response:return json.load(response)
    report={'data':'synthetic independent development data; actual E5 and service processes','passed':False,'phases':[]}
    output=args.output or ROOT/'evals/results'/('index-service-ui-v1.json' if args.web else 'index-service-v1.json');output.parent.mkdir(parents=True,exist_ok=True)
    try:
        report['seed']=probe('seed')
        report['query_windows']=probe('query_windows')
        first=None
        for phase in ('initial','restart'):
            process=subprocess.Popen([str(application)],cwd=ROOT,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            try:
                for _ in range(120):
                    if process.poll() is not None:raise RuntimeError('Index service exited; check feature/assets, stderr withheld')
                    try:
                        state=get('/api/v1/state')
                        assert state['indexing']['enabled'] and state['indexing']['retrieval_mode']=='hybrid'
                        break
                    except (URLError,TimeoutError):time.sleep(.5)
                else:raise RuntimeError('Index service startup timed out')
                # Probe uses the public production search method; it never creates vectors.
                deadline=time.monotonic()+90
                while True:
                    try:
                        current=probe('verify');break
                    except RuntimeError:
                        if process.poll() is not None or time.monotonic()>=deadline:raise
                        time.sleep(.5)
                retrieved=get('/api/v1/memories/retrieve?'+urlencode({'q':'怎样将文件可靠地保存在磁盘上？','limit':10}))
                assert retrieved['retrieval']['mode']=='hybrid' and retrieved['retrieval']['indexed']==2
                assert retrieved['items'] and '磁盘同步' in retrieved['items'][0]['content']
                long_query='甲'*900
                long_result=get('/api/v1/memories/retrieve?'+urlencode({'q':long_query,'limit':10}))
                assert long_result['retrieval']['query_characters']==len(long_query) and long_result['retrieval']['mode']=='hybrid'
                report.setdefault('shared_reads',[]).append({'phase':phase,'mode':retrieved['retrieval']['mode'],'eligible':retrieved['retrieval']['eligible'],'indexed':retrieved['retrieval']['indexed'],'first_memory_id':retrieved['items'][0]['id'],'long_query_characters':len(long_query)})
                if phase=='initial' and args.web:
                    ui=os.environ.copy();ui.update(ALIA_TEST_URL=base,ALIA_TEST_TOKEN=env['ALIA_INSTANCE_TOKEN'],ALIA_TEST_OUTPUT=str(ROOT/'evals/results/shared-retrieval-ui'))
                    subprocess.run(['node','web/scripts/index-smoke.mjs'],cwd=ROOT,env=ui,check=True)
                    report['ui']='Actual hybrid service/profile and query results; six widths; screenshots captured'
                if first is not None:assert current['jobs']==first['jobs'],'Restart unnecessarily re-encoded ready memories'
                else:first=current
                report['phases'].append({'phase':phase,'individual_id':state['individual_id'],'indexing':state['indexing'],'probe':current})
                assert state['individual_id']==report['phases'][0]['individual_id']
            finally:
                if process.poll() is None:process.send_signal(signal.SIGTERM)
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill();process.wait();raise RuntimeError('Index service did not shut down gracefully') from None
                if process.returncode:raise RuntimeError('Index service exit failed')
        bad=env.copy();bad['ALIA_EMBEDDING_DIR']=str(ROOT/'data/models/no-such-assets')
        failed=subprocess.run([str(application)],cwd=ROOT,env=bad,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=15)
        assert failed.returncode!=0,'Missing assets silently enabled an unusable encoder'
        report.update(passed=True,missing_assets='startup rejected',scope='Queue, shared HTTP retrieval and service integration; frozen corpus and full M2 acceptance not run')
        print('Index service PASS: actual CPU E5, durable queue, two service processes, zero cloud calls, missing assets rejected')
    finally:
        output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
        assert database.startswith('alia_semantic_test_') and len(database)==51
        subprocess.run(psql+[f'DROP DATABASE {database} WITH (FORCE)'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
if __name__=='__main__':main()
