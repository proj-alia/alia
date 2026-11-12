#!/usr/bin/env python3
"""Run actual local E5 and vector storage on synthetic, isolated data; no LLM."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import uuid
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--evaluate',action='store_true')
parser.add_argument('--shared',action='store_true')
parser.add_argument('--output',type=Path)
args=parser.parse_args()
if args.shared and not args.evaluate:parser.error('--shared requires --evaluate')
output=args.output or ROOT/'evals/results'/('retrieval-shared-v1.json' if args.shared else 'retrieval-baseline-v1.json' if args.evaluate else 'semantic-smoke.json')
if output.exists():parser.error('Report already exists; select a new --output to preserve evidence')
output.parent.mkdir(parents=True,exist_ok=True)
if args.evaluate:subprocess.run(['python3',str(ROOT/'scripts/check_retrieval_corpus.py')],check=True)
values=dict(line.split('=',1) for line in (ROOT/'.env').read_text().splitlines() if line.strip() and not line.startswith('#'))
database='alia_semantic_test_'+uuid.uuid4().hex
env=os.environ.copy()
env['DATABASE_URL']=f"postgres://alia:{values['ALIA_DB_PASSWORD']}@127.0.0.1:{values.get('ALIA_DB_PORT','55433')}/{database}"
psql=['sudo','-n','docker','compose','exec','-T','db','psql','-U','alia','-d','postgres','-v','ON_ERROR_STOP=1','-c']
subprocess.run(psql+[f'CREATE DATABASE {database}'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
try:
    result=subprocess.run([str(ROOT/'target/debug/examples'/('evaluate_retrieval' if args.evaluate else 'verify_semantic_store')),str(ROOT/'data/models/multilingual-e5-small')]+(['--shared'] if args.shared else []),cwd=ROOT,env=env,capture_output=True,text=True,timeout=180)
    report={'exit':result.returncode,'reports':[json.loads(line) for line in result.stdout.splitlines() if line.strip()]}
    output.write_text(json.dumps(report,indent=2)+'\n')
    if result.returncode:raise RuntimeError('Semantic smoke failed; provider stderr withheld')
    print('Frozen retrieval baseline measured; inspect report, not an automatic acceptance' if args.evaluate else 'Real local encoder/PostgreSQL smoke PASS; frozen recall corpus not used')
finally:
    assert database.startswith('alia_semantic_test_') and len(database)==51
    subprocess.run(psql+[f'DROP DATABASE {database} WITH (FORCE)'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
