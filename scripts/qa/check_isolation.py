#!/usr/bin/env python3
"""Real two-Postgres isolation and fail-closed identity/cleanup evidence. No models."""
import argparse
from pathlib import Path
import json
import os
import tempfile
import time
from common import resource_locks, utc, write_new
from database import provision, cleanup, sql, public, read_json, save_private, ProvisionError, load_state


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2])
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.mkdir()
    report={'task':'ALIA-P03-isolation','started_at':utc(),'checks':[],'resources':[],
            'cleanup':[],'passed':False,'model_calls':0}
    states=[]
    with resource_locks(('database-admin','shared')) as locks:
        report['queue_seconds']=locks.queue_seconds
        start=time.monotonic()
        try:
            for role in ('engineering','behavior'):
                try:
                    path,state=provision(args.root,role)
                except ProvisionError as error:
                    states.append((error.state_path,load_state(error.state_path)))
                    raise
                states.append((path,state))
                report['resources'].append(public(state))
            eng,beh=states[0][1],states[1][1]
            assert eng['port']!=beh['port'] and eng['volume']!=beh['volume']
            sql(eng,"CREATE TABLE p03_marker(value text); INSERT INTO p03_marker VALUES('engineering');")
            assert sql(beh,"SELECT count(*) FROM information_schema.tables WHERE table_name='p03_marker';")=='0'
            sql(beh,"CREATE TABLE p03_marker(value text); INSERT INTO p03_marker VALUES('behavior');")
            assert sql(eng,'SELECT value FROM p03_marker;')=='engineering'
            assert sql(beh,'SELECT value FROM p03_marker;')=='behavior'
            report['checks'].append({'name':'two-independent-ports-volumes-and-data','passed':True})
            # Corrupted identity cannot remove even an otherwise owned container.
            forged=dict(eng,container_id='not-the-created-container')
            forged_path=states[0][0].parent/'forged.json'
            write_new(forged_path,forged,private=True)
            rejected=False
            try:
                cleanup(forged_path)
            except ValueError:
                rejected=True
            assert rejected and sql(eng,'SELECT value FROM p03_marker;')=='engineering'
            forged_path.unlink()  # exact private fixture created above
            report['checks'].append({'name':'wrong-container-identity-refused-no-data-loss','passed':True})
            path,state=states[-1]
            result=cleanup(path)
            report['cleanup'].append({'role':state['role'],'state_path':str(path),**result})
            assert result['passed']
            states.pop()
            assert sql(eng,'SELECT value FROM p03_marker;')=='engineering'
            report['checks'].append({'name':'behavior-cleanup-preserves-engineering','passed':True})
            # Idempotent exact-owned cleanup is safe when resources are gone.
            assert cleanup(path)['passed']
            report['checks'].append({'name':'repeat-owned-cleanup','passed':True})
            report['passed']=True
        except Exception as error:
            report['error']=type(error).__name__+': '+str(error)
        finally:
            for path,state in states:
                try:
                    result=cleanup(path)
                    report['cleanup'].append({'role':state['role'],'state_path':str(path),**result})
                    report['passed']=report['passed'] and result['passed']
                except Exception as error:
                    report['cleanup'].append({'role':state['role'],'state_path':str(path),'passed':False,'error':type(error).__name__})
                    report['passed']=False
            report['ended_at']=utc()
            report['run_seconds']=time.monotonic()-start
            write_new(args.output/'report.json',report)
    print(json.dumps({'passed':report['passed'],'report':str(args.output/'report.json')}))
    return 0 if report['passed'] else 1

if __name__=='__main__':
    raise SystemExit(main())
