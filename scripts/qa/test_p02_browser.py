#!/usr/bin/env python3
"""Supply isolated frozen service/database to P02's external browser-only mode."""
import argparse
import json
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from urllib.request import urlopen
from urllib.error import URLError
from artifacts import verify_artifacts
from common import clean_env, resource_locks, utc, write_new
from database import DOCKER, load_state, sql, url
from freeze import inventory


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--checkpoint',type=Path,required=True)
    p.add_argument('--binary-dir',type=Path,required=True)
    p.add_argument('--web-dir',type=Path,required=True)
    p.add_argument('--source-root',type=Path,required=True)
    p.add_argument('--test-db-state',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    manifest,_=verify_artifacts(args.checkpoint,args.binary_dir,args.web_dir)
    if inventory(args.source_root)!=manifest['sources']:
        p.error('execution source differs from checkpoint')
    state=load_state(args.test_db_state)
    if state['role']!='engineering':
        p.error('P02 browser requires engineering resource')
    args.output.mkdir(parents=True)
    report={'task':'ALIA-P03-P02-browser-parent','started_at':utc(),'model_calls':0,
            'passed':False,'cleanup':[]}
    database='alia_p03_'+uuid.uuid4().hex
    token=secrets.token_urlsafe(40)
    env=clean_env()
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0))
        port=sock.getsockname()[1]
    base=f'http://127.0.0.1:{port}'
    created=False
    process=None
    with resource_locks(('database',state.get('container_id','unknown-test-db')),
                        ('browser',str(args.output))) as locks:
        try:
            sql(state,f'CREATE DATABASE {database};')
            created=True
            with tempfile.TemporaryDirectory(prefix='alia-p03-p02-') as data:
                service_env=env.copy()
                service_env.update(DATABASE_URL=url(state,database),ALIA_INSTANCE_TOKEN=token,
                    ALIA_WEB_DIR=str(args.web_dir),ALIA_DATA_DIR=data,ALIA_BIND=f'127.0.0.1:{port}')
                process=subprocess.Popen([str(args.binary_dir/'alia')],cwd=args.source_root,
                                         env=locks.child_environment(service_env),pass_fds=locks.pass_fds,
                                         stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError('isolated service exited during startup')
                    try:
                        with urlopen(base+'/healthz',timeout=2) as response:
                            if response.status==200:
                                break
                    except (URLError,TimeoutError):
                        pass
                    time.sleep(.1)
                else:
                    raise RuntimeError('isolated service readiness timeout')
                browser_env=env.copy()
                browser_env.update(ALIA_TEST_URL=base,ALIA_TEST_TOKEN=token,ALIA_TEST_BINARY=str(args.binary_dir/'alia'),
                    ALIA_TEST_OUTPUT=str(args.output/'browser'),ALIA_P02_DATABASE=database,ALIA_P02_LOCK_HELD='1',
                    ALIA_P02_PSQL_COMMAND=json.dumps(DOCKER+['exec','-i',state['container'],'psql','-X',
                        '-U',state['username'],'-d',database]))
                browser_env.update(locks.child_environment(browser_env))
                result=subprocess.run(['node','web/scripts/test-data-workspace.mjs'],cwd=args.source_root,
                                      env=browser_env,pass_fds=locks.pass_fds)
                report['browser_exit']=result.returncode
                report['passed']=result.returncode==0
        except Exception as error:
            report['error']=type(error).__name__
        finally:
            if process:
                try:
                    if process.poll() is None:
                        process.send_signal(signal.SIGTERM)
                    process.wait(timeout=5)
                    report['cleanup'].append({'service':True,'exit_code':process.returncode})
                    if process.returncode!=0:
                        report['passed']=False
                except subprocess.TimeoutExpired:
                    process.kill();process.wait()
                    report['cleanup'].append({'service':False,'forced_kill':True})
                    report['passed']=False
            if created:
                try:
                    sql(state,f'DROP DATABASE {database} WITH (FORCE);')
                    report['cleanup'].append({'database':True})
                except Exception as error:
                    report['cleanup'].append({'database':False,'error':type(error).__name__})
                    report['passed']=False
            report['ended_at']=utc()
            write_new(args.output/'parent-report.json',report)
    print(json.dumps({'passed':report['passed'],'report':str(args.output/'parent-report.json')}))
    return 0 if report['passed'] else 1

if __name__=='__main__':
    raise SystemExit(main())
