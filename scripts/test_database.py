#!/usr/bin/env python3
"""Run ALL ignored SQLx tests serially on an explicitly disposable QA database.

Legacy invocation keeps the local .env default. Frozen mode never reads .env or
starts Compose. SQLx creates its own per-case databases on the QA-only instance.
"""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
from common import clean_env, resource_locks
from database import load_state, url, assert_owned
from freeze import verify, inventory
from sandbox_tests import dispatch_plan, database_command


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint', type=Path)
    parser.add_argument('--source-root', type=Path)
    parser.add_argument('--test-db-state', type=Path)
    parser.add_argument('--target-dir', type=Path)
    args = parser.parse_args()
    environment = clean_env()
    # Preserve the parent runner's recorded build profile so SQLx reuses its
    # already-built test executables rather than recompiling with Cargo defaults.
    for key in ('CARGO_BUILD_JOBS', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
        if key in os.environ:
            environment[key] = os.environ[key]
    if args.checkpoint:
        manifest = verify(args.checkpoint)
        root = args.source_root.resolve() if args.source_root else args.checkpoint.resolve() / 'source'
        if inventory(root) != manifest['sources']:
            parser.error('execution source differs from checkpoint')
        if not args.test_db_state or not args.target_dir:
            parser.error('frozen mode requires --test-db-state and --target-dir')
        state = load_state(args.test_db_state)
        if state['role'] != 'engineering':
            parser.error('SQLx requires the engineering test resource')
        assert_owned(state, 'container')
        environment['DATABASE_URL'] = url(state)
        environment['CARGO_TARGET_DIR'] = str(args.target_dir.resolve())
    else:
        if args.source_root or args.test_db_state or args.target_dir:
            parser.error('explicit resources require --checkpoint')
        root = ROOT
        values = dict(line.split('=', 1) for line in (root / '.env').read_text().splitlines()
                      if line.strip() and not line.startswith('#'))
        environment['DATABASE_URL'] = (f"postgres://alia:{values['ALIA_DB_PASSWORD']}@127.0.0.1:"
                                       f"{values.get('ALIA_DB_PORT', '55433')}/postgres")
    environment['RUST_TEST_THREADS'] = '1'
    environment['CARGO_INCREMENTAL'] = '0'
    resources = [('database', state.get('container_id', 'legacy-compose')),
                 ('target', environment.get('CARGO_TARGET_DIR', root / 'target'))]
    with resource_locks(*resources, environment=environment) as locks:
        environment = locks.child_environment(environment)
        plan = dispatch_plan(root)
        result = subprocess.run(database_command(plan), cwd=root, env=environment,
                                pass_fds=locks.pass_fds)
    return result.returncode


if __name__ == '__main__':
    raise SystemExit(main())
