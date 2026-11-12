#!/usr/bin/env python3
"""Frozen-source engineering regression. No model calls, deployment or production DB.

One report directory owns its execution copy, dependency install, targets, public
artifacts and evidence. Never compiles from the changing development checkout.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
from common import clean_env, digest, resource_locks, read_json, safe_relative, utc, write_new
from freeze import inventory, verify
from database import provision, cleanup, public, sql, url, ProvisionError, load_state
from artifacts import required_binaries, BELIEF_OBSERVER_SOURCE
from sandbox_tests import sandbox_declared, dispatch_plan, database_output_passed, stage_report_passed


REQUIRED_STAGES = ('tool-versions','m0','retrieval-corpus','driver-unit','infrastructure-unit',
    'fmt','rust-build','rust-tests','observation-tests','clippy','npm-ci','frontend-build',
    'frontend-tests','feature-tests','feature-clippy','database-tests','browser','P02-dedicated-browser',
    'management-inputs','overview-browser','documents-browser')

COLLABORATION_R4 = {
    'scripts/qa/collaboration.py': 'eb29b79311b04403132ea95d49100b0784f1ba959939a16c70f1dd673771caf9',
    'scripts/qa/common.py': '2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9',
}
# Existing immutable complete candidates may retain their original pre-COLLAB
# provenance. An arbitrary new id/source tree cannot opt into that exception.
LEGACY_CHECKPOINTS = frozenset((
    '1ff8cdc45323598f4e8e63755fec53148d628b67a311b8f867f4f4a3e8c6633a',
    '54e4eb000d95f90d36ae92f3d91762da7be797a44876f326d5c1829e254b96e4',
    '22003de0618727be042ec4d157936fb099b67d438e7e6ebc59525535010d2b3e',
    '3b49fcaf5d261e6123e1c7e4e85da5ca3fbf495894f540dafc291a237a6c3202',
    'dad2bc205776904f0bb8899729fc728fc807ae8e918b8120faffbaa6990a2824',
    '2da04226c46e1fd47443e3b066d3e289b8cc9781eb37b0c22654c5a1e060d43c',
    '529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974',
))

CAPSULE_BASE = 'evals/p03/candidate-metadata/'
CAPSULE_PATHS = tuple(CAPSULE_BASE + name for name in (
    'task-card.json', 'registry.json', 'review-root.json', 'review-p01.json',
    'validation.json', 'source-binding.json'))
CARD_FIELDS = {'format', 'task_id', 'revision', 'owner', 'reviewers', 'risk', 'baseline',
               'sources', 'dependencies', 'interfaces', 'scope', 'goal', 'acceptance',
               'delta', 'reviews', 'validation'}


def candidate_content(sources):
    """Bind every executable-input path/hash, with six closed metadata exceptions."""
    value = {'format': 'alia-candidate-content/1',
             'sources': {name: sha for name, sha in sources.items() if name not in CAPSULE_PATHS}}
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=True,
                                    separators=(',', ':')).encode('utf-8')).hexdigest()


def candidate_capsule(source, manifest, card, registry):
    if (str(card), str(registry)) != CAPSULE_PATHS[:2]:
        raise ValueError('candidate capsule requires fixed card/registry paths')
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError('duplicate candidate capsule key')
            result[key] = value
        return result
    values = []
    for name in CAPSULE_PATHS:
        path = source/name
        if name not in manifest['sources'] or digest(path) != manifest['sources'][name]:
            raise ValueError('missing or unbound candidate capsule: ' + name)
        value = json.loads(path.read_text(), object_pairs_hook=pairs,
                           parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite capsule')))
        if not isinstance(value, dict):
            raise ValueError('candidate capsule schema mismatch')
        values.append(value)
    task, current, root_review, p01_review, validation, binding = values
    if set(task) != CARD_FIELDS or set(current) != {'format', 'interfaces', 'tasks'}:
        raise ValueError('candidate capsule card/registry schema mismatch')
    for reviewer, value in [('Root', root_review), ('P01', p01_review)]:
        if (set(value) != {'format', 'reviewer', 'text'} or
                value.get('format') != 'alia-candidate-review/1' or value.get('reviewer') != reviewer or
                not isinstance(value.get('text'), str) or not value['text'].strip()):
            raise ValueError('candidate capsule review schema mismatch')
    if (set(validation) != {'format', 'text'} or validation.get('format') != 'alia-candidate-validation/1' or
            not isinstance(validation.get('text'), str) or not validation['text'].strip()):
        raise ValueError('candidate capsule validation schema mismatch')
    content = candidate_content(manifest['sources'])
    if (set(binding) != {'format', 'algorithm', 'capsule_paths', 'content_sha256'} or
            binding.get('format') != 'alia-candidate-binding/1' or binding.get('algorithm') != 'alia-candidate-content/1' or
            binding.get('capsule_paths') != list(CAPSULE_PATHS)):
        raise ValueError('candidate capsule exclusion schema mismatch')
    if (binding.get('content_sha256') != content or
            task.get('interfaces', {}).get('candidate-content') != content):
        raise ValueError('candidate-content binding mismatch')
    return content


def collaboration_inputs(snapshot, source, card=None, registry=None):
    """Metadata gate before heavy work; all inputs/evidence come from the freeze.

    This consumes the approved r4 helper. It does not create releases, reviews,
    acceptance or dispatches and never derives them from the changing checkout.
    """
    snapshot, source = Path(snapshot), Path(source)
    manifest = verify(snapshot)
    if inventory(source) != manifest['sources']:
        raise ValueError('collaboration execution source differs from checkpoint')
    if card is None and registry is None:
        if digest(snapshot/'manifest.json') not in LEGACY_CHECKPOINTS:
            raise ValueError('new candidate requires frozen --task-card and --task-registry')
        return {'passed': True, 'mode': 'exact-legacy-checkpoint',
                'review_acceptance': 'not retroactively established', 'not_run': ['COLLAB-02 historical review']}
    if card is None or registry is None:
        raise ValueError('task-card and task-registry must be supplied together')
    paths = []
    for value in (card, registry):
        name = safe_relative(str(value)).as_posix()
        path = source/name
        if manifest['sources'].get(name) != digest(path):
            raise ValueError('unbound frozen collaboration metadata: ' + name)
        paths.append(path)
    for name, sha in COLLABORATION_R4.items():
        if manifest['sources'].get(name) != sha or digest(source/name) != sha:
            raise ValueError('COLLAB-02 r4 runtime binding mismatch: ' + name)
    import collaboration
    if digest(Path(collaboration.__file__)) != COLLABORATION_R4['scripts/qa/collaboration.py']:
        raise ValueError('loaded collaboration helper is not approved r4')
    if digest(Path(__file__).with_name('common.py')) != COLLABORATION_R4['scripts/qa/common.py']:
        raise ValueError('loaded collaboration dependency is not approved r4')
    content = candidate_capsule(source, manifest, card, registry)
    task, current = read_json(paths[0]), read_json(paths[1])
    result = collaboration.validate(task, current, source, phase='ready')
    return {**result, 'mode': 'COLLAB-02/r4', 'candidate_content_sha256': content,
            'owner': task['owner'], 'reviewers': task['reviewers'],
            'card_sha256': digest(paths[0]), 'registry_sha256': digest(paths[1]),
            'accepted_tool_manifest_sha256': '31589edf23202d8824d6b5e0db3815156d38375a961e941c853f0db637de865b'}


def required_stages_for(manifest):
    from management_browser import declared_cases
    return (REQUIRED_STAGES + tuple(case + '-browser' for case in declared_cases(manifest)
                                   if case not in ('overview', 'documents')) +
            (('sandbox-tests',) if sandbox_declared(manifest) else ()))


def mandatory_stages_pass(stages, manifest=None):
    names = [s['name'] for s in stages]
    required = required_stages_for(manifest) if manifest is not None else REQUIRED_STAGES
    return (len(names) == len(set(names)) and set(required) <= set(names) and
            all(s['status'] == 'passed' for s in stages))


def observation_examples(manifest):
    examples = ['live_uncertainty', 'live_individual']
    if BELIEF_OBSERVER_SOURCE in manifest['sources']:
        examples.append('live_belief_revisions')
    return examples


def observation_output_passed(manifest, text):
    targets = re.findall(r'Running unittests examples/(\w+)\.rs \(', text)
    rows = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; '
                      r'(\d+) measured; (\d+) filtered out', text)
    expected = observation_examples(manifest)
    if len(targets) != len(set(targets)) or set(targets) != set(expected) or len(rows) != len(targets):
        return False
    # Cargo runs the selected unit binaries sequentially. stdout summaries and
    # stderr Running headers retain the same target order in the captured log.
    return all(int(row[0]) >= (15 if target == 'live_belief_revisions' else 2) and
               all(int(value) == 0 for value in row[1:]) for target, row in zip(targets, rows))


def test_counts(text):
    summaries = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', text)
    return {key: sum(int(row[i]) for row in summaries) for i, key in enumerate(
        ['passed', 'failed', 'ignored', 'measured', 'filtered_out'])} if summaries else None


def redact(text, secrets):
    for secret in sorted(filter(None, secrets), key=len, reverse=True):
        text = text.replace(secret, '[redacted]')
    text = re.sub(r'postgres(?:ql)?://[^\s\"\']+', '[redacted-test-database-url]', text)
    text = re.sub(r'(?i)(Bearer\s+)[A-Za-z0-9_.~+/=-]+', r'\1[redacted]', text)
    return text


class Run:
    def __init__(self, snapshot, output, python, node_bin, chromium=None, *, task_card=None, task_registry=None):
        self.snapshot, self.output = snapshot.resolve(), output.resolve()
        self.task_card, self.task_registry = task_card, task_registry
        self.collaboration_failure = None
        self.manifest = verify(self.snapshot)
        self.required_stages = required_stages_for(self.manifest)
        self.output.parent.mkdir(parents=True, exist_ok=True)
        self.output.mkdir()
        self.source = self.output / 'execution/source'
        self.source.parent.mkdir()
        shutil.copytree(self.snapshot / 'source', self.source)
        # Generated dependencies remain in the execution workspace for default
        # npm/Vite module resolution; frozen source/ is never mutated.
        for base, dirs, files in os.walk(self.source):
            Path(base).chmod(0o755)
            for name in files:
                (Path(base) / name).chmod(0o644)
        self.logs = self.output / 'logs'
        self.logs.mkdir()
        self.env = clean_env()
        if node_bin:
            self.env['PATH'] = str(node_bin.resolve()) + os.pathsep + self.env.get('PATH', '')
        if chromium:
            self.env['CHROMIUM_EXECUTABLE'] = str(chromium.resolve())
        self.env.update(CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='4',
                        CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', RUST_TEST_THREADS='1')
        self.python = str(python.absolute())  # preserve venv interpreter symlink path
        self.secrets = []
        self.states = []
        self.report = {'format': 'alia-p03-engineering/1', 'task': 'ALIA-P03',
            'checkpoint': self.manifest['id'], 'checkpoint_sha256': digest(self.snapshot / 'manifest.json'),
            'required_stages': list(self.required_stages),
            'observation_examples': observation_examples(self.manifest),
            'started_at': utc(), 'stages': [], 'databases': [], 'cleanup': [],
            'config_hashes': {k:v for k,v in self.manifest['sources'].items() if k.startswith(('config/', 'schemas/'))},
            'build_environment': {k:self.env[k] for k in ('CARGO_INCREMENTAL','CARGO_BUILD_JOBS','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','RUST_TEST_THREADS')},
            'model_calls': 0, 'behavior': 'unverified here; P01 owns independent behavioral conclusions',
            'collaboration': {'passed': False, 'mode': 'not_checked'},
            'limitations': ['No production deployment/init/image rebuild', 'No cloud model or full lifecycle behavior execution',
                            'Real 14-day observation and M1–M5 product gates remain separate'],
            'efficiency': {'delivery_cycles': [], 'main_development_seconds': None,
                           'test_chore_seconds': None, 'rework_seconds': None,
                           'ready_slice_count': None, 'ready_to_candidate_seconds': None,
                           'integration_rework_count': None, 'lock_wait_seconds': None,
                           'note': 'Two-cycle baseline not yet observed; no efficiency multiplier inferred'}}

    def stage(self, name, command, *, env=None, cwd=None, owner='engineering', timeout=1800):
        record = {'name': name, 'command': command, 'cwd': str(cwd or self.source),
                  'owner_module': owner, 'started_at': utc(), 'queue_seconds': 0,
                  'log': f'logs/{name}.log'}
        self.report['stages'].append(record)
        start = time.monotonic()
        print(f'START {name}', flush=True)
        try:
            resources = self.stage_resources(name, env or self.env)
            with resource_locks(*resources, environment=env or self.env) as locks:
                record['queue_seconds'] = locks.queue_seconds
                child_env = locks.child_environment(env or self.env)
                process = subprocess.Popen(command, cwd=cwd or self.source, env=child_env,
                                           pass_fds=locks.pass_fds, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                           text=True, errors='replace', start_new_session=True)
                try:
                    stdout, stderr = process.communicate(timeout=timeout)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    stdout, stderr = process.communicate()
                    record['failure'] = 'timeout; process group killed and reaped'
                record['exit_code'] = process.returncode
                content = stdout + '\n' + stderr
        except subprocess.TimeoutExpired as error:
            record['exit_code'] = None
            record['failure'] = 'timeout'
            content = (error.stdout or b'') + b'\n' + (error.stderr or b'') if isinstance(error.stdout, bytes) else (error.stdout or '') + '\n' + (error.stderr or '')
            if isinstance(content, bytes):
                content = content.decode('utf8', errors='replace')
        except OSError as error:
            record['exit_code'] = None
            record['failure'] = type(error).__name__
            content = type(error).__name__ + ': command could not start'
        content = redact(content, self.secrets)
        with (self.output / record['log']).open('x') as f:
            f.write(content)
        record.update(ended_at=utc(), run_seconds=time.monotonic() - start,
                      status='passed' if record['exit_code'] == 0 else 'failed', tests=test_counts(content))
        print(f"END {name}: {record['status']} exit={record['exit_code']}", flush=True)
        return record['status'] == 'passed'

    def stage_resources(self, name, environment):
        """Return only locks for resources this stage can mutate or consume."""
        resources = []
        if name in {'rust-build','rust-tests','observation-tests','clippy','feature-tests',
                    'feature-clippy','database-tests','sandbox-tests'}:
            target = environment.get('CARGO_TARGET_DIR') or str(self.output / (
                'build/feature' if name.startswith('feature-') else 'build/default'))
            resources.append(('target', Path(target).resolve()))
        if name in {'database-tests','browser','P02-dedicated-browser'} or name.endswith('-browser'):
            if self.states:
                resources.append(('database',self.states[0][1].get('container_id','unknown-test-db')))
        if name in {'browser','P02-dedicated-browser'} or name.endswith('-browser'):
            browser_name = {'browser':'browser', 'P02-dedicated-browser':'p02-browser'}.get(
                name, name.removesuffix('-browser'))
            resources.append(('browser', str(self.output / browser_name)))
        if name == 'sandbox-tests':
            resources.append(('sandbox', str(self.output / name)))
        if name in {'npm-ci','frontend-build','frontend-tests'}:
            resources.append(('workspace', str(self.source / 'web')))
        return resources

    def skipped(self, name, reason):
        self.report['stages'].append({'name': name, 'status': 'unverified', 'reason': reason,
                                      'exit_code': None, 'tests': None})

    def run_observation_tests(self):
        command = ['cargo', 'test', '--locked']
        for name in observation_examples(self.manifest):
            command.extend(['--example', name])
        if self.stage('observation-tests', command, owner='P01/observation'):
            record = self.report['stages'][-1]
            if not observation_output_passed(self.manifest, (self.output / record['log']).read_text()):
                record.update(status='failed', failure='required observer unit executions missing or ignored')
                print('FAIL observation-tests: required unit execution evidence missing', flush=True)

    def run_management_browsers(self, built, web_ok):
        from management_browser import declared_cases
        for case in declared_cases(self.manifest):
            name = case + '-browser'
            if built and web_ok:
                self.stage(name, [self.python, 'scripts/qa/management_browser.py',
                    '--case', case, '--checkpoint', str(self.snapshot),
                    '--binary-dir', str(self.output/'artifacts/bin'),
                    '--web-dir', str(self.output/'artifacts/web'), '--source-root', str(self.source),
                    '--test-db-state', str(self.states[0][0]), '--output', str(self.output/name)],
                    owner='accepted-management/' + case)
            else:
                self.skipped(name, 'required build/static assets unavailable; mandatory stage unverified')

    def run_sandbox_tests(self, target):
        if sandbox_declared(self.manifest):
            if self.stage('sandbox-tests',[self.python,'scripts/qa/sandbox_tests.py',
                '--checkpoint',str(self.snapshot),'--source-root',str(self.source),
                '--target-dir',str(target),'--output',str(self.output/'sandbox-tests')],
                owner='Docker/isolated-sandbox',timeout=260):
                record = self.report['stages'][-1]
                record['test_domain'] = 'actual-Docker-not-SQLx'
                if not stage_report_passed(self.output/'sandbox-tests',self.manifest,
                                           self.report['checkpoint_sha256']):
                    record.update(status='failed',failure='Docker child evidence missing or invalid')
                else:
                    record['tests'] = test_counts((self.output/'sandbox-tests/cargo.log').read_text())

    def publish_artifacts(self, target, web_ok):
        out = self.output / 'artifacts'
        (out / 'bin/examples').mkdir(parents=True)
        names = sorted(name.removeprefix('examples/') for name in required_binaries(self.manifest)
                       if name.startswith('examples/'))
        for name, source in [('alia', target / 'debug/alia')] + [
                ('examples/' + n, target / 'debug/examples' / n) for n in names]:
            dest = out / 'bin' / name
            shutil.copyfile(source, dest)
            dest.chmod(0o555)
        if web_ok:
            shutil.copytree(self.output / 'web-dist', out / 'web')
        # Compatibility file for P01: binaries directly under bin/examples.
        write_new(out / 'p01-manifest.json', {'id': self.manifest['id'], 'sources': self.manifest['sources'],
                   'checkpoint_sha256': self.report['checkpoint_sha256'],
                   'binaries': {n: digest(out / 'bin/examples' / n) for n in names}})
        files = {p.relative_to(out).as_posix(): digest(p) for p in sorted(out.rglob('*')) if p.is_file()}
        artifact = {'format': 'alia-p03-artifacts/1', 'checkpoint_id': self.manifest['id'],
                    'checkpoint_sha256': self.report['checkpoint_sha256'], 'files': files,
                    'source_root': str(self.source), 'created_at': utc(),
                    'build_environment': self.report['build_environment'],
                    'build_commands': [s for s in self.report['stages'] if s['name'] in ('rust-build','frontend-build')],
                    'tool_versions_log': 'logs/tool-versions.log'}
        write_new(out / 'artifacts.json', artifact)
        (out / 'artifacts.json').chmod(0o444)
        self.report['artifacts_sha256'] = digest(out / 'artifacts.json')
        self.report['artifacts'] = 'artifacts/artifacts.json'

    def setup_databases(self):
        # The two databases are created under a resource lock, but the lock is
        # released before builds/other domains so independent work can proceed.
        with resource_locks(('database-admin','shared')):
            for role in ('engineering','behavior'):
                try:
                    path, state = provision(self.source, role)
                except ProvisionError as error:
                    self.states.append((error.state_path, load_state(error.state_path)))
                    raise
                self.states.append((path, state))
                self.secrets.extend([state['password'], url(state)])
                self.report['databases'].append(public(state))
            eng, beh = self.states[0][1], self.states[1][1]
            sql(eng, 'CREATE TABLE p03_owner_marker(value text); INSERT INTO p03_owner_marker VALUES (\'engineering\');')
            assert sql(beh, "SELECT count(*) FROM information_schema.tables WHERE table_name='p03_owner_marker';") == '0'
            sql(beh, 'CREATE TABLE p03_owner_marker(value text); INSERT INTO p03_owner_marker VALUES (\'behavior\');')
            assert sql(eng, 'SELECT value FROM p03_owner_marker;') == 'engineering'
            assert sql(beh, 'SELECT value FROM p03_owner_marker;') == 'behavior'
            assert eng['port'] != beh['port'] and eng['container_id'] != beh['container_id']
            self.report['database_isolation'] = {'passed': True, 'at': utc(), 'model_calls': 0}

    def cleanup_databases(self):
        with resource_locks(('database-admin','shared'),
                             *[('database', state.get('container_id','unknown-test-db'))
                               for _, state in self.states]):
            for path, state in self.states:
                try:
                    self.report['cleanup'].append({'role':state['role'],'private_state':str(path),**cleanup(path)})
                except Exception as error:
                    self.report['cleanup'].append({'role':state['role'],'private_state':str(path),
                                                   'passed':False,'error':type(error).__name__})

    def run(self):
        try:
            self.report['collaboration'] = collaboration_inputs(
                self.snapshot, self.source, self.task_card, self.task_registry)
        except Exception as error:
            self.collaboration_failure = type(error).__name__ + ': ' + str(error)
            self.report['collaboration'] = {'passed': False, 'mode': 'rejected',
                                            'failure': self.collaboration_failure}
            # Existing finalization records unrun stages and exact source status;
            # no candidate/resource lock, process, build or database is started.
            return self.run_locked()
        with resource_locks(('candidate','full-matrix')) as locks:
            self.report['candidate_queue_seconds'] = locks.queue_seconds
            return self.run_locked()

    def run_locked(self):
        try:
            if self.collaboration_failure:
                raise ValueError('collaboration preflight failed: ' + self.collaboration_failure)
            test_plan = dispatch_plan(self.source,self.manifest)
            self.report['test_domains'] = {'SQLx_expected':len(test_plan['sqlx_cases']),
                'Docker_expected':len(test_plan['docker_cases'])}
            self.stage('tool-versions', [self.python, '-c',
                "import subprocess; [subprocess.run(c,check=True) for c in [['rustc','--version'],['cargo','--version'],['node','--version'],['npm','--version']]]"])
            if not self.stage('management-inputs', [self.python,'scripts/qa/management_browser.py',
                    '--verify-inputs-only','--checkpoint',str(self.snapshot),
                    '--source-root',str(self.source)], owner='P03/accepted-management-inputs'):
                raise RuntimeError('required accepted management inputs failed; see stage log')
            self.stage('m0', [self.python, 'scripts/check_phase0.py'], owner='design/schema')
            self.stage('retrieval-corpus', [self.python, 'scripts/check_retrieval_corpus.py'], owner='retrieval/evals')
            self.stage('driver-unit', [self.python, '-m', 'unittest', 'discover', '-s', 'evals/p01', '-p', 'test_*.py'], owner='P01/driver')
            self.stage('infrastructure-unit', [self.python, '-m', 'unittest', 'discover', '-s', 'evals/p03', '-p', 'test_*.py'], owner='P03/infrastructure')
            self.stage('fmt', ['cargo','fmt','--all','--','--check'], owner='Rust/style')
            target = self.output / 'build/default'
            self.env['CARGO_TARGET_DIR'] = str(target)
            built = self.stage('rust-build', ['cargo','build','--locked','--bins','--examples'], owner='Rust/build')
            ordinary = self.stage('rust-tests', ['cargo','test','--locked','--workspace'], owner='Rust/tests')
            self.run_observation_tests()
            self.stage('clippy', ['cargo','clippy','--locked','--all-targets','--','-D','warnings'], owner='Rust/lints')
            npm = self.stage('npm-ci', ['npm','ci','--ignore-scripts','--no-audit','--no-fund'], cwd=self.source/'web', owner='frontend/dependencies')
            if npm:
                web_ok = self.stage('frontend-build', ['npm','run','build','--','--outDir', str(self.output/'web-dist')], cwd=self.source/'web', owner='frontend')
                self.stage('frontend-tests', ['npm','test'], cwd=self.source/'web', owner='frontend')
            else:
                web_ok = False
                self.skipped('frontend-build', 'npm-ci failed')
                self.skipped('frontend-tests', 'npm-ci failed')
            if built:
                self.publish_artifacts(target, web_ok)
            else:
                self.skipped('artifact-publish', 'required Rust build failed')
            feature_env = self.env.copy()
            feature_env['CARGO_TARGET_DIR'] = str(self.output/'build/feature')
            self.stage('feature-tests', ['cargo','test','--locked','--workspace','--features','local-embeddings'], env=feature_env, owner='Rust/local-embeddings')
            self.stage('feature-clippy', ['cargo','clippy','--locked','--all-targets','--features','local-embeddings','--','-D','warnings'], env=feature_env, owner='Rust/local-embeddings')
            # Allocate isolated test instances. Each DB/browser stage locks the
            # shared service only while it operates, never through Rust builds.
            self.setup_databases()
            self.stage('database-tests', [self.python,'scripts/test_database.py','--checkpoint',str(self.snapshot),
                '--source-root',str(self.source),'--test-db-state',str(self.states[0][0]),'--target-dir',str(target)], owner='Rust/SQLx/HTTP')
            database_record = self.report['stages'][-1]
            database_record['test_domain'] = 'SQLx-not-Docker'
            database_record['expected_cases'] = test_plan['sqlx_cases']
            if database_record['status']=='passed' and not database_output_passed(
                    test_plan,(self.output/database_record['log']).read_text()):
                database_record.update(status='failed',failure='SQLx execution list mismatched; Docker not counted as PG')
            self.run_sandbox_tests(target)
            if built and web_ok:
                self.stage('browser', [self.python,'scripts/test_web.py','--checkpoint',str(self.snapshot),
                    '--binary-dir',str(self.output/'artifacts/bin'),'--web-dir',str(self.output/'artifacts/web'),
                    '--source-root',str(self.source),'--test-db-state',str(self.states[0][0]),
                    '--output',str(self.output/'browser')], owner='browser/full-existing-smoke')
            else:
                self.skipped('browser', 'required build/static assets unavailable')
            if built and web_ok:
                self.stage('P02-dedicated-browser', [self.python,'scripts/qa/test_p02_browser.py',
                    '--checkpoint',str(self.snapshot),'--binary-dir',str(self.output/'artifacts/bin'),
                    '--web-dir',str(self.output/'artifacts/web'),'--source-root',str(self.source),
                    '--test-db-state',str(self.states[0][0]),'--output',str(self.output/'p02-browser')], owner='P02/console-browser')
            else:
                self.skipped('P02-dedicated-browser', 'required builds unavailable')
            self.run_management_browsers(built, web_ok)
            # Clean behavior first and prove it cannot remove engineering data.
            with resource_locks(('database-admin','shared'),
                                ('database', self.states[-1][1].get('container_id','unknown-test-db'))):
                path, state = self.states[-1]
                result = cleanup(path)
                if result['passed']:
                    self.states.pop()
                self.report['cleanup'].append({'role': state['role'], 'private_state':str(path), **result})
                assert sql(self.states[0][1], 'SELECT value FROM p03_owner_marker;') == 'engineering'
                self.report['cleanup_isolation'] = {'passed': True, 'at': utc()}
        except Exception as error:
            self.report['infrastructure_failure'] = redact(type(error).__name__ + ': ' + str(error), self.secrets)
        finally:
            if self.states:
                self.cleanup_databases()
            for field, check in (
                    ('snapshot_verified_after', lambda: verify(self.snapshot)['id'] == self.manifest['id']),
                    ('execution_source_verified_after', lambda: inventory(self.source) == self.manifest['sources'])):
                try:
                    self.report[field] = check()
                except Exception as error:
                    self.report[field] = False
                    self.report[field + '_error'] = type(error).__name__
            existing = {s['name'] for s in self.report['stages']}
            for name in self.required_stages:
                if name not in existing:
                    self.skipped(name, 'not reached after infrastructure failure')
            self.report['next_dependencies'] = [
                {'stage':s['name'],'owner_module':s.get('owner_module','P03/infrastructure'),
                 'reproduce':s.get('command'),'log':s.get('log'),'reason':s.get('reason')}
                for s in self.report['stages'] if s['status']!='passed']
            if self.report['collaboration'].get('passed') is not True:
                self.report['next_dependencies'].append({'stage': 'collaboration-inputs',
                    'owner_module': 'Root/COLLAB-02', 'reason': 'frozen task/review gate not passed'})
            self.report['ended_at'] = utc()
            self.report['run_seconds'] = sum(s.get('run_seconds',0) for s in self.report['stages'])
            self.report['lock_wait_seconds'] = (self.report.get('candidate_queue_seconds', 0) +
                sum(s.get('queue_seconds', 0) for s in self.report['stages']))
            self.report['efficiency']['lock_wait_seconds'] = self.report['lock_wait_seconds']
            self.report['passed'] = (not self.report.get('infrastructure_failure') and
                self.report['collaboration'].get('passed') is True and
                mandatory_stages_pass(self.report['stages'], self.manifest) and
                all(s['passed'] for s in self.report['cleanup']) and self.report['execution_source_verified_after'] and self.report['snapshot_verified_after'])
            self.report['full_engineering_pass'] = self.report['passed']
            executed = [s for s in self.report['stages'] if s['status']!='unverified']
            self.report['subset_commands_pass'] = bool(executed) and all(s['status']=='passed' for s in executed)
            write_new(self.output/'report.json', self.report)
            self.render_summary()
        print(json.dumps({'passed':self.report['passed'],'report':str(self.output/'report.json')}),flush=True)
        return 0 if self.report['passed'] else 1

    def render_summary(self):
        lines = ['# ALIA-P03 engineering report', '', f"Checkpoint: `{self.manifest['id']}`",
                 f"Manifest SHA256: `{self.report['checkpoint_sha256']}`", '',
                 '| Stage | Result | Exit | Passed / failed / ignored | Seconds | Log |',
                 '| --- | --- | --- | --- | --- | --- |']
        for s in self.report['stages']:
            counts = s.get('tests')
            count = '/'.join(str(counts[k]) for k in ('passed','failed','ignored')) if counts else '—'
            log = '['+s['name']+']('+s['log']+')' if 'log' in s else s.get('reason','')
            lines.append(f"| {s['name']} | {s['status']} | {s['exit_code']} | {count} | {s.get('run_seconds',0):.2f} | {log} |")
        lines.extend(['', 'Behavior: P01 report required; no model calls by P03.',
                      'Unknown usage is not zero. Ordinary ignored DB tests are not passes.',
                      'Failures remain attached to this candidate. Production code unchanged.',
                      'Two delivery-cycle efficiency measurements: unavailable, no multiplier claimed.', '',
                      'Exact commands, UTC timestamps, ownership, cleanup and limitations: [report.json](report.json).'])
        if self.report.get('infrastructure_failure'):
            lines.extend(['', 'Infrastructure failure: '+self.report['infrastructure_failure']])
        with (self.output/'report.md').open('x') as f:
            f.write('\n'.join(lines)+'\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--snapshot', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--python', type=Path, default=Path(sys.executable))
    p.add_argument('--node-bin', type=Path)
    p.add_argument('--chromium', type=Path)
    p.add_argument('--task-card', type=Path, help='relative path inside frozen source; required for new candidates')
    p.add_argument('--task-registry', type=Path, help='frozen Root-owned registry, paired with --task-card')
    args = p.parse_args()
    return Run(args.snapshot,args.output,args.python,args.node_bin,args.chromium,
               task_card=args.task_card,task_registry=args.task_registry).run()

if __name__ == '__main__':
    raise SystemExit(main())
