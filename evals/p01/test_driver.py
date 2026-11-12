"""No model/network/DB/compilation. All subprocesses explicitly injected."""
import copy
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('lifecycle_driver', ROOT / 'scripts/test_live_lifecycle.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
import check_results as checker


def projection(phase):
    return {'phase': phase, 'individual_id': 'same', 'budget_frozen': False,
            'engine_pass': True, 'goals': [{'id': 'goal', 'criterion': 'research and limitations', 'completion': None}],
            'observations': [{'status': 'succeeded', 'capability_id': 'documents.read'}],
            'memories': [{'scope': 'general'}], 'events': [],
            'replies': [{'conversation_id': phase}]}


def ledger(**changes):
    row = {'id': 'reservation', 'run_id': 'run', 'status': 'settled',
           'actual_input': 10, 'actual_output': 2, 'charged_tokens': 12, 'usage_unknown': False}
    row.update(changes)
    return {'reservation_count': 1, 'usage': [row], 'budget_frozen': False}


class Trial:
    def __init__(self, phase_outputs=None, recovery=None, cleanup=None, create=None):
        self.outputs = phase_outputs or {}
        self.recovery = recovery if recovery is not None else subprocess.CompletedProcess([], 0, json.dumps(ledger()), '')
        self.cleanup = cleanup if cleanup is not None else subprocess.CompletedProcess([], 0, '', '')
        self.create = create if create is not None else subprocess.CompletedProcess([], 0, '', '')
        self.calls = []

    def __call__(self, command, **kwargs):
        self.calls.append((command, kwargs))
        if 'CREATE DATABASE' in command[-1]:
            result = self.create
        elif 'DROP DATABASE' in command[-1]:
            result = self.cleanup
        elif 'SELECT jsonb_build_object' in command[-1]:
            result = self.recovery
        else:
            phase = kwargs['env']['ALIA_LIVE_PHASE']
            result = self.outputs.get(phase, subprocess.CompletedProcess([], 0, json.dumps(projection(phase)), 'PRIVATE STDERR'))
        if isinstance(result, BaseException):
            raise result
        return result


class DriverTests(unittest.TestCase):
    def execute(self, trial=None, phases=driver.PHASES):
        trial = trial or Trial()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            result = driver.execute_trial(phases, Path('/frozen'), output,
                                          {'SECRET': 'NEVER_PERSIST'}, ['psql', '-d', 'postgres', '-c'],
                                          'alia_live_test_' + 'a' * 32, {'checkpoint_id': 'fixture'}, trial)
            saved = json.loads(output.read_text())
            self.assertEqual(result, saved)
            self.assertNotIn('NEVER_PERSIST', output.read_text())
            self.assertNotIn('PRIVATE STDERR', output.read_text())
            self.assertEqual(trial.calls[-1][0][-1], 'DROP DATABASE IF EXISTS alia_live_test_' + 'a' * 32 + ' WITH (FORCE)')
            return result, trial

    def test_four_phases_and_cleanup(self):
        result, trial = self.execute()
        self.assertTrue(result['engine_pass'])
        self.assertIsNone(result['behavior_pass'])
        self.assertEqual(result['accounting']['charged_tokens'], 12)
        self.assertEqual([c[1]['env']['ALIA_LIVE_PHASE'] for c in trial.calls if 'env' in c[1]], list(driver.PHASES))
        self.assertTrue(all(c[1]['cwd'] == ROOT for c in trial.calls))

    def test_all_existing_isolated_modes(self):
        for phase in ('uncertainty', 'grounding', 'person-retrieval', 'entities', 'discover'):
            with self.subTest(phase=phase):
                result, _ = self.execute(phases=(phase,))
                self.assertTrue(result['engine_pass'])

    def test_timeout_preserves_complete_partial_projection(self):
        timeout = subprocess.TimeoutExpired('private', 180, output=(json.dumps(projection('first-study')) + '\n{"unfinished":').encode())
        result, _ = self.execute(Trial({'first-study': timeout}))
        self.assertEqual(result['failure']['code'], 'phase_timeout')
        self.assertEqual(len(result['reports']), 1)
        self.assertEqual(result['stages'][0]['parse_errors'][0]['code'], 'invalid_json_line')
        self.assertEqual(result['not_run_phases'], ['restore', 'observe', 'unknown'])
        self.assertEqual(result['accounting']['charged_tokens'], 12)

    def test_no_json_invalid_json_multiline_and_invalid_shape(self):
        values = [('', 'no_json'), ('PRIVATE RAW', 'invalid_json_line'),
                  ('{', 'invalid_json_line'), ('[]', 'invalid_json_line'),
                  ('{"phase":"first-study","phase":"other"}', 'invalid_json_line'),
                  ('{"x":NaN}', 'invalid_json_line')]
        for output, code in values:
            with self.subTest(output=output):
                result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 0, output, '')}))
                self.assertEqual(result['failure']['code'], 'phase_output_invalid')
                self.assertEqual(result['stages'][0]['parse_errors'][0]['code'], code)
                self.assertNotIn('PRIVATE RAW', json.dumps(result))

    def test_multiple_complete_reports_retained_but_fail(self):
        output = '\n'.join([json.dumps(projection('first-study'))] * 2)
        result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 0, output, '')}))
        self.assertEqual(len(result['reports']), 2)
        self.assertEqual(result['stages'][0]['parse_errors'], [{'code': 'multiple_reports', 'count': 2}])
        self.assertFalse(result['engine_pass'])

    def test_bad_line_does_not_erase_previous_or_following_json(self):
        objects, errors = driver.parse_projection('{"a":1}\nINCOMPLETE\n{"b":2}\n')
        self.assertEqual(objects, [{'a': 1}, {'b': 2}])
        self.assertEqual(errors[0], {'code': 'invalid_json_line', 'line': 2})

    def test_returncode_survives_parser_failure(self):
        result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 7, 'invalid', '')}))
        self.assertEqual(result['latest_exit'], 7)
        self.assertEqual(result['stages'][0]['exit'], 7)

    def test_runtime_nonzero_retains_projection_and_stop_position(self):
        result, _ = self.execute(Trial({'restore': subprocess.CompletedProcess([], 1, json.dumps(projection('restore')), '')}))
        self.assertEqual(result['failure']['category'], 'runtime_failure')
        self.assertEqual(result['failure']['phase'], 'restore')
        self.assertEqual(len(result['reports']), 2)
        self.assertEqual(result['not_run_phases'], ['observe', 'unknown'])

    def test_projection_missing_fields(self):
        result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 0, '{}', '')}))
        self.assertEqual(result['failure']['code'], 'projection_shape_invalid')

    def test_research_narrowing_fails(self):
        data = projection('first-study')
        data['goals'][0]['completion'] = {'kind': 'source_excerpt'}
        result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 0, json.dumps(data), '')}))
        self.assertIn('research_narrowed', result['stages'][0]['engineering_errors'])

    def test_identity_criterion_session_and_freeze(self):
        for phase, mutate, expected in [
            ('restore', lambda d: d.update(individual_id='different'), 'identity_changed'),
            ('observe', lambda d: d['goals'][0].update(criterion='smaller'), 'original_goal_changed'),
            ('unknown', lambda d: d.update(budget_frozen=True), 'budget_frozen_or_unknown')]:
            data = projection(phase)
            mutate(data)
            result, _ = self.execute(Trial({phase: subprocess.CompletedProcess([], 0, json.dumps(data), '')}))
            self.assertIn(expected, result['stages'][-1]['engineering_errors'])
        data = projection('unknown')
        data['replies'][0]['conversation_id'] = 'restore'
        result, _ = self.execute(Trial({'unknown': subprocess.CompletedProcess([], 0, json.dumps(data), '')}))
        self.assertEqual(result['failure']['code'], 'foreground_session_reused')

    def test_process_launch_error(self):
        result, _ = self.execute(Trial({'first-study': FileNotFoundError('secret path')}))
        self.assertEqual(result['failure']['code'], 'phase_process_failed')
        self.assertNotIn('secret path', json.dumps(result))

    def test_recovery_timeout_exit_json_and_shape(self):
        for recovery in [subprocess.TimeoutExpired('psql', 15),
                         subprocess.CompletedProcess([], 1, 'PRIVATE RAW', ''),
                         subprocess.CompletedProcess([], 0, 'invalid', ''),
                         subprocess.CompletedProcess([], 0, '{"usage":[]}', '')]:
            result, _ = self.execute(Trial(recovery=recovery))
            self.assertFalse(result['engine_pass'])
            self.assertFalse(result['accounting']['recovered'])
            self.assertIsNone(result['accounting']['charged_tokens'])
            self.assertEqual(result['cleanup']['status'], 'passed')

    def test_unknown_dispatched_and_truncated_fees_are_not_zero(self):
        for data in [ledger(usage_unknown=True, charged_tokens=400),
                     ledger(status='dispatched', charged_tokens=0),
                     dict(ledger(), reservation_count=257)]:
            result, _ = self.execute(Trial(recovery=subprocess.CompletedProcess([], 0, json.dumps(data), '')))
            self.assertIsNone(result['accounting']['charged_tokens'])
            self.assertFalse(result['accounting']['usage_complete'])
            self.assertFalse(result['engine_pass'])

    def test_cleanup_failure_does_not_replace_primary(self):
        result, _ = self.execute(Trial({'first-study': subprocess.CompletedProcess([], 9, json.dumps(projection('first-study')), '')}, cleanup=subprocess.CompletedProcess([], 1, '', '')))
        self.assertEqual(result['failure']['code'], 'phase_exit_nonzero')
        self.assertEqual(result['latest_exit'], 9)
        self.assertEqual(result['cleanup']['status'], 'failed')
        self.assertEqual(result['accounting']['charged_tokens'], 12)

    def test_cleanup_timeout_and_oserror_fail_engine(self):
        for error in [subprocess.TimeoutExpired('psql', 30), OSError('private')]:
            result, _ = self.execute(Trial(cleanup=error))
            self.assertEqual(result['failure']['code'], 'cleanup_failed')
            self.assertFalse(result['engine_pass'])

    def test_create_failure_and_ambiguous_timeout_also_cleanup(self):
        for create in [subprocess.CompletedProcess([], 1, '', ''), subprocess.TimeoutExpired('psql', 30)]:
            result, _ = self.execute(Trial(create=create))
            self.assertEqual(result['stages'], [])
            self.assertFalse(result['engine_pass'])
            self.assertEqual(result['not_run_phases'], list(driver.PHASES))

    def test_interruption_cannot_pass_single_phase(self):
        result, _ = self.execute(Trial({'uncertainty': KeyboardInterrupt()}), phases=('uncertainty',))
        self.assertFalse(result['engine_pass'])
        self.assertEqual(result['failure']['code'], 'driver_interrupted')
        self.assertEqual(result['stages'][0]['status'], 'failed')
        self.assertEqual(result['cleanup']['status'], 'passed')

    def test_invalid_database_is_rejected_before_process(self):
        trial = Trial()
        with self.assertRaisesRegex(ValueError, 'refusing non-disposable'):
            driver.execute_trial(driver.PHASES, Path('/frozen'), Path('/unused'), {},
                                 ['psql', '-d', 'postgres', '-c'], 'production', {}, trial)
        self.assertEqual(trial.calls, [])

    def test_checkpoint_binary_binding(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            for name in ('live_individual', 'live_autonomy'):
                (directory / name).write_bytes(b'fixture-binary')
            raw = driver.collect_provenance(directory, driver.PHASES)
            self.assertFalse(raw['checkpoint_verified'])
            checkpoint = directory / 'checkpoint.json'
            checkpoint.write_text(json.dumps({'id': 'fixed-v1', 'sources': {'src/runtime/worker.rs': 'f' * 64}, 'binaries': raw['binaries']}))
            self.assertTrue(driver.collect_provenance(directory, driver.PHASES, checkpoint)['checkpoint_verified'])
            checkpoint.write_text(json.dumps({'format': 'root-format/1', 'files': {'src/runtime/worker.rs': 'f' * 64}, 'binaries': raw['binaries']}))
            self.assertTrue(driver.collect_provenance(directory, driver.PHASES, checkpoint)['checkpoint_verified'])
            (directory / 'live_individual').write_bytes(b'replaced')
            with self.assertRaisesRegex(ValueError, 'checkpoint_binary_mismatch'):
                driver.collect_provenance(directory, driver.PHASES, checkpoint)

    def test_output_refuses_overwrite_before_process(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'existing.json'
            output.write_text('OLD EVIDENCE')
            with patch.object(driver.subprocess, 'run') as run:
                with self.assertRaises(SystemExit):
                    driver.main(['--credentials', '/absent', '--output', str(output)])
                run.assert_not_called()
            self.assertEqual(output.read_text(), 'OLD EVIDENCE')

    def test_preflight_failure_has_report_no_private_detail_no_process(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'failure.json'
            with patch.object(driver.subprocess, 'run') as run:
                code = driver.main(['--credentials', '/private/missing', '--binary-dir', '/absent/binaries', '--output', str(output)])
                run.assert_not_called()
            self.assertEqual(code, 1)
            data = json.loads(output.read_text())
            self.assertEqual(data['failure']['code'], 'preflight_failed')
            self.assertNotIn('/private/missing', output.read_text())


class ExplicitQA:
    """P03 public function signatures injected; no actual Docker/helper run."""
    def __init__(self, checkpoint, binary_dir, *, role='behavior', verify_fail_at=None, identity_fail_at=None):
        self.checkpoint, self.binary_dir = checkpoint, binary_dir
        self.locked, self.events, self.verifies, self.guards = False, [], 0, 0
        self.verify_fail_at, self.identity_fail_at = verify_fail_at, identity_fail_at
        self.manifest = {'id': 'fixture-frozen-v1', 'format': 'alia-p03-checkpoint/1',
                         'sources': {'scripts/test_live_lifecycle.py': driver.sha256(Path(driver.__file__))}}
        self.artifact = {'format': 'alia-p03-artifacts/1', 'build_commands': [{'command': ['fixture-build']}],
                         'build_environment': {'RUST_TEST_THREADS': '1'}, 'tool_versions_log': 'logs/tool-versions.log'}
        self.state = {'format': 'alia-p03-db/1', 'owner': 'a' * 32, 'role': role, 'status': 'ready',
                      'container': 'exact-owned-behavior', 'volume': 'exact-owned-volume',
                      'container_id': 'fixture-container-id', 'image_id': 'fixture-image-id',
                      'username': 'aliaqa', 'password': 'PRIVATE_DB_PASSWORD', 'port': 12345}
        self.common = SimpleNamespace(heavy_lock=self.lock, clean_env=lambda: {})
        self.artifacts = SimpleNamespace(verify_artifacts=self.verify)
        self.database = SimpleNamespace(load_state=lambda path: self.state, assert_owned=self.owned,
            public=lambda state: {k: state[k] for k in ('owner', 'role', 'container', 'volume', 'container_id', 'image_id', 'port')},
            url=lambda state, name: f"postgres://aliaqa:{state['password']}@127.0.0.1:12345/{name}",
            DOCKER=['fixture-docker'])

    @contextmanager
    def lock(self):
        self.locked = True
        self.events.append('lock_enter')
        try:
            yield None, 0.125
        finally:
            self.events.append('lock_exit')
            self.locked = False

    def verify(self, checkpoint, binary_dir):
        assert self.locked
        assert checkpoint == self.checkpoint and binary_dir == self.binary_dir
        self.events.append('verify')
        self.verifies += 1
        if self.verifies == self.verify_fail_at:
            raise ValueError('private artifact failure')
        return self.manifest, self.artifact

    def owned(self, state, kind):
        assert self.locked
        self.guards += 1
        self.events.append('guard_' + kind)
        if self.identity_fail_at and self.guards >= self.identity_fail_at:
            raise ValueError('identity replaced')
        return {'Id': 'fixture-' + kind}


class ExplicitModeTests(unittest.TestCase):
    def invoke(self, *, role='behavior', verify_fail_at=None, identity_fail_at=None, cleanup=None):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            checkpoint, binary_dir = directory / 'checkpoint', directory / 'artifacts/bin'
            (checkpoint / 'source/scripts').mkdir(parents=True)
            (checkpoint / 'source/scripts/test_live_lifecycle.py').write_bytes(Path(driver.__file__).read_bytes())
            (binary_dir / 'examples').mkdir(parents=True)
            (binary_dir / 'examples/live_uncertainty').write_bytes(b'fixture-executable')
            (binary_dir.parent / 'artifacts.json').write_text('{"fixture":true}')
            (checkpoint / 'manifest.json').write_text('{"fixture":true}')
            state, credentials, output = directory / 'state.json', directory / 'credentials.env', directory / 'report.json'
            state.write_text('{}')
            credentials.write_text('opencode-go\napikey: PRIVATE_PROVIDER_KEY\n')
            qa = ExplicitQA(checkpoint, binary_dir, role=role, verify_fail_at=verify_fail_at, identity_fail_at=identity_fail_at)
            trial = Trial(cleanup=cleanup)
            def run(command, **kwargs):
                self.assertTrue(qa.locked)
                qa.events.append('cleanup' if 'DROP DATABASE' in command[-1] else 'subprocess')
                return trial(command, **kwargs)
            original_read = Path.read_text
            def read_without_development_env(path, *args, **kwargs):
                if path == ROOT / '.env':
                    raise AssertionError('explicit mode read development .env')
                return original_read(path, *args, **kwargs)
            with patch.object(Path, 'read_text', read_without_development_env), patch.dict(driver.os.environ, {'ALIA_TOKENIZER_PATH': ''}):
                code = driver.main(['--credentials', str(credentials), '--checkpoint', str(checkpoint),
                                    '--binary-dir', str(binary_dir), '--test-db-state', str(state),
                                    '--uncertainty-only', '--output', str(output)], run=run, qa_loader=lambda path: qa)
            data = json.loads(output.read_text())
            public = output.read_text()
            self.assertNotIn('PRIVATE_PROVIDER_KEY', public)
            self.assertNotIn('PRIVATE_DB_PASSWORD', public)
            self.assertNotIn('private artifact failure', public)
            self.assertTrue(all(c[1]['cwd'] == checkpoint / 'source' for c in trial.calls))
            self.assertFalse(any('compose' in c[0] for c in trial.calls))
            self.assertFalse(any('rm' in c[0] for c in trial.calls))
            return code, data, qa, trial

    def test_frozen_input_success_has_no_checkout_or_compose_fallback(self):
        code, data, qa, trial = self.invoke()
        self.assertEqual(code, 0)
        self.assertTrue(data['engine_pass'])
        self.assertEqual(data['provenance']['checkpoint_id'], 'fixture-frozen-v1')
        self.assertEqual(data['provenance']['test_db']['role'], 'behavior')
        self.assertEqual(data['provenance']['queue_seconds'], 0.125)
        self.assertEqual(data['provenance']['artifact_build'], [{'command': ['fixture-build']}])
        self.assertEqual(data['provenance']['artifact_build_environment'], {'RUST_TEST_THREADS': '1'})
        self.assertEqual(data['final_verification']['status'], 'passed')
        self.assertEqual(qa.events[-1], 'lock_exit')
        self.assertLess(qa.events.index('cleanup'), len(qa.events) - 1)
        self.assertTrue(all(c[0][0] in ('fixture-docker', str(qa.binary_dir / 'examples/live_uncertainty')) for c in trial.calls))

    def test_final_artifact_failure_keeps_fees_and_cleanup(self):
        code, data, qa, trial = self.invoke(verify_fail_at=2)
        self.assertEqual(code, 1)
        self.assertFalse(data['engine_pass'])
        self.assertEqual(data['failure']['code'], 'final_verification_failed')
        self.assertEqual(data['accounting']['charged_tokens'], 12)
        self.assertEqual(data['cleanup']['status'], 'passed')
        self.assertEqual(qa.events[-1], 'lock_exit')

    def test_partial_or_mismatched_artifacts_refused_before_process(self):
        code, data, qa, trial = self.invoke(verify_fail_at=1)
        self.assertEqual(code, 1)
        self.assertEqual(data['failure']['code'], 'explicit_preflight_failed')
        self.assertEqual(trial.calls, [])
        self.assertEqual(qa.events[-1], 'lock_exit')

    def test_engineering_role_cannot_be_reused_for_behavior(self):
        code, data, qa, trial = self.invoke(role='engineering')
        self.assertEqual(code, 1)
        self.assertEqual(trial.calls, [])
        self.assertEqual(data['failure']['code'], 'explicit_preflight_failed')

    def test_replaced_resource_blocks_queries_and_destructive_cleanup(self):
        code, data, qa, trial = self.invoke(identity_fail_at=5)
        self.assertEqual(code, 1)
        self.assertFalse(data['engine_pass'])
        self.assertIsNone(data['accounting']['charged_tokens'])
        self.assertEqual(data['cleanup']['status'], 'failed')
        self.assertEqual(len(trial.calls), 1)  # create before replacement; nothing sent afterwards
        self.assertEqual(qa.events[-1], 'lock_exit')

    def test_cleanup_failure_keeps_known_accounting_while_lock_held(self):
        code, data, qa, trial = self.invoke(cleanup=subprocess.CompletedProcess([], 3, '', 'PRIVATE STDERR'))
        self.assertEqual(code, 1)
        self.assertEqual(data['cleanup']['exit'], 3)
        self.assertEqual(data['accounting']['charged_tokens'], 12)
        self.assertEqual(qa.events[-1], 'lock_exit')
        self.assertIn('cleanup', qa.events)

    def test_missing_explicit_inputs_never_load_environment_or_call_process(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            run, loader = unittest.mock.Mock(), unittest.mock.Mock()
            code = driver.main(['--credentials', '/absent', '--checkpoint', '/missing/checkpoint-directory',
                                '--output', str(output)], run=run, qa_loader=loader)
            self.assertEqual(code, 1)
            run.assert_not_called()
            loader.assert_not_called()
            self.assertEqual(json.loads(output.read_text())['failure']['code'], 'explicit_preflight_failed')

    def test_loader_uses_hash_bound_snapshot_modules_without_bytecode_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            checkpoint = Path(directory)
            sources = {}
            for name in ('common', 'freeze', 'artifacts', 'database'):
                relative = f'scripts/qa/{name}.py'
                target = checkpoint / 'source' / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / relative).read_bytes())
                sources[relative] = driver.sha256(target)
            (checkpoint / 'manifest.json').write_text(json.dumps({'format': 'alia-p03-checkpoint/1', 'sources': sources}))
            previous_common = sys.modules.get('common')
            qa = driver.load_frozen_qa(checkpoint)
            self.assertTrue(callable(qa.artifacts.verify_artifacts))
            self.assertTrue(callable(qa.database.load_state))
            self.assertTrue(callable(qa.common.heavy_lock))
            self.assertIs(sys.modules.get('common'), previous_common)
            self.assertFalse(list((checkpoint / 'source').rglob('*.pyc')))

    def test_unbound_snapshot_adapter_never_executes(self):
        with tempfile.TemporaryDirectory() as directory:
            checkpoint = Path(directory)
            target = checkpoint / 'source/scripts/qa/common.py'
            target.parent.mkdir(parents=True)
            target.write_text('raise RuntimeError("unbound code must not execute")')
            (checkpoint / 'manifest.json').write_text(json.dumps({'format': 'alia-p03-checkpoint/1',
                'sources': {'scripts/qa/common.py': '0' * 64}}))
            with self.assertRaisesRegex(ValueError, 'unbound QA adapter'):
                driver.load_frozen_qa(checkpoint)

    def test_symlink_component_refused_before_helpers(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            (directory / 'real').mkdir()
            (directory / 'alias').symlink_to(directory / 'real', target_is_directory=True)
            args = SimpleNamespace(checkpoint=directory / 'alias', binary_dir=directory / 'bin',
                                   test_db_state=directory / 'db.json')
            loader = unittest.mock.Mock()
            with self.assertRaisesRegex(ValueError, 'symlink'):
                driver.run_explicit(args, driver.PHASES, directory / 'out.json', qa_loader=loader)
            loader.assert_not_called()


class CheckerTests(unittest.TestCase):
    def test_legacy_final_snapshot_not_sum_or_final_ledger(self):
        source = {'reports': [{'usage': [{'charged': 10, 'unknown': False, 'status': 'settled'}]},
                              {'usage': [{'charged': 10, 'unknown': False, 'status': 'settled'}, {'charged': 20, 'unknown': False, 'status': 'settled'}]}]}
        result = checker.historical_accounting(source)
        self.assertEqual(result['observed_tokens'], 30)
        self.assertIsNone(result['charged_tokens'])
        self.assertFalse(result['complete'])
        self.assertIsNone(checker.historical_accounting({})['observed_tokens'])

    def test_missing_evidence_cannot_be_passed(self):
        review = {'cases_sha256': checker.digest(checker.CASES), 'reviewer': 'independent',
                  'method': 'independent_source_comparison', 'provider_self_assessment': False,
                  'sources': [], 'checks': [{'id': 'reading', 'outcome': 'passed', 'category': 'unverified',
                                             'comparison': 'Unproven', 'evidence': [{'source': 'missing', 'pointer': '/x'}]}]}
        result = checker.evaluate(review)
        self.assertIsNone(result['behavior_pass'])
        self.assertEqual(result['checks'][0]['outcome'], 'unverified')
        self.assertTrue(result['validation_errors'])
        self.assertEqual(len(result['checks']), 8)

    def test_wrong_source_hash_and_provider_self_assessment_fail(self):
        review = {'sources': [{'id': 'cases', 'path': str(checker.CASES), 'sha256': '0' * 64}],
                  'provider_self_assessment': True}
        result = checker.evaluate(review)
        self.assertIsNone(result['behavior_pass'])
        self.assertTrue(any('source unavailable' in e for e in result['validation_errors']))

    def test_nested_case_diagnostics_and_missing_case_reviews(self):
        report = {'reports': [{'cases': [{'review_diagnostics': [
            {'status': 'assessment_failed', 'validation_code': 'assessment_schema'},
            {'assessment': {'checks': [{'verdict': 'unsupported', 'target': {'kind': 'reply'}}]}}]}]}]}
        findings = checker.classify(report)
        self.assertEqual({f['category'] for f in findings}, {'review_protocol_error', 'semantic_veto'})
        self.assertTrue(all('/cases/0/review_diagnostics/' in f['evidence'] for f in findings))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trial.json'
            path.write_text(json.dumps({'reports': [{'cases': [{'id': 'a'}, {'id': 'b'}]}]}))
            review = {'cases_sha256': checker.digest(checker.CASES), 'reviewer': 'independent',
                      'method': 'independent_source_comparison', 'provider_self_assessment': False,
                      'sources': [{'id': 'trial', 'path': str(path), 'sha256': checker.digest(path)}],
                      'case_scope': {'source': 'trial', 'pointer': '/reports/0/cases'},
                      'case_checks': [{'id': 'a', 'outcome': 'passed', 'category': None,
                                       'comparison': 'Explicit comparison', 'evidence': [{'source': 'trial', 'pointer': '/reports/0/cases/0'}]}]}
            result = checker.evaluate(review)
            self.assertIsNone(result['case_behavior_pass'])
            self.assertEqual(result['case_checks'][-1]['id'], 'b')
            self.assertEqual(result['case_checks'][-1]['outcome'], 'unverified')

    def test_diagnostic_categories_are_distinct(self):
        report = {'reports': [{'diagnostics': [
            {'kind': 'decision.review_retry_scheduled', 'payload': {'code': 'assessment_schema'}},
            {'kind': 'decision.reviewed', 'payload': {'assessment': {'checks': [{'verdict': 'uncertain'}]}}},
            {'kind': 'run.stopped', 'payload': {'code': 'decision_grounding_failed'}}]}]}
        self.assertEqual({f['category'] for f in checker.classify(report)},
                         {'review_protocol_error', 'semantic_veto', 'runtime_rejection'})

    def test_fixture_failure_is_never_semantic_success(self):
        result = checker.evaluate({'checks': [{'id': 'unknown-history', 'outcome': 'failed',
                                             'category': 'author_error', 'comparison': '', 'evidence': []}]})
        self.assertIsNone(result['behavior_pass'])


if __name__ == '__main__':
    unittest.main()
