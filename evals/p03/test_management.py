"""Offline protocol/lifecycle faults for accepted management browser integration."""
from contextlib import contextmanager
from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
import management_browser as mg
from common import digest, read_json, write_new
from freeze import freeze
from run_engineering import REQUIRED_STAGES, mandatory_stages_pass


class AcceptedInputs(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='alia-p03-management-unit-')
        self.base = Path(self.temp.name)
        self.root = self.base / 'source'
        self.root.mkdir()
        self.snapshot = self.base / 'checkpoint'
        self.pins = {}
        self.sources = {'p02-02': 'web/scripts/test-overview.mjs',
                        'p04-01': 'tests/memory_revisions.rs',
                        'p05-01': 'web/scripts/test-documents.mjs'}
        for name, path in self.sources.items():
            file = self.root / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text('accepted fixture ' + name)
            metadata = self.root / f'evals/p03/accepted-slices/{name}.json'
            metadata.parent.mkdir(parents=True, exist_ok=True)
            write_new(metadata, {'format':'alia-accepted-slice/1','id':name,'sources':{path:digest(file)}})
            self.pins[name] = digest(metadata)
        self.pin_patch = patch.dict(mg.SLICES, self.pins, clear=True)
        self.pin_patch.start()

    def tearDown(self):
        self.pin_patch.stop()
        for base, dirs, files in os.walk(self.base):
            Path(base).chmod(0o700)
            for name in files:
                if not (Path(base)/name).is_symlink():
                    (Path(base)/name).chmod(0o600)
        self.temp.cleanup()

    def validate(self):
        freeze(self.root, self.snapshot)
        return mg.validate_inputs(self.snapshot, self.snapshot/'source')

    def test_all_three_pins_and_P04_test_are_bound_to_candidate(self):
        _, bindings = self.validate()
        self.assertEqual(set(bindings),set(self.sources))
        self.assertTrue(all(v['source_count']==1 for v in bindings.values()))

    def test_missing_http_pg_test_rejects_complete_candidate(self):
        (self.root/self.sources['p04-01']).unlink()
        with self.assertRaisesRegex(ValueError,'missing or changed accepted source'):
            self.validate()

    def test_later_live_browser_code_cannot_replace_accepted_script(self):
        (self.root/self.sources['p05-01']).write_text('new unaccepted live version')
        with self.assertRaisesRegex(ValueError,'missing or changed accepted source'):
            self.validate()

    def test_accepted_manifest_bytes_must_match_pin(self):
        p=self.root/'evals/p03/accepted-slices/p02-02.json'
        p.write_text(p.read_text()+'\n')
        with self.assertRaisesRegex(ValueError,'manifest binding mismatch'):
            self.validate()

    def test_execution_source_cannot_fall_back_to_development_tree(self):
        self.validate()
        (self.root/self.sources['p02-02']).write_text('development changed')
        with self.assertRaisesRegex(ValueError,'execution source differs'):
            mg.validate_inputs(self.snapshot,self.root)
        mg.validate_inputs(self.snapshot,self.snapshot/'source')

    def test_metadata_cannot_omit_P04_test_even_if_other_source_is_valid(self):
        p=self.root/'evals/p03/accepted-slices/p04-01.json'
        other=self.root/'src/other.rs';other.parent.mkdir();other.write_text('other')
        p.write_text(json.dumps({'format':'alia-accepted-slice/1','id':'p04-01','sources':{'src/other.rs':digest(other)}}))
        mg.SLICES['p04-01']=digest(p)
        with self.assertRaisesRegex(ValueError,'P04 HTTP/PG regression test absent'):
            self.validate()

    def test_every_new_stage_is_required_and_unverified_never_full_pass(self):
        stages=[{'name':name,'status':'passed'} for name in REQUIRED_STAGES]
        self.assertTrue(mandatory_stages_pass(stages))
        for name in ['management-inputs','overview-browser','documents-browser']:
            with self.subTest(name=name):
                self.assertFalse(mandatory_stages_pass([s for s in stages if s['name']!=name]))
                modified=deepcopy(stages)
                next(s for s in modified if s['name']==name)['status']='unverified'
                self.assertFalse(mandatory_stages_pass(modified))
        self.assertFalse(mandatory_stages_pass(stages+[stages[-1]]))


