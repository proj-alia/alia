#!/usr/bin/env python3
"""One-shot synthetic Store producer for an explicitly owned P02 database."""
import argparse
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import uuid

from common import clean_env, digest, read_json, write_new
from database import load_state, url, public

BASE_MANIFEST = 'a965d6b835e029ea987dd2babe60b534b01394dce3f547a53253d811aa482b1c'
FIELDS = {'database_state', 'database', 'individual_id', 'service_url', 'service_token',
          'source_root', 'source_manifest', 'source_manifest_sha256',
          'base_manifest', 'producer_binary', 'producer_binary_sha256',
          'service_binary', 'service_binary_sha256', 'output'}


def require(ok):
    if not ok:
        raise ValueError('invalid or unbound development fixture request')


def private_json(path):
    path = Path(path)
    require(not path.is_symlink() and path.is_file() and
            path.stat().st_uid == os.getuid() and stat.S_IMODE(path.stat().st_mode) == 0o600)
    return read_json(path)


def validate_request(request):
    require(isinstance(request, dict) and set(request) == FIELDS)
    require(all(isinstance(v, str) and bool(v) for v in request.values()))
    require(re.fullmatch(r'alia_p02_[a-z0-9_]+', request['database']))
    require(str(uuid.UUID(request['individual_id'])) == request['individual_id'] and
            uuid.UUID(request['individual_id']).int != 0)
    from urllib.parse import urlsplit
    endpoint = urlsplit(request['service_url'])
    require(endpoint.scheme == 'http' and endpoint.hostname == '127.0.0.1' and
            endpoint.port is not None and not endpoint.username and not endpoint.password and
            not endpoint.query and not endpoint.fragment and endpoint.path.rstrip('/') == '/api/v1')
    for key in ('source_manifest_sha256', 'producer_binary_sha256', 'service_binary_sha256'):
        require(re.fullmatch('[a-f0-9]{64}', request[key]))
    require(digest(request['base_manifest']) == BASE_MANIFEST)
    require(digest(request['source_manifest']) == request['source_manifest_sha256'])
    source = Path(request['source_root']).resolve()
    files = read_json(request['source_manifest'])['sources']
    base = read_json(request['base_manifest'])['sources']
    # P02 may overlay accepted UI sources. The fixed backend and QA dependency
    # bytes remain exact, including refusal of unlisted/new backend modules.
    def backend(name):
        return name.startswith(('src/', 'migrations/', 'schemas/', 'config/')) or name in {
            'Cargo.toml', 'Cargo.lock', 'build.rs', 'tests/support/development_context_fixture.rs',
            'scripts/qa/common.py', 'scripts/qa/database.py'}
    require({n: h for n, h in files.items() if backend(n)} ==
            {n: h for n, h in base.items() if backend(n)})
    require({str(p.relative_to(source)) for p in (source/'src').rglob('*') if p.is_file()} ==
            {n for n in files if n.startswith('src/')})
    require('examples/development_ui_fixture.rs' in files and
            'scripts/qa/development_ui_fixture.py' in files)
    for name, expected in files.items():
        rel = Path(name)
        require(not rel.is_absolute() and '..' not in rel.parts and str(rel) == name)
        p = source / rel
        require(p.is_file() and p.resolve().is_relative_to(source) and
                not any(v.is_symlink() for v in [p, *p.parents] if v.is_relative_to(source)))
        require(digest(p) == expected)
    for key in ('producer_binary', 'service_binary'):
        require(Path(request[key]).is_file() and not Path(request[key]).is_symlink() and
                digest(request[key]) == request[key + '_sha256'])
    require(digest(__file__) == files['scripts/qa/development_ui_fixture.py'])
    require(not Path(request['output']).exists() and Path(request['output']).parent.is_dir())
    return request


def produce(path):
    request = validate_request(private_json(path))
    # Credentials may only enter the isolated child through our protected input.
    # Reject supplied cloud/model credentials instead of accidentally inheriting.
    require(not any(v and re.search(r'(API_KEY|ACCESS_TOKEN|MODEL_TOKEN|PROVIDER_KEY|DEEPSEEK|OPENCODE)', k, re.I)
                    for k, v in os.environ.items()))
    private_json(request['database_state'])
    state = load_state(Path(request['database_state']))
    database_url = url(state, request['database'])  # actual loopback/label/ID checks
    claim = Path(request['database_state']).parent / (request['database'] + '.development-fixture-attempt.json')
    write_new(claim, {'individual_id': request['individual_id'], 'state': 'claimed_outcome_unknown'}, private=True)
    environment = clean_env()
    environment['ALIA_DEVELOPMENT_UI_PRIVATE'] = json.dumps({
        'database_url': database_url, 'database': request['database'],
        'individual_id': request['individual_id'], 'service_url': request['service_url'],
        'service_token': request['service_token']})
    child = subprocess.run([request['producer_binary']], env=environment, capture_output=True,
                           text=True, timeout=110, check=False)
    if child.returncode != 0:
        location = re.fullmatch(r'synthetic fixture failed at line ([0-9]+)\n?', child.stderr)
        if location:
            print('fixture panic location line ' + location[1], file=sys.stderr)
        raise RuntimeError('fixture failed; partial state is unknown; no automatic reseed')
    report = json.loads(child.stdout)
    require(report.get('synthetic') is True and report.get('individual_id') == request['individual_id'] and
            report.get('database') == request['database'] and report.get('reservations_before') == 0 and
            report.get('reservations_after') == 0 and report.get('production_verifier') is False)
    # Public report is explicitly allowlisted; raw subprocess output is never saved.
    allowed = {'format', 'synthetic', 'production_verifier', 'formal_acceptance', 'individual_id',
        'database', 'items', 'roles_order', 'roles', 'goal_id', 'task_id', 'proposed_id',
        'reservations_before', 'reservations_after', 'cleanup_owner'}
    require(set(report) == allowed)
    report['bindings'] = {key: request[key] for key in (
        'source_manifest_sha256', 'producer_binary_sha256', 'service_binary_sha256')}
    report['database_resource'] = public(state)
    report['producer_exit_code'] = child.returncode
    write_new(Path(request['output']), report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--request', required=True, type=Path)
    args = parser.parse_args()
    try:
        produce(args.request)
    except Exception:
        print('fixture refused or failed; inspect parent-owned attempt, do not reseed', file=sys.stderr)
        return 1
    print('synthetic development fixture committed; public report written')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
