"""Offline final-layer, child-contract and conditional-stage tests; no real services."""
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
import management_browser as mg
import run_engineering as engineering
import test_management as lifecycle_fixtures
from common import digest, read_json, write_new
from freeze import freeze

NEW_CASES = ('memory-revisions', 'source-inspector', 'beliefs', 'relationships', 'notes')


class LayeredInputs(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='alia-p03-workspaces-input-')
        self.base = Path(self.temp.name)
        self.source = self.base / 'development'
        self.source.mkdir()
        self.checkpoint = self.base / 'checkpoint'
        self.base_pins = dict(mg.SLICES)
        self.optional_pins = dict(mg.OPTIONAL_SLICES)
        self.add_layer('p02-02', {'web/scripts/test-overview.mjs': 'overview'})
        self.add_layer('p04-01', {'src/adapters/memory_commands.rs': 'old adapter',
            'src/service/memory_commands.rs': 'old service', 'tests/memory_revisions.rs': 'revision tests'})
        self.add_layer('p05-01', {'web/scripts/test-documents.mjs': 'documents',
            'web/src/features/Settings.tsx': 'documents-only settings'})
        self.patches = [patch.dict(mg.SLICES, self.base_pins, clear=True),
                        patch.dict(mg.OPTIONAL_SLICES, self.optional_pins, clear=True)]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in reversed(self.patches):
            p.stop()
        for base, dirs, files in os.walk(self.base):
            Path(base).chmod(0o700)
            for name in files:
                (Path(base) / name).chmod(0o600)
        self.temp.cleanup()

    def add_layer(self, name, content):
        sources = {}
        for path, text in content.items():
            p = self.source / path
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
            sources[path] = digest(p)
        p = self.source / f'evals/p03/accepted-slices/{name}.json'
        p.parent.mkdir(parents=True, exist_ok=True)
        write_new(p, {'format': 'alia-accepted-slice/1', 'id': name, 'sources': sources})
        if name in self.base_pins:
            self.base_pins[name] = digest(p)
        else:
            self.optional_pins[name] = digest(p)
            if hasattr(self, 'patches'):
                mg.OPTIONAL_SLICES[name] = digest(p)

    def all_layers(self):
        self.add_layer('p02-03', {'web/scripts/test-memory-revisions.mjs': 'revision browser',
            'web/src/features/memory/MemoryInspector.tsx': 'first inspector'})
        self.add_layer('p04-02', {'src/adapters/memory_commands.rs': 'history adapter',
            'src/service/memory_commands.rs': 'history service',
            'tests/memory_revisions.rs': 'revision tests', 'tests/memory_history.rs': 'history tests'})
        self.add_layer('p04-03', {'src/adapters/source_events.rs': 'safe event reader',
            'tests/source_events.rs': 'safe event tests'})
        self.add_layer('p05-02', {'web/scripts/test-notes.mjs': 'notes browser',
            'web/src/features/Settings.tsx': 'documents and notes settings'})
        self.add_layer('p02-04', {'web/scripts/test-source-inspector.mjs': 'source browser',
            'web/src/features/memory/MemoryInspector.tsx': 'final shared inspector',
            'web/src/features/sources/SourceEventInspector.tsx': 'shared source component'})
        self.add_layer('p04-04', {'web/scripts/test-beliefs.mjs': 'belief browser',
            'web/src/features/Cognition.tsx': 'cognition imports shared source component'})
        self.add_layer('p05-03', {'web/scripts/test-relationships.mjs': 'relationship browser'})

    def validate(self):
        freeze(self.source, self.checkpoint)
        return mg.validate_inputs(self.checkpoint, self.checkpoint / 'source')

    def test_legacy_keeps_exact_original21_stages_and_three_packages(self):
        manifest, bindings = self.validate()
        self.assertEqual(set(bindings), {'p02-02', 'p04-01', 'p05-01'})
        self.assertEqual(mg.declared_cases(manifest), ('overview', 'documents'))
        self.assertEqual(engineering.required_stages_for(manifest), engineering.REQUIRED_STAGES)

    def test_final_order_accepts_three_intentional_overlays_and_requires26_stages(self):
        self.all_layers()
        manifest, bindings = self.validate()
        self.assertEqual(len(bindings), 10)
        self.assertEqual(set(mg.declared_cases(manifest)), {'overview', 'documents', *NEW_CASES})
        self.assertEqual(len(engineering.required_stages_for(manifest)), 26)
        self.assertEqual((self.checkpoint / 'source/web/src/features/memory/MemoryInspector.tsx').read_text(),
                         'final shared inspector')

    def test_historical_memory_inspector_cannot_replace_final_overlay(self):
        self.all_layers()
        (self.source / 'web/src/features/memory/MemoryInspector.tsx').write_text('first inspector')
        with self.assertRaisesRegex(ValueError, 'missing or changed accepted source'):
            self.validate()

    def test_historical_memory_api_cannot_replace_accepted_history_api(self):
        self.all_layers()
        (self.source / 'src/service/memory_commands.rs').write_text('old service')
        with self.assertRaisesRegex(ValueError, 'missing or changed accepted source'):
            self.validate()

    def test_historical_settings_cannot_remove_notes_overlay(self):
        self.all_layers()
        (self.source / 'web/src/features/Settings.tsx').write_text('documents-only settings')
        with self.assertRaisesRegex(ValueError, 'missing or changed accepted source'):
            self.validate()

    def test_declared_browser_requires_backend_dependency_manifest(self):
        self.all_layers()
        (self.source / 'evals/p03/accepted-slices/p04-03.json').unlink()
        with self.assertRaisesRegex(ValueError, 'manifest binding mismatch'):
            self.validate()

    def test_dependency_http_pg_test_cannot_be_missing(self):
        self.all_layers()
        (self.source / 'tests/memory_history.rs').unlink()
        with self.assertRaisesRegex(ValueError, 'missing or changed accepted source'):
            self.validate()

    def test_declared_manifest_cannot_hide_a_missing_browser_script(self):
        self.all_layers()
        (self.source / 'web/scripts/test-notes.mjs').unlink()
        with self.assertRaisesRegex(ValueError, 'missing or changed accepted source'):
            self.validate()

    def test_optional_manifest_bytes_are_pinned(self):
        self.all_layers()
        p = self.source / 'evals/p03/accepted-slices/p02-04.json'
        p.write_text(p.read_text() + '\n')
        with self.assertRaisesRegex(ValueError, 'manifest binding mismatch'):
            self.validate()

    def test_optional_manifest_identity_is_validated(self):
        self.all_layers()
        p = self.source / 'evals/p03/accepted-slices/p02-04.json'
        data = read_json(p)
        data['id'] = 'wrong'
        p.write_text(json.dumps(data))
        mg.OPTIONAL_SLICES['p02-04'] = digest(p)
        with self.assertRaisesRegex(ValueError, 'invalid accepted manifest'):
            self.validate()

    def test_untrusted_script_name_never_selects_an_executable_or_stage(self):
        p = self.source / 'web/scripts/test-arbitrary.mjs'
        p.write_text('not an accepted executable')
        manifest, _ = self.validate()
        self.assertEqual(mg.declared_cases(manifest), ('overview', 'documents'))
        self.assertEqual(len(engineering.required_stages_for(manifest)), 21)

    def test_each_declared_stage_is_mandatory_and_failure_cannot_hide_in_exit0_subset(self):
        self.all_layers()
        manifest, _ = self.validate()
        stages = [{'name': n, 'status': 'passed'} for n in engineering.required_stages_for(manifest)]
        self.assertTrue(engineering.mandatory_stages_pass(stages, manifest))
        for case in NEW_CASES:
            name = case + '-browser'
            self.assertFalse(engineering.mandatory_stages_pass([s for s in stages if s['name'] != name], manifest))
            for status in ('failed', 'unverified'):
                changed = deepcopy(stages)
                next(s for s in changed if s['name'] == name)['status'] = status
                self.assertFalse(engineering.mandatory_stages_pass(changed, manifest))
        self.assertFalse(engineering.mandatory_stages_pass(stages + [stages[-1]], manifest))

    def test_runner_dispatches_only_fixed_declared_case_commands(self):
        self.all_layers()
        self.validate()
        run = engineering.Run(self.checkpoint, self.base / 'run', Path(sys.executable), None)
        run.states = [(self.base / 'private.json', {})]
        with patch.object(run, 'stage', return_value=True) as stage:
            run.run_management_browsers(True, True)
        self.assertEqual([c.args[0] for c in stage.call_args_list],
                         [case + '-browser' for case in mg.declared_cases(run.manifest)])
        for call in stage.call_args_list:
            command = call.args[1]
            self.assertIn('scripts/qa/management_browser.py', command)
            self.assertIn(str(self.checkpoint), command)
            self.assertIn(str(run.output / 'artifacts/bin'), command)
            self.assertIn(str(self.base / 'private.json'), command)
        run.run_management_browsers(False, True)
        self.assertEqual(len(run.report['stages']), 7)
        self.assertTrue(all(s['status'] == 'unverified' for s in run.report['stages']))


