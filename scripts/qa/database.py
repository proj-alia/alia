#!/usr/bin/env python3
"""Two explicitly owned disposable PostgreSQL resources. Never uses Compose state."""
import argparse
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile
import time
import uuid
from common import clean_env, resource_locks, read_json, utc, write_new

LABEL = 'io.alia.qa'
ROLES = {'engineering', 'behavior'}
DOCKER = ['sudo', '-n', 'docker']


class ProvisionError(RuntimeError):
    def __init__(self, path):
        self.state_path = Path(path)
        super().__init__(f'Test DB startup failed; private recovery state: {path}')


def docker(args, *, env=None, input=None):
    result = subprocess.run(DOCKER + args, env=env or clean_env(), input=input,
                            capture_output=True, text=True, timeout=90)
    if result.returncode:
        # Neither inspect.Env nor supplied SQL nor credentials is rendered.
        raise RuntimeError('docker operation failed: ' + args[0])
    return result.stdout.strip()


def inspect(kind, name):
    r = subprocess.run(DOCKER + [kind, 'inspect', name], capture_output=True,
                       env=clean_env(), text=True, timeout=30)
    if r.returncode:
        if re.search(r'no such (container|volume)\b', r.stderr, re.I):
            return None
        raise RuntimeError('docker identity inspection failed')
    return json.loads(r.stdout)[0]


def image_from_compose(root):
    text = (Path(root) / 'compose.yaml').read_text()
    matches = re.findall(r'^\s+image:\s*(pgvector/pgvector:[^\s]+)', text, re.M)
    if len(matches) != 1 or not re.fullmatch(r'pgvector/pgvector:[\w.-]+@sha256:[0-9a-f]{64}', matches[0]):
        raise ValueError('expected exactly one pinned pgvector image')
    return matches[0]


def labels(owner, role):
    return {LABEL + '.managed': 'p03', LABEL + '.owner': owner, LABEL + '.role': role}


def validate_state(state):
    if state.get('format') != 'alia-p03-db/1':
        raise ValueError('unsupported test DB state format')
    owner, role = state.get('owner', ''), state.get('role')
    if not re.fullmatch('[0-9a-f]{32}', owner) or role not in ROLES:
        raise ValueError('invalid QA resource owner/role')
    prefix = f'alia-p03-{owner}-{role}'
    if state.get('container') != prefix or state.get('volume') != prefix + '-data':
        raise ValueError('QA resource name/owner mismatch')
    if state.get('username') != 'aliaqa':
        raise ValueError('unexpected test DB owner username')
    return state


def load_state(path):
    path = Path(path)
    if path.is_symlink() or path.stat().st_mode & 0o077 or path.parent.stat().st_mode & 0o077:
        raise ValueError('test DB state must be private (directory 0700, file 0600)')
    return validate_state(read_json(path))


def assert_owned(state, kind):
    validate_state(state)
    obj = inspect(kind, state[kind])
    if obj is None:
        return None
    actual = obj.get('Config', {}).get('Labels', {}) if kind == 'container' else obj.get('Labels', {})
    if any(actual.get(k) != v for k, v in labels(state['owner'], state['role']).items()):
        raise ValueError('refusing cleanup/access of resource with mismatched labels')
    if kind == 'container':
        if state.get('container_id') and obj['Id'] != state['container_id']:
            raise ValueError('container identity replaced')
        if state.get('image_id') and obj.get('Image') != state['image_id']:
            raise ValueError('container image identity mismatch')
        if 'port' in state:
            bindings = obj.get('NetworkSettings', {}).get('Ports', {}).get('5432/tcp', [])
            if len(bindings) != 1 or bindings[0].get('HostIp') != '127.0.0.1' or int(bindings[0].get('HostPort', 0)) != state['port']:
                raise ValueError('test DB host port differs from owned container')
    return obj


def public(state):
    return {k: state[k] for k in ('owner', 'role', 'container', 'container_id', 'volume',
                                 'image', 'image_id', 'port', 'created_at') if k in state}


def save_private(path, state):
    # Atomic update inside private directory; no values go to stdout.
    tmp = Path(path).with_suffix('.new')
    write_new(tmp, state, private=True)
    tmp.replace(path)


