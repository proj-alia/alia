#!/usr/bin/env python3
"""Three bounded live cycles across fresh processes. Synthetic test data only.
Uses the user's explicit Go credential section; no production individual changes.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import uuid
from probe_model import read_go_key

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--credentials', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'evals/results/live-individual.json')
    args = parser.parse_args()
    key = read_go_key(args.credentials.read_text())
    settings = dict(line.split('=',1) for line in (ROOT / '.env').read_text().splitlines() if line.strip() and not line.startswith('#'))
    database = 'alia_live_test_' + uuid.uuid4().hex
    psql = ['sudo','-n','docker','compose','exec','-T','db','psql','-U','alia','-d','postgres','-v','ON_ERROR_STOP=1','-c']
    env = os.environ.copy()
    env.update(DATABASE_URL=f"postgres://alia:{settings['ALIA_DB_PASSWORD']}@127.0.0.1:{settings.get('ALIA_DB_PORT','55433')}/{database}",
               ALIA_MODEL_PROVIDER='opencode_go', ALIA_MODEL_ENDPOINT='https://opencode.ai/inference/go/openai/v1',
               ALIA_MODEL='glm-5.3-flash', OPENCODE_API_KEY=key)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    reports = []
    subprocess.run(psql + [f'CREATE DATABASE {database}'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
    try:
        for phase in ('first','restore','unknown'):
            env['ALIA_LIVE_PHASE'] = phase
            result = subprocess.run([str(ROOT/'target/debug/examples/live_individual')],cwd=ROOT,env=env,capture_output=True,text=True,timeout=180)
            for line in result.stdout.splitlines():
                reports.append(json.loads(line))
            args.output.write_text(json.dumps({'reports':reports,'latest_exit':result.returncode,'behavior_review':'pending'},ensure_ascii=False,indent=2))
            print(json.dumps({'phase':phase,'exit':result.returncode,'report':str(args.output)}),flush=True)
            if result.returncode:
                # Never print arbitrary provider/driver stderr or env credentials.
                raise RuntimeError('Isolated live cycle failed; inspect synthetic report')
        assert len(reports)==3 and len({r['individual_id'] for r in reports})==1
        assert len({r['replies'][0]['conversation_id'] for r in reports})==3
        print('Live managed persistence and usage checks PASS; semantic behavior still requires review')
    finally:
        assert database.startswith('alia_live_test_') and len(database)==47
        subprocess.run(psql+[f'DROP DATABASE {database} WITH (FORCE)'],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)

if __name__=='__main__':
    main()
