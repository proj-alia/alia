#!/usr/bin/env python3
"""Bounded live-provider research in a disposable DB, no test completion writes."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import uuid
from probe_model import read_go_key
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser()
    p.add_argument('--credentials',type=Path,required=True)
    p.add_argument('--output',type=Path,default=ROOT/'evals/results/live-autonomy.json')
    a=p.parse_args()
    settings=dict(line.split('=',1) for line in (ROOT/'.env').read_text().splitlines() if line.strip() and not line.startswith('#'))
    database='alia_live_test_'+uuid.uuid4().hex
    env=os.environ.copy()
    env.update(DATABASE_URL=f"postgres://alia:{settings['ALIA_DB_PASSWORD']}@127.0.0.1:{settings.get('ALIA_DB_PORT','55433')}/{database}",ALIA_MODEL_PROVIDER='opencode_go',ALIA_MODEL_ENDPOINT='https://opencode.ai/inference/go/openai/v1',ALIA_MODEL='glm-5.3-flash',OPENCODE_API_KEY=read_go_key(a.credentials.read_text()))
    psql=['sudo','-n','docker','compose','exec','-T','db','psql','-U','alia','-d','postgres','-v','ON_ERROR_STOP=1','-c']
    a.output.parent.mkdir(parents=True,exist_ok=True)
    subprocess.run(psql+[f'CREATE DATABASE {database}'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
    try:
        result=subprocess.run([str(ROOT/'target/debug/examples/live_autonomy')],cwd=ROOT,env=env,capture_output=True,text=True,timeout=1000)
        reports=[json.loads(line) for line in result.stdout.splitlines() if line.strip()]
        diagnostics=[line.removeprefix('Live isolated verification failed: ') for line in result.stderr.splitlines() if line.startswith('Live isolated verification failed: ')]
        allowed={'runtime invalid','worker storage failure','autonomous completion not demonstrated','migration failed','init failed','model not ready','model config'}
        failure=next((code for code in diagnostics if code in allowed),None)
        a.output.write_text(json.dumps({'reports':reports,'exit':result.returncode,'failure_code':failure},ensure_ascii=False,indent=2))
        print(json.dumps({'exit':result.returncode,'report':str(a.output),'reports':len(reports)}))
        if result.returncode:raise RuntimeError('Live autonomy did not pass; inspect synthetic report')
        print('Managed autonomous completion PASS; semantic review remains manual')
    finally:
        assert database.startswith('alia_live_test_') and len(database)==47
        subprocess.run(psql+[f'DROP DATABASE {database} WITH (FORCE)'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
if __name__=='__main__':main()
