#!/usr/bin/env python3
"""Run source-declared accepted browser cases on frozen, exclusive QA inputs."""
import argparse
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import uuid
from urllib.error import URLError
from urllib.request import urlopen

from artifacts import verify_artifacts
from common import clean_env, digest, resource_locks, read_json, safe_relative, utc, write_new
from database import DOCKER, assert_owned, load_state, sql, url
from freeze import inventory, verify
from run_engineering import redact

SLICES = {
    'p02-02': '7199cedcaa00f2b7e12354a21963233cb816aeb2adf00c73f516ef427bff16fc',
    'p04-01': 'b98460dc7d90490ef2eb76e81adb0d09b5c96d7c1c655a3c5920f45b1b6acf8d',
    'p05-01': 'b05c26944db0c9a6a95481770b74a2a330090250213299320ac4e2fc57115695',
}
OPTIONAL_SLICES = {
    'p02-03': '83c35098c3841d90403ee534d0426dcce3bb184467dafd8ba942ee23db9830f5',
    'p04-02': '9a8b770f71846f7041e540ac27788cefaf8d8766b626a4212508ecc1f16fa184',
    'p04-03': '14c1526d566640054a3f8f5466fe76347d07364b0da370ff1d9f859b09ab0093',
    'p05-02': 'bca580b8f447c5c2d806a556c11744e33945346a1762e3ad40b3876120739e0a',
    'p02-04': '22bc75e7a0bd59aa701229f232e78f0f82bd9003ba85300cacc40c05f29c1fa3',
    'p04-04': '258ae74c51ca4b9c8eff2f3e6f5217ba498bd7380a4a0d83d9ea57d2df79d98f',
    'p05-03': '2795f9db3ae4a8d778d9dda5232130d010574ccda2cb92574f3bdc95d17207d5',
    'p02-05': 'ee553288cfe5c2437c6b4635d1c40f6bdfdf83617f7b8c1953f303c7bf036b58',
    'p02-07': '234a4608bab9b263db68454c5624b20419f344a0b11aa5fec82f7bb5181b797f',
}
# Root's two explicitly accepted successor envelopes bind the original frozen
# source manifest/report instead of repeating legacy format/id fields. Only
# these fixed names/pins may use that closed shape; no live-source fallback.
SUCCESSOR_SCRIPTS = {'p02-05': 'web/scripts/test-memory-deletion.mjs',
                     'p02-07': 'web/scripts/test-sse-privacy.mjs'}