class ProtocolAndLifecycle(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='alia-p03-lifecycle-unit-')
        self.base=Path(self.temp.name)
        self.binary=self.base/'artifacts/bin/alia'
        self.binary.parent.mkdir(parents=True)
        self.binary.write_text('fixed executable fixture')
        self.manifest={'id':'fixture','sources':{'src/accepted.rs':'a'*64}}
        self.args=SimpleNamespace(case='overview',checkpoint=self.base/'checkpoint',source_root=self.base/'source',
            binary_dir=self.binary.parent,web_dir=self.base/'artifacts/web',test_db_state=self.base/'private.json',
            output=self.base/'output')
        self.args.checkpoint.mkdir()
        (self.args.checkpoint/'manifest.json').write_text('{}')
        self.state={'role':'engineering','status':'ready','password':'private-db-pass',
                    'container':'exact-owned-test-container','username':'aliaqa'}
        self.active=False
        self.calls=[]
        self.procs=[]

    def tearDown(self):
        self.temp.cleanup()

    def child_report(self,case='overview'):
        return {'task':mg.CASES[case]['task'],'passed':True,'cleanup':True,
                'binary_sha256':digest(self.binary),'sources':self.manifest['sources'],
                'checks':[{'name':'check-'+str(i)} for i in range(mg.CASES[case]['groups'])]}

    def test_case_specific_environment_has_no_default_compose_or_provider_secret(self):
        for case,prefix in [('overview','ALIA_P02'),('documents','ALIA_P05')]:
            with self.subTest(case=case),patch.dict(os.environ,{'OPENCODE_API_KEY':'private-provider-fixture'}):
                env=mg.browser_environment({'PATH':'fixture-node-bin','DATABASE_URL':'must-not-inherit',
                     'OPENCODE_API_KEY':'must-not-inherit'},self.state,'alia_p03_fixture','token',
                     'http://127.0.0.1:12345',self.binary,self.base/'private-browser',case,99)
            self.assertNotIn('DATABASE_URL',env)
            self.assertNotIn('OPENCODE_API_KEY',env)
            self.assertEqual(env[prefix+'_DATABASE'],'alia_p03_fixture')
            self.assertEqual(env[prefix+'_LOCK_HELD'],'1')
            argv=json.loads(env[prefix+'_PSQL_COMMAND'])
            self.assertIn('exact-owned-test-container',argv)
            self.assertNotIn('compose',argv)
            self.assertNotIn(self.state['password'],json.dumps(argv))
            self.assertEqual(env['ALIA_TEST_BINARY'],str(self.binary))
            self.assertEqual(env['ALIA_QA_LOCK_FD'],'99')

    def test_child_success_requires_complete_cleanup_binary_and_source_evidence(self):
        good=self.child_report()
        self.assertTrue(mg.child_passed(good,'overview',digest(self.binary),self.manifest))
        bads=[{'task':'wrong'},{'passed':False},{'cleanup':False},{'binary_sha256':'0'*64},
              {'checks':good['checks'][:-1]},{'checks':[{'name':'same'}]*9},
              {'sources':{'src/accepted.rs':'wrong'}},{'sources':'malformed'},{'model_calls':1}]
        for change in bads:
            with self.subTest(change=change):
                self.assertFalse(mg.child_passed({**good,**change},'overview',digest(self.binary),self.manifest))

    def test_private_report_export_redacts_credentials_and_withholds_screenshots(self):
        private=self.base/'private-browser';private.mkdir()
        report={**self.child_report(),'error':'private-instance-token private-db-pass'}
        write_new(private/'report.json',report)
        (private/'failure.png').write_bytes(b'private image fixture')
        mg.export_child(private,self.base/'public',['private-instance-token','private-db-pass'])
        data=(self.base/'public/report.json').read_text()
        self.assertNotIn('private-instance-token',data)
        self.assertNotIn('private-db-pass',data)
        self.assertFalse((self.base/'public/failure.png').exists())

    def test_malformed_child_report_is_failed_and_sanitized_partial_evidence_retained(self):
        private=self.base/'private-browser';private.mkdir()
        (private/'report.json').write_text('{broken private-db-pass')
        with self.assertRaisesRegex(ValueError,'child report invalid'):
            mg.export_child(private,self.base/'public',['private-db-pass'])
        self.assertNotIn('private-db-pass',(self.base/'public/invalid-report.txt').read_text())

    def exercise(self, *, case='overview', node_exit=0, create_failure=False, startup_failure=False,
                 service_failure=False, drop_failure=False, child_timeout=False, final_failure=False,
                 child_cleanup=True, preflight_failure=False, wrong_role=False):
        self.args.case=case
        owner=self
        class Process:
            def __init__(self,child=False,env=None):
                self.child=child;self.env=env;self.returncode=node_exit if child else None
                self.once_timeout=child_timeout
            def poll(self):return self.returncode
            def send_signal(self,sig):self.returncode=0
            def terminate(self):self.returncode=-15
            def kill(self):self.returncode=-9
            def wait(self,timeout=None):
                if service_failure and not self.child and self.returncode==0:
                    self.returncode=7
                return self.returncode
            def communicate(self,timeout=None):
                if self.once_timeout:
                    self.once_timeout=False
                    raise subprocess.TimeoutExpired('node fixture',timeout)
                return 'private-db-pass '+self.env['ALIA_TEST_TOKEN'],'fixture stderr'
        def popen(command,**kwargs):
            self.assertTrue(self.active)
            if command[0]=='node':
                env=kwargs['env'];self.assertEqual(kwargs['pass_fds'],(99,))
                private=Path(env['ALIA_TEST_OUTPUT']);private.mkdir()
                child=self.child_report(case);child['cleanup']=child_cleanup
                write_new(private/'report.json',child)
                proc=Process(True,env)
            else:
                if startup_failure:raise OSError('injected service start failure')
                proc=Process()
            self.procs.append(proc)
            return proc
        def sql(state,statement):
            self.assertTrue(self.active)
            self.calls.append(statement)
            if create_failure and statement.startswith('CREATE'):raise RuntimeError('lost CREATE response')
            if drop_failure and statement.startswith('DROP'):raise RuntimeError('DROP failure')
            return ''
        @contextmanager
        def lock(*resources, **kwargs):
            self.active=True
            class Locks:
                queue_seconds=0.25
                pass_fds=(99,)
                def child_environment(self, environment=None):return dict(environment or {})
            try:yield Locks()
            finally:self.active=False
        artifact_result=(self.manifest,{})
        validation=(self.manifest,{'fixture':{}})
        state={**self.state,'role':'behavior' if wrong_role else 'engineering'}
        with patch.object(mg,'resource_locks',lock),patch.object(mg,'validate_inputs',
                side_effect=ValueError('missing accepted input') if preflight_failure else None,
                return_value=validation),patch.object(mg,'verify_artifacts',
                side_effect=[artifact_result,ValueError('final artifact mismatch')] if final_failure else None,
                return_value=artifact_result),patch.object(mg,'load_state',return_value=state),\
             patch.object(mg,'assert_owned',return_value={'owned':True}),\
             patch.object(mg,'url',return_value='postgres://fixture:private-db-pass@127.0.0.1:12345/test'),\
             patch.object(mg,'sql',side_effect=sql),patch.object(mg,'wait_ready'),\
             patch.object(mg.subprocess,'Popen',side_effect=popen):
            result=mg.run_case(self.args)
        self.assertFalse(self.active)
        report=read_json(self.args.output/'parent-report.json')
        self.assertNotIn('private-db-pass',json.dumps(report))
        return result,report

    def test_both_cases_use_distinct_generated_trial_database_and_cleanup(self):
        names=[]
        for case in ['overview','documents']:
            self.args.output=self.base/case
            code,report=self.exercise(case=case)
            self.assertEqual(code,0)
            self.assertTrue(report['passed'])
            self.assertTrue(all(x['passed'] for x in report['cleanup']))
            names.append(report['database'])
            self.assertEqual(report['child_checks'],mg.CASES[case]['groups'])
        self.assertEqual(len(set(names)),2)

    def test_lost_create_response_still_drops_exact_attempted_name(self):
        code,report=self.exercise(create_failure=True)
        self.assertEqual(code,1)
        self.assertTrue(self.calls[0].startswith('CREATE DATABASE alia_p03_'))
        self.assertTrue(self.calls[-1].startswith('DROP DATABASE IF EXISTS alia_p03_'))
        self.assertEqual(next(x for x in report['cleanup'] if x['resource']=='database')['passed'],True)

    def test_service_start_failure_still_cleans_trial_database_and_private_runtime(self):
        code,report=self.exercise(startup_failure=True)
        self.assertEqual(code,1)
        self.assertTrue(self.calls[-1].startswith('DROP'))
        self.assertIn('private-runtime',[x['resource'] for x in report['cleanup']])

    def test_service_shutdown_failure_does_not_skip_database_cleanup(self):
        code,report=self.exercise(service_failure=True)
        self.assertEqual(code,1)
        self.assertFalse(next(x for x in report['cleanup'] if x['resource']=='service')['passed'])
        self.assertTrue(next(x for x in report['cleanup'] if x['resource']=='database')['passed'])

    def test_database_cleanup_failure_is_a_failed_parent_with_recovery_name(self):
        code,report=self.exercise(drop_failure=True)
        self.assertEqual(code,1)
        failure=next(x for x in report['cleanup'] if x['resource']=='database')
        self.assertFalse(failure['passed'])
        self.assertRegex(failure['name'],r'^alia_p03_[0-9a-f]{32}$')

    def test_child_nonzero_or_timeout_preserves_report_and_cleans_resources(self):
        for timeout,exit_code in [(False,1),(True,0)]:
            with self.subTest(timeout=timeout):
                self.args.output=self.base/('timeout' if timeout else 'child-failed')
                code,report=self.exercise(node_exit=exit_code,child_timeout=timeout)
                self.assertEqual(code,1)
                self.assertTrue((self.args.output/'browser/report.json').exists())
                self.assertTrue(all(c['passed'] for c in report['cleanup']))
                self.assertNotIn('private-db-pass',(self.args.output/'child-stdout.log').read_text())

    def test_child_cleanup_failure_cannot_be_hidden_by_exit_zero(self):
        code,report=self.exercise(child_cleanup=False)
        self.assertEqual(code,1)
        self.assertFalse(report['passed'])

    def test_final_integrity_failure_still_publishes_failed_report(self):
        code,report=self.exercise(final_failure=True)
        self.assertEqual(code,1)
        self.assertFalse(report['final_integrity'])
        self.assertTrue(all(c['passed'] for c in report['cleanup']))

    def test_preflight_missing_input_and_wrong_role_never_start_process_or_sql(self):
        for preflight,role in [(True,False),(False,True)]:
            with self.subTest(preflight=preflight):
                self.args.output=self.base/('preflight' if preflight else 'role')
                code,report=self.exercise(preflight_failure=preflight,wrong_role=role)
                self.assertEqual(code,1)
                self.assertEqual(self.procs,[])
                self.assertEqual(self.calls,[])

    def test_old_output_refused_before_any_process_or_database(self):
        self.args.output.mkdir()
        write_new(self.args.output/'parent-report.json',{'old':'preserve'})
        with self.assertRaises(FileExistsError):self.exercise()
        self.assertEqual(read_json(self.args.output/'parent-report.json'),{'old':'preserve'})
        self.assertEqual(self.calls,[])


if __name__=='__main__':
    unittest.main()