def provision(root, role, directory=None):
    if role not in ROLES:
        raise ValueError('unknown DB purpose')
    directory = Path(directory or tempfile.mkdtemp(prefix='alia-p03-db-')).resolve()
    directory.mkdir(mode=0o700, exist_ok=True)
    if directory.stat().st_mode & 0o077:
        raise ValueError('DB state directory not private')
    owner = uuid.uuid4().hex
    name = f'alia-p03-{owner}-{role}'
    state = {'format': 'alia-p03-db/1', 'owner': owner, 'role': role,
             'container': name, 'volume': name + '-data', 'image': image_from_compose(root),
             'created_at': utc(), 'username': 'aliaqa', 'password': secrets.token_hex(32),
             'status': 'starting'}
    path = directory / f'{role}.json'
    write_new(path, state, private=True)
    label_args = [arg for k, v in labels(owner, role).items() for arg in ('--label', f'{k}={v}')]
    try:
        docker(['volume', 'create', *label_args, state['volume']])
        cid = docker(['run', '--detach', '--pull=never', '--name', name, *label_args,
                      '--env-file', '/dev/stdin', '--env', 'POSTGRES_USER=aliaqa',
                      '--env', 'POSTGRES_DB=postgres', '--publish', '127.0.0.1::5432',
                      '--mount', f"type=volume,src={state['volume']},dst=/var/lib/postgresql/data",
                      state['image']], input='POSTGRES_PASSWORD=' + state['password'] + '\n')
        state['container_id'] = cid
        save_private(path, state)
        obj = assert_owned(state, 'container')
        state['image_id'] = obj['Image']
        bindings = obj['NetworkSettings']['Ports']['5432/tcp']
        if len(bindings) != 1 or bindings[0]['HostIp'] != '127.0.0.1':
            raise ValueError('test DB is not loopback-only')
        state['port'] = int(bindings[0]['HostPort'])
        save_private(path, state)
        for _ in range(100):
            r = subprocess.run(DOCKER + ['exec', name, 'pg_isready', '-U', 'aliaqa', '-d', 'postgres'],
                               env=clean_env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
            if r.returncode == 0:
                state['status'] = 'ready'
                save_private(path, state)
                return path, state
            time.sleep(.2)
        raise RuntimeError('test PostgreSQL readiness timeout')
    except Exception as error:
        state['status'] = 'startup_failed'
        state['failure'] = type(error).__name__
        save_private(path, state)
        # Retain exact owned metadata so cleanup can be retried, not guessed.
        raise ProvisionError(path) from None


def url(state, database='postgres'):
    validate_state(state)
    if state.get('status') != 'ready' or not re.fullmatch('[a-z][a-z0-9_]*', database):
        raise ValueError('test DB unavailable/invalid database name')
    if assert_owned(state, 'container') is None:
        raise ValueError('test DB container absent')
    return f"postgres://{state['username']}:{state['password']}@127.0.0.1:{state['port']}/{database}"


def sql(state, statement, database='postgres'):
    if not re.fullmatch('[a-z][a-z0-9_]*', database):
        raise ValueError('invalid database name')
    if assert_owned(state, 'container') is None:
        raise ValueError('test DB container absent')
    return docker(['exec', '-i', state['container'], 'psql', '-X', '-U', state['username'],
                   '-d', database, '-v', 'ON_ERROR_STOP=1', '-A', '-t'], input=statement)


def cleanup(path):
    state = load_state(path)
    errors = []
    # Inspect BOTH identities before any destructive action, fail closed.
    container = assert_owned(state, 'container')
    volume = assert_owned(state, 'volume')
    for kind, obj in (('container', container), ('volume', volume)):
        if obj is not None:
            try:
                docker(['rm', '-f', state[kind]] if kind == 'container' else ['volume', 'rm', state[kind]])
            except Exception as error:
                errors.append({'resource': kind, 'error': type(error).__name__})
    state['cleanup'] = {'at': utc(), 'errors': errors, 'passed': not errors}
    if not errors:
        state.pop('password', None)
        state['status'] = 'cleaned'
    save_private(path, state)
    return state['cleanup']


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['start', 'cleanup', 'status'])
    p.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument('--role', choices=sorted(ROLES))
    p.add_argument('--state', type=Path)
    args = p.parse_args()
    if args.action == 'start':
        with resource_locks(('database-admin', 'shared')):
            try:
                path, state = provision(args.root, args.role)
            except ProvisionError as error:
                cleanup(error.state_path)
                raise
            print(json.dumps({'state_path': str(path), **public(state)}))
    elif args.action == 'cleanup':
        state = load_state(args.state)
        with resource_locks(('database-admin','shared'),
                            ('database',state.get('container_id','unknown-test-db'))):
            result = cleanup(args.state)
            print(json.dumps(result))
            raise SystemExit(0 if result['passed'] else 1)
    else:
        state = load_state(args.state)
        with resource_locks(('database',state.get('container_id','unknown-test-db'))):
            assert_owned(state, 'container')
            print(json.dumps(public(state)))

if __name__ == '__main__':
    main()