def accepted_identity(name, accepted):
    if name not in SUCCESSOR_SCRIPTS:
        return accepted.get('format') == 'alia-accepted-slice/1' and accepted.get('id') == name
    if set(accepted) != {'scope', 'sources', 'source_manifest', 'report'} or not isinstance(
            accepted.get('scope'), str) or not accepted['scope'].strip():
        return False
    for key in ('source_manifest', 'report'):
        value = accepted[key]
        if not isinstance(value, dict) or set(value) != {'path', 'sha256'}:
            return False
        if not isinstance(value['path'], str):
            return False
        safe_relative(value['path'])
        if not isinstance(value['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', value['sha256']):
            return False
    return True
CASES = {
    'overview': {'slice': 'p02-02', 'script': 'web/scripts/test-overview.mjs',
                 'prefix': 'ALIA_P02', 'task': 'ALIA-P02-02', 'groups': 9},
    'documents': {'slice': 'p05-01', 'script': 'web/scripts/test-documents.mjs',
                  'prefix': 'ALIA_P05', 'task': 'ALIA-P05-01', 'groups': 8},
    'memory-revisions': {'slice': 'p02-03', 'script': 'web/scripts/test-memory-revisions.mjs',
        'prefix': 'ALIA_P02', 'task': 'ALIA-P02-03', 'groups': 15,
        'layers': ('p02-03', 'p04-02'),
        'report_files': ('web/src/features/Memory.tsx', 'web/src/app/App.tsx',
                         'web/src/app/useConsoleNavigation.ts', 'web/scripts/test-memory-revisions.mjs'),
        'report_dirs': ('web/src/features/memory/',)},
    'source-inspector': {'slice': 'p02-04', 'script': 'web/scripts/test-source-inspector.mjs',
        'prefix': 'ALIA_P02', 'task': 'ALIA-P02-04', 'groups': 16, 'check_shape': 'strings',
        'layers': ('p02-03', 'p04-02', 'p04-03', 'p02-04'),
        'report_files': ('web/src/features/memory/MemoryInspector.tsx',
                         'web/scripts/test-source-inspector.mjs'),
        'report_dirs': ('web/src/features/sources/',)},
    'beliefs': {'slice': 'p04-04', 'script': 'web/scripts/test-beliefs.mjs',
        'prefix': 'ALIA_P04', 'task': 'ALIA-P04-04', 'groups': 10,
        'layers': ('p02-03', 'p04-02', 'p04-03', 'p02-04', 'p04-04'),
        'report_files': ('src/adapters/beliefs_view.rs', 'src/service/beliefs_view.rs',
            'web/src/features/Cognition.tsx', 'web/src/features/beliefs/BeliefsWorkspace.tsx',
            'web/src/features/beliefs/useBeliefsRead.ts', 'web/src/features/beliefs/beliefs.css',
            'web/scripts/test-beliefs.mjs')},
    'relationships': {'slice': 'p05-03', 'script': 'web/scripts/test-relationships.mjs',
        'prefix': 'ALIA_P05', 'task': 'ALIA-P05-03', 'groups': 9,
        'layers': ('p05-03',),
        'report_files': ('src/adapters/relationships_view.rs', 'src/service/relationships_view.rs',
            'web/src/features/Relations.tsx', 'web/src/features/relationships/useRelationshipsRead.ts')},
    'notes': {'slice': 'p05-02', 'script': 'web/scripts/test-notes.mjs',
        'prefix': 'ALIA_P05', 'task': 'ALIA-P05-02', 'groups': 8,
        'layers': ('p05-02',),
        'report_files': ('src/adapters/notes_view.rs', 'src/service/notes_view.rs',
            'web/src/features/Notes.tsx', 'web/src/features/notes/useNotesRead.ts',
            'web/src/features/Settings.tsx')},
}


def declared_cases(manifest):
    return tuple(name for name, case in CASES.items()
                 if name in ('overview', 'documents') or case['script'] in manifest['sources'])


def accepted_bindings(manifest, source):
    """Validate pinned manifests, then their ordered final per-path composition."""
    needed = set(SLICES)
    for name in declared_cases(manifest):
        needed.update(CASES[name].get('layers', ()))
    for name in OPTIONAL_SLICES:
        if f'evals/p03/accepted-slices/{name}.json' in manifest['sources']:
            needed.add(name)
    for name, script in SUCCESSOR_SCRIPTS.items():
        if script in manifest['sources']:
            needed.add(name)
    if 'p02-07' in needed:
        needed.add('p02-05')
    bindings, expected = {}, {}
    for name, sha in {**SLICES, **OPTIONAL_SLICES}.items():
        if name not in needed:
            continue
        relative = f'evals/p03/accepted-slices/{name}.json'
        path = Path(source) / relative
        if manifest['sources'].get(relative) != sha or not path.is_file() or digest(path) != sha:
            raise ValueError('accepted manifest binding mismatch: ' + name)
        accepted = read_json(path)
        if not isinstance(accepted, dict) or not accepted_identity(name, accepted):
            raise ValueError('invalid accepted manifest: ' + name)
        files = accepted.get('sources')
        if not isinstance(files, dict) or not files:
            raise ValueError('accepted sources absent: ' + name)
        if name in SUCCESSOR_SCRIPTS and SUCCESSOR_SCRIPTS[name] not in files:
            raise ValueError('accepted successor browser script absent: ' + name)
        for file, value in files.items():
            safe_relative(file)
            if not isinstance(value, str) or not re.fullmatch('[0-9a-f]{64}', value):
                raise ValueError('invalid accepted source digest')
        if name == 'p04-01' and 'tests/memory_revisions.rs' not in files:
            raise ValueError('P04 HTTP/PG regression test absent')
        for case in CASES.values():
            if case['slice'] == name and case['script'] not in files:
                raise ValueError('accepted browser script absent')
        expected.update(files)
        bindings[name] = {'manifest_sha256': sha, 'source_count': len(files)}
    for file, value in expected.items():
        if manifest['sources'].get(file) != value:
            raise ValueError('candidate missing or changed accepted source: ' + file)
    return bindings


def validate_inputs(checkpoint, source):
    """Pins include all accepted files, including P04's automatically collected test."""
    manifest = verify(checkpoint)
    if inventory(source) != manifest['sources']:
        raise ValueError('execution source differs from checkpoint')
    return manifest, accepted_bindings(manifest, source)


def browser_environment(environment, state, database, token, base, binary, output, case, locks):
    env = clean_env()  # never propagate deployment/model credentials from callers
    env.update({k: v for k, v in environment.items() if k in env or k in {
        'PATH', 'CHROMIUM_EXECUTABLE', 'PLAYWRIGHT_BROWSERS_PATH'}})
    prefix = CASES[case]['prefix']
    env.update(ALIA_TEST_URL=base, ALIA_TEST_TOKEN=token, ALIA_TEST_BINARY=str(binary),
               ALIA_TEST_OUTPUT=str(output))
    if isinstance(locks, int):  # compatibility for direct environment unit fixtures
        env['ALIA_QA_LOCK_FD'] = str(locks)
    else:
        env.update(locks.child_environment(env))
    env[prefix + '_DATABASE'] = database
    env[prefix + '_LOCK_HELD'] = '1'
    env[prefix + '_PSQL_COMMAND'] = json.dumps(DOCKER + ['exec', '-i', state['container'],
        'psql', '-X', '-A', '-t', '-U', state['username'], '-d', database])
    return env


def wait_ready(process, base):
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError('service exited before readiness')
        try:
            with urlopen(base + '/healthz', timeout=2) as response:
                if response.status == 200:
                    return
        except (URLError, TimeoutError):
            pass
        time.sleep(.1)
    raise RuntimeError('service readiness timeout')


def stop_process(process):
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
    forced = False
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        forced = True
        process.kill()
        process.wait()
    return {'passed': process.returncode == 0 and not forced,
            'exit_code': process.returncode, 'forced_kill': forced}


def export_child(private_output, public_output, secrets_to_hide):
    public_output.mkdir()
    path = private_output / 'report.json'
    if path.is_symlink() or not path.is_file():
        raise ValueError('child report missing')
    raw = path.read_text()
    if len(raw.encode()) > 1024 * 1024:
        raise ValueError('child report exceeds evidence limit')
    sanitized = redact(raw, secrets_to_hide)
    try:
        child = read_json(path)
        json.loads(sanitized)
    except (ValueError, TypeError):
        with (public_output / 'invalid-report.txt').open('x') as f:
            f.write(sanitized)
        raise ValueError('child report invalid') from None
    if not isinstance(child, dict):
        raise ValueError('child report must be an object')
    write_new(public_output / 'report.json', json.loads(sanitized))
    # If credentials appeared in text, do not export potentially unsafe screenshots.
    if sanitized == raw:
        for image in private_output.glob('*.png'):
            if image.is_symlink():
                raise ValueError('child screenshot symlink refused')
            with image.open('rb') as src, (public_output / image.name).open('xb') as dst:
                shutil.copyfileobj(src, dst)
    return child


def child_passed(child, case, binary_sha, source_manifest):
    config = CASES[case]
    if not isinstance(child, dict):
        return False
    if 'model_calls' in child and (type(child['model_calls']) is not int or child['model_calls'] != 0):
        return False
    if 'cleanup_errors' in child and not isinstance(child['cleanup_errors'], list):
        return False
    checks = child.get('checks')
    if (child.get('task') != config['task'] or child.get('passed') is not True or
            child.get('cleanup') is not True or child.get('model_calls', 0) != 0 or
            child.get('binary_sha256') != binary_sha or not isinstance(checks, list) or
            len(checks) < config['groups'] or child.get('sql_fault_cleanup') is False or
            child.get('cleanup_errors')):
        return False
    if config.get('check_shape') == 'strings':
        if not all(isinstance(c, str) and c.strip() for c in checks):
            return False
        names = checks
    else:
        if not all(isinstance(c, dict) and isinstance(c.get('name'), str) and c['name'].strip()
                   and c.get('passed', True) is True and
                   c.get('status') not in ('failed', 'unverified', 'skipped') for c in checks):
            return False
        names = [c['name'] for c in checks]
    if len(set(names)) != len(names):
        return False
    reported_sources = child.get('sources')
    if not isinstance(reported_sources, dict) or not reported_sources or not all(
            isinstance(p, str) and p in source_manifest['sources'] and isinstance(h, str) and
            re.fullmatch('[0-9a-f]{64}', h) and source_manifest['sources'][p] == h
            for p, h in reported_sources.items()):
        return False
    if 'report_files' in config:
        required = set(config['report_files'])
        for prefix in config.get('report_dirs', ()):
            required.update(p for p in source_manifest['sources']
                            if p.startswith(prefix) and '/' not in p[len(prefix):])
        if set(reported_sources) != required:
            return False
    return True


def run_case(args):
    args.output.mkdir(parents=True)  # no old evidence overwritten
    report = {'format': 'alia-p03-management-browser/1', 'case': args.case,
              'started_at': utc(), 'passed': False, 'cleanup': [], 'model_calls': 0}
    service = child_process = runtime = None
    attempted_database = False
    verified = False
    secrets_to_hide = []
    state = load_state(args.test_db_state)
    database = 'alia_p03_' + uuid.uuid4().hex
    with resource_locks(('database', state.get('container_id', 'unknown-test-db')),
                        ('browser', str(args.output))) as locks:
        report['queue_seconds'] = locks.queue_seconds
        try:
            manifest, bindings = validate_inputs(args.checkpoint, args.source_root)
            if args.case not in declared_cases(manifest):
                raise ValueError('browser case source not declared in checkpoint')
            verify_artifacts(args.checkpoint, args.binary_dir, args.web_dir)
            if state['role'] != 'engineering' or state.get('status') != 'ready':
                raise ValueError('management browser requires ready engineering DB')
            if assert_owned(state, 'container') is None or assert_owned(state, 'volume') is None:
                raise ValueError('owned test DB resource absent')
            verified = True
            binary = args.binary_dir / 'alia'
            report.update(checkpoint=manifest['id'], checkpoint_sha256=digest(args.checkpoint / 'manifest.json'),
                          accepted_slices=bindings, binary_sha256=digest(binary),
                          script=CASES[args.case]['script'], database=database)
            token = secrets.token_urlsafe(40)
            secrets_to_hide = [token, state['password'], url(state, database)]
            runtime = tempfile.TemporaryDirectory(prefix='alia-p03-management-')
            private = Path(runtime.name)
            (private / 'data').mkdir(mode=0o700)
            env = clean_env()
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0))
                port = sock.getsockname()[1]
            base = f'http://127.0.0.1:{port}'
            attempted_database = True  # cleanup exact generated name even on lost CREATE response
            sql(state, f'CREATE DATABASE {database};')
            service_env = dict(env, DATABASE_URL=secrets_to_hide[-1], ALIA_INSTANCE_TOKEN=token,
                ALIA_WEB_DIR=str(args.web_dir), ALIA_DATA_DIR=str(private / 'data'),
                ALIA_BIND=f'127.0.0.1:{port}')
            service = subprocess.Popen([str(binary)], cwd=args.source_root,
                                       env=locks.child_environment(service_env), pass_fds=locks.pass_fds,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            wait_ready(service, base)
            child_env = browser_environment(env, state, database, token, base, binary,
                                            private / 'browser', args.case, locks)
            child_process = subprocess.Popen(['node', CASES[args.case]['script']], cwd=args.source_root,
                env=child_env, pass_fds=locks.pass_fds, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                text=True, errors='replace')
            try:
                stdout, stderr = child_process.communicate(timeout=240)
            except subprocess.TimeoutExpired:
                child_process.terminate()
                try:
                    stdout, stderr = child_process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    child_process.kill()
                    stdout, stderr = child_process.communicate()
                report['child_timeout'] = True
            for name, content in [('child-stdout.log', stdout), ('child-stderr.log', stderr)]:
                with (args.output / name).open('x') as f:
                    f.write(redact(content, secrets_to_hide))
            report['child_exit'] = child_process.returncode
            child = export_child(private / 'browser', args.output / 'browser', secrets_to_hide)
            report['child_checks'] = len(child.get('checks', []))
            report['passed'] = (not report.get('child_timeout') and child_process.returncode == 0 and
                               child_passed(child, args.case, report['binary_sha256'], manifest))
            if not report['passed']:
                report['failure'] = 'browser exit, assertions, cleanup or provenance failed'
        except (Exception, KeyboardInterrupt) as error:
            report['failure'] = redact(type(error).__name__ + ': ' + str(error), secrets_to_hide)
        finally:
            if child_process and child_process.poll() is None:
                try:
                    report['cleanup'].append({'resource': 'browser-child', **stop_process(child_process)})
                except Exception as error:
                    report['cleanup'].append({'resource': 'browser-child', 'passed': False, 'error': type(error).__name__})
            if service:
                try:
                    report['cleanup'].append({'resource': 'service', **stop_process(service)})
                except Exception as error:
                    report['cleanup'].append({'resource': 'service', 'passed': False, 'error': type(error).__name__})
            if attempted_database:
                try:
                    sql(state, f'DROP DATABASE IF EXISTS {database} WITH (FORCE);')
                    report['cleanup'].append({'resource': 'database', 'passed': True, 'name': database})
                except Exception as error:
                    report['cleanup'].append({'resource': 'database', 'passed': False, 'name': database, 'error': type(error).__name__})
            if runtime:
                try:
                    runtime.cleanup()
                    report['cleanup'].append({'resource': 'private-runtime', 'passed': True})
                except Exception as error:
                    report['cleanup'].append({'resource': 'private-runtime', 'passed': False, 'error': type(error).__name__})
            if verified:
                try:
                    validate_inputs(args.checkpoint, args.source_root)
                    verify_artifacts(args.checkpoint, args.binary_dir, args.web_dir)
                    report['final_integrity'] = True
                except Exception as error:
                    report['final_integrity'] = False
                    report['integrity_error'] = type(error).__name__
            report['passed'] = (report['passed'] and report.get('final_integrity', False) and
                                all(c['passed'] for c in report['cleanup']))
            report['ended_at'] = utc()
            write_new(args.output / 'parent-report.json', report)
    print(json.dumps({'passed': report['passed'], 'report': str(args.output / 'parent-report.json')}))
    return 0 if report['passed'] else 1


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--checkpoint', type=Path, required=True)
    p.add_argument('--source-root', type=Path, required=True)
    p.add_argument('--verify-inputs-only', action='store_true')
    p.add_argument('--case', choices=sorted(CASES))
    p.add_argument('--binary-dir', type=Path)
    p.add_argument('--web-dir', type=Path)
    p.add_argument('--test-db-state', type=Path)
    p.add_argument('--output', type=Path)
    args = p.parse_args()
    if args.verify_inputs_only:
        _, bindings = validate_inputs(args.checkpoint, args.source_root)
        print(json.dumps({'accepted_management_inputs_verified': bindings}))
        return 0
    if not all((args.case, args.binary_dir, args.web_dir, args.test_db_state, args.output)):
        p.error('case, frozen binary/web, private test DB state and fresh output required')
    return run_case(args)

if __name__ == '__main__':
    raise SystemExit(main())