def report_fixture(manifest, binary, case):
    config = mg.CASES[case]
    required = set(config['report_files'])
    for prefix in config.get('report_dirs', ()):
        required.update(p for p in manifest['sources'] if p.startswith(prefix))
    names = ['actual-group-' + str(i) for i in range(config['groups'])]
    return {'task': config['task'], 'passed': True, 'cleanup': True, 'model_calls': 0,
            'binary_sha256': digest(binary), 'sources': {p: manifest['sources'][p] for p in required},
            'checks': names if case == 'source-inspector' else [{'name': n} for n in names]}


class ChildContracts(unittest.TestCase):
    def setUp(self):
        self.harness = lifecycle_fixtures.ProtocolAndLifecycle(methodName='runTest')
        self.harness.setUp()
        sources = {}
        for case in NEW_CASES:
            config = mg.CASES[case]
            sources[config['script']] = 'a' * 64
            for p in config['report_files']:
                sources[p] = 'a' * 64
            for prefix in config.get('report_dirs', ()):
                sources[prefix + 'fixture.tsx'] = 'a' * 64
        self.harness.manifest = {'id': 'fixture', 'sources': sources}
        self.harness.child_report = lambda case: report_fixture(
            self.harness.manifest, self.harness.binary, case)

    def tearDown(self):
        self.harness.tearDown()

    def test_all_five_real_shapes_prefixes_and_exclusive_cleanup(self):
        names = []
        for case, prefix in zip(NEW_CASES, ['ALIA_P02', 'ALIA_P02', 'ALIA_P04', 'ALIA_P05', 'ALIA_P05']):
            self.harness.args.output = self.harness.base / case
            code, report = self.harness.exercise(case=case)
            self.assertEqual(code, 0, report)
            self.assertEqual(report['child_checks'], mg.CASES[case]['groups'])
            self.assertTrue(report['final_integrity'])
            self.assertTrue(all(c['passed'] for c in report['cleanup']))
            child = self.harness.procs[-1]
            self.assertEqual(child.env[prefix + '_DATABASE'], report['database'])
            self.assertEqual(child.env[prefix + '_LOCK_HELD'], '1')
            self.assertIn('exact-owned-test-container', child.env[prefix + '_PSQL_COMMAND'])
            self.assertNotIn('DATABASE_URL', child.env)
            names.append(report['database'])
        self.assertEqual(len(set(names)), 5)

    def test_undeclared_case_starts_no_process_or_sql(self):
        del self.harness.manifest['sources'][mg.CASES['relationships']['script']]
        code, report = self.harness.exercise(case='relationships')
        self.assertEqual(code, 1)
        self.assertIn('not declared', report['failure'])
        self.assertEqual(self.harness.calls, [])
        self.assertEqual(self.harness.procs, [])

    def test_missing_stale_or_extra_reported_sources_fail_closed(self):
        for case in NEW_CASES:
            good = self.harness.child_report(case)
            self.assertTrue(mg.child_passed(good, case, digest(self.harness.binary), self.harness.manifest))
            missing = dict(good['sources'])
            missing.pop(next(iter(missing)))
            stale = {p: '0' * 64 for p in good['sources']}
            extra = {**good['sources'], 'missing-path': None}
            for sources in (missing, stale, extra):
                self.assertFalse(mg.child_passed({**good, 'sources': sources}, case,
                                               digest(self.harness.binary), self.harness.manifest))

    def test_wrong_group_shapes_duplicates_empty_and_explicit_failed_group_rejected(self):
        for case in NEW_CASES:
            good = self.harness.child_report(case)
            wrong = [{'name': 'wrong-shape'}] * 16 if case == 'source-inspector' else ['wrong-shape'] * 16
            bads = [None, wrong, good['checks'][:-1], [good['checks'][0]] * len(good['checks'])]
            if case != 'source-inspector':
                bads.append([{'name': 'x' + str(i), 'passed': False} for i in range(len(good['checks']))])
            for checks in bads:
                self.assertFalse(mg.child_passed({**good, 'checks': checks}, case,
                                               digest(self.harness.binary), self.harness.manifest))

    def test_sql_cleanup_failure_and_malformed_cleanup_are_not_exit0_success(self):
        good = self.harness.child_report('source-inspector')
        for change in ({'sql_fault_cleanup': False}, {'cleanup_errors': ['failed']},
                       {'cleanup_errors': {}}, {'model_calls': 1}, {'model_calls': False},
                       {'cleanup': False}, {'task': 'wrong'}):
            self.assertFalse(mg.child_passed({**good, **change}, 'source-inspector',
                                           digest(self.harness.binary), self.harness.manifest))

    def test_new_case_timeout_still_sanitizes_and_cleans(self):
        code, report = self.harness.exercise(case='source-inspector', child_timeout=True)
        self.assertEqual(code, 1)
        self.assertTrue(report['child_timeout'])
        self.assertTrue(all(c['passed'] for c in report['cleanup']))

    def test_new_case_service_shutdown_failure_does_not_skip_database_drop(self):
        code, report = self.harness.exercise(case='notes', service_failure=True)
        self.assertEqual(code, 1)
        self.assertFalse(next(c for c in report['cleanup'] if c['resource'] == 'service')['passed'])
        self.assertTrue(next(c for c in report['cleanup'] if c['resource'] == 'database')['passed'])


