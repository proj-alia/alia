#!/usr/bin/env python3
"""Real browser + two service processes on a disposable database; zero model calls.

Frozen mode requires explicit checkpoint, immutable artifacts, isolated test DB,
execution source and a fresh output directory. Legacy defaults remain available.
"""
import argparse
import json
import os
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

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
from common import clean_env, resource_locks, utc, write_new
from artifacts import verify_artifacts
from database import load_state, sql, url
from freeze import inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint', type=Path)
    parser.add_argument('--binary-dir', type=Path)
    parser.add_argument('--web-dir', type=Path)
    parser.add_argument('--source-root', type=Path)
    parser.add_argument('--test-db-state', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    explicit = (args.binary_dir, args.web_dir, args.source_root, args.test_db_state)
    if args.checkpoint and not all(explicit):
        parser.error('frozen mode requires binary/web/source/test-db directories')
    if not args.checkpoint and any(explicit):
        parser.error('explicit resources require --checkpoint')
    database_scope = (load_state(args.test_db_state).get('container_id', 'unknown-test-db')
                      if args.test_db_state else 'legacy-compose')
    browser_scope = str(args.output.resolve()) if args.output else str(ROOT)
    resources = [('database', database_scope), ('browser', browser_scope)]
    if not args.checkpoint:
        resources.append(('target', ROOT / 'target'))
    with resource_locks(*resources) as locks:
        return run(args, locks)


def run(args, locks):
    env = locks.child_environment(clean_env())
    if args.checkpoint:
        manifest, _ = verify_artifacts(args.checkpoint, args.binary_dir, args.web_dir)
        root = args.source_root.resolve()
        if inventory(root) != manifest['sources']:
            raise ValueError('browser execution source differs from checkpoint')
        state = load_state(args.test_db_state)
        if state['role'] != 'engineering':
            raise ValueError('browser requires engineering test resource')
        binary, seed = args.binary_dir / 'alia', args.binary_dir / 'examples/seed_web_inspection'
        web = args.web_dir
        execute_sql = lambda statement: sql(state, statement)
    else:
        root = ROOT
        subprocess.run(['cargo', 'build', '--locked', '--example', 'seed_web_inspection'],
                       cwd=root, env=env, pass_fds=locks.pass_fds, check=True)
        settings = dict(line.split('=', 1) for line in (root / '.env').read_text().splitlines()
                        if line.strip() and not line.startswith('#'))
        state = {'username': 'alia', 'password': settings['ALIA_DB_PASSWORD'],
                 'port': settings.get('ALIA_DB_PORT', '55433')}
        binary, seed, web = root / 'target/debug/alia', root / 'target/debug/examples/seed_web_inspection', root / 'web/dist'
        def execute_sql(statement):
            subprocess.run(['sudo', '-n', 'docker', 'compose', 'exec', '-T', 'db', 'psql', '-U', 'alia',
                            '-d', 'postgres', '-v', 'ON_ERROR_STOP=1'], input=statement,
                           cwd=root, env=env, text=True, check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
    output = (args.output or Path(os.environ.get('ALIA_TEST_OUTPUT', str(ROOT / 'evals/results/web-v4')))).resolve()
    output.mkdir(parents=True)  # refuses any old browser evidence directory
    report = {'task': 'ALIA-P03-browser', 'started_at': utc(), 'phases': [], 'passed': False,
              'cleanup': [], 'model_calls': 0}
    db = 'alia_web_test_' + uuid.uuid4().hex
    token = secrets.token_urlsafe(40)
    environment = env.copy()
    environment.update(ALIA_INSTANCE_TOKEN=token, ALIA_WEB_DIR=str(web))
    environment['DATABASE_URL'] = (url(state, db) if args.checkpoint else
        f"postgres://alia:{state['password']}@127.0.0.1:{state['port']}/{db}")
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    base = f'http://127.0.0.1:{port}'
    environment['ALIA_BIND'] = f'127.0.0.1:{port}'
    test_environment = env.copy()
    test_environment.update(ALIA_TEST_URL=base, ALIA_TEST_TOKEN=token, ALIA_TEST_OUTPUT=str(output))
    created = False
    try:
        execute_sql(f'CREATE DATABASE {db};')
        created = True
        with tempfile.TemporaryDirectory(prefix='alia-p03-browser-') as data:
            environment['ALIA_DATA_DIR'] = data
            for phase in ('create', 'restore'):
                phase_record = {'phase': phase, 'started_at': utc()}
                report['phases'].append(phase_record)
                if phase == 'restore':
                    subprocess.run([str(seed)], cwd=root, env=environment, check=True,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                process = subprocess.Popen([str(binary)], cwd=root, env=environment,
                                           pass_fds=locks.pass_fds,
                                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    for _ in range(100):
                        if process.poll() is not None:
                            raise RuntimeError('Isolated service exited during startup')
                        try:
                            with urlopen(base + '/healthz', timeout=2) as response:
                                if json.load(response) == {'status': 'ready'}:
                                    break
                        except (URLError, TimeoutError):
                            pass
                        time.sleep(.1)
                    else:
                        raise RuntimeError('Isolated service readiness timeout')
                    test_environment['ALIA_TEST_PHASE'] = phase
                    result = subprocess.run(['node', 'web/scripts/smoke.mjs'], cwd=root,
                                            env=locks.child_environment(test_environment),
                                            pass_fds=locks.pass_fds)
                    phase_record['exit_code'] = result.returncode
                    if result.returncode:
                        raise RuntimeError('Browser phase failed: ' + phase)
                finally:
                    if process.poll() is None:
                        process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                        phase_record['shutdown'] = 'forced_kill'
                    else:
                        phase_record['shutdown'] = 'graceful' if process.returncode == 0 else 'failed'
                    phase_record['service_exit'] = process.returncode
                    phase_record['ended_at'] = utc()
                if phase_record['shutdown'] != 'graceful':
                    raise RuntimeError('Service did not shut down gracefully')
        report['passed'] = True
    except Exception as error:
        report['error'] = type(error).__name__ + ': ' + str(error).replace(token, '[redacted]').replace(state['password'], '[redacted]')
    finally:
        if created:
            try:
                execute_sql(f'DROP DATABASE {db} WITH (FORCE);')
                report['cleanup'].append({'database': db, 'passed': True})
            except Exception as error:
                report['cleanup'].append({'database': db, 'passed': False, 'error': type(error).__name__})
                report['passed'] = False
        report['ended_at'] = utc()
        write_new(output / 'driver-report.json', report)
    print(json.dumps({'browser_passed': report['passed'], 'report': str(output / 'driver-report.json')}))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