class ObservationExecution(unittest.TestCase):
    def evidence(self, new=False, passed=None, ignored=0, omit=None):
        examples = ['live_uncertainty', 'live_individual'] + (['live_belief_revisions'] if new else [])
        counts = [2, 2, 15] if new else [2, 2]
        if passed is not None:
            counts[-1] = max(0, passed - sum(counts[:-1]))
        summaries = '\n'.join(f'test result: ok. {count} passed; 0 failed; '
            f'{ignored if i == len(counts)-1 else 0} ignored; 0 measured; 0 filtered out'
            for i, count in enumerate(counts))
        return summaries + '\n' + '\n'.join(
            'Running unittests examples/' + n + '.rs (/isolated/test)' for n in examples if n != omit)

    def test_legacy_two_targets_and_four_executions_stay_mandatory(self):
        manifest = {'sources': {}}
        self.assertEqual(engineering.observation_examples(manifest), ['live_uncertainty', 'live_individual'])
        self.assertTrue(engineering.observation_output_passed(manifest, self.evidence()))
        self.assertFalse(engineering.observation_output_passed(manifest, self.evidence(passed=3)))

    def test_declared_observer_requires_actual15_new_executions_and_target(self):
        manifest = {'sources': {'examples/live_belief_revisions.rs': 'a' * 64}}
        self.assertTrue(engineering.observation_output_passed(manifest, self.evidence(new=True)))
        for text in [self.evidence(), self.evidence(new=True, passed=18),
                     self.evidence(new=True, ignored=1),
                     self.evidence(new=True, omit='live_belief_revisions'), 'Finished build only']:
            self.assertFalse(engineering.observation_output_passed(manifest, text))

    def test_runner_executes_conditional_example_without_build_only_or_filter_flags(self):
        with tempfile.TemporaryDirectory(prefix='alia-p03-observer-stage-') as temp:
            root = Path(temp)
            run = object.__new__(engineering.Run)
            run.output = root
            run.manifest = {'sources': {'examples/live_belief_revisions.rs': 'a' * 64}}
            run.report = {'stages': []}
            def stage(name, command, **kwargs):
                self.assertEqual(command, ['cargo', 'test', '--locked', '--example', 'live_uncertainty',
                    '--example', 'live_individual', '--example', 'live_belief_revisions'])
                (root / 'observation.log').write_text(self.evidence(new=True, passed=4))
                run.report['stages'].append({'name': name, 'status': 'passed', 'exit_code': 0,
                                            'log': 'observation.log'})
                return True
            with patch.object(run, 'stage', side_effect=stage):
                run.run_observation_tests()
            self.assertEqual(run.report['stages'][0]['status'], 'failed')
            self.assertIn('executions', run.report['stages'][0]['failure'])

    def test_other_targets_cannot_substitute_for15_new_observer_executions(self):
        manifest = {'sources': {'examples/live_belief_revisions.rs': 'a' * 64}}
        text = self.evidence(new=True).replace('2 passed', '30 passed').replace('15 passed', '3 passed')
        self.assertFalse(engineering.observation_output_passed(manifest, text))


if __name__ == '__main__':
    unittest.main()
