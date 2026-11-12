"""Dispatch/provenance/failure tests; mocked Docker is never real isolation evidence."""
from contextlib import contextmanager
import copy
import json
import os
import shutil
from pathlib import Path
import signal
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts/qa'))
import freeze
import sandbox_tests as qa
import run_engineering as engineering
from common import digest,read_json,write_new


class SandboxDispatch(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='alia-p03-dispatch-unit-')
        self.base=Path(self.temp.name);self.root=self.base/'source'
        (self.root/'src/adapters').mkdir(parents=True)
        (self.root/'tests').mkdir()
        (self.root/'src/lib.rs').write_text('#[sqlx::test]\n#[ignore]\nasync fn database_case() {}\n')
        (self.root/qa.SDK).write_text('// fixture SDK; not executable isolation evidence\n')
        self.tests='const IMAGE: &str =\n"'+qa.IMAGE+'";\n'+''.join(
            '#[test]\n#[ignore = "explicit Docker"]\nfn '+name+'() {}\n' for name in sorted(qa.CASES))
        (self.root/qa.TEST).write_text(self.tests)

    def tearDown(self):
        for base,dirs,files in os.walk(self.base):
            Path(base).chmod(0o700)
            for name in files:
                p=Path(base)/name
                if not p.is_symlink():p.chmod(0o600)
        self.temp.cleanup()

    def plan(self):return qa.dispatch_plan(self.root)

    def log(self):
        return '\n'.join('test '+name+' ... ok' for name in sorted(qa.CASES))+\
            '\ntest result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out\n'+\
            '\n     Running tests/sandbox.rs (build/debug/deps/sandbox-1234)\n'

    def outcomes(self,path):
        path.mkdir()
        for number,(name,status) in enumerate(qa.OUTCOMES.items()):
            unavailable=status=='unavailable'
            value={'execution_id':'alia-sandbox-'+format(number,'032x'),'status':status,
                'exit_code':None if unavailable else (7 if status=='code_failed' else
                    137 if status in ('resource_exceeded','timed_out') else 0),
                'stdout':'','stderr':('pinned sandbox image unavailable' if name=='unavailable_image'
                    else 'sandbox engine unavailable') if unavailable else '',
                'output_truncated':status=='output_limit','duration_ms':100,
                'image_digest':'python@sha256:'+'0'*64 if name=='unavailable_image' else qa.IMAGE,
                'isolation_verified':not unavailable,'cleanup_passed':True}
            write_new(path/(name+'.json'),value)
        write_new(path/'foreign_cleanup.json',{'execution_id':'alia-sandbox-'+format(12,'032x'),
            'actual_id':'a'*64,'mismatched_owner_refused':True,
            'foreign_fixture_preserved_before_owned_teardown':True,'manual_fixture_cleanup':True})

    def test_plan_preserves_all_sqlx_and_excludes_exact_nine_from_database(self):
        plan=self.plan();command=qa.database_command(plan)
        self.assertEqual(plan['sqlx_cases'],['database_case'])
        self.assertEqual(set(plan['docker_cases']),qa.CASES)
        self.assertEqual(command[:8],['cargo','test','--locked','--workspace','--lib','--tests','--','--ignored'])
        self.assertEqual(set(command[9::2]),qa.CASES)
        self.assertNotIn('--test',command)

    def test_legacy_plan_has_no_skip_or_new_mandatory_stage(self):
        (self.root/qa.SDK).unlink();(self.root/qa.TEST).unlink()
        self.assertFalse(self.plan()['sandbox_declared'])
        self.assertNotIn('--skip',qa.database_command(self.plan()))
        self.assertEqual(engineering.required_stages_for({'sources':{}}),engineering.REQUIRED_STAGES)

    def test_partial_declaration_fails_closed(self):
        (self.root/qa.SDK).unlink()
        with self.assertRaisesRegex(ValueError,'requires SDK'):self.plan()

    def test_missing_actual_test_refused(self):
        (self.root/qa.TEST).write_text(self.tests.replace('fn '+sorted(qa.CASES)[0],'fn replaced'))
        with self.assertRaises(ValueError):self.plan()

    def test_unpinned_image_refused(self):
        (self.root/qa.TEST).write_text(self.tests.replace(qa.IMAGE,'python:latest'))
        with self.assertRaisesRegex(ValueError,'digest'):self.plan()

    def test_unknown_ignored_domain_refused(self):
        (self.root/'tests/new.rs').write_text('#[test]\n#[ignore]\nfn unknown() {}')
        with self.assertRaisesRegex(ValueError,'unclassified'):self.plan()

    def test_skip_substring_cannot_hide_a_database_case(self):
        (self.root/'src/lib.rs').write_text('#[sqlx::test]\n#[ignore]\nasync fn prefix_'+sorted(qa.CASES)[0]+'() {}')
        with self.assertRaisesRegex(ValueError,'collides'):self.plan()

    def test_comments_raw_strings_and_character_literals_do_not_add_tests(self):
        fake='#[test]\n#[ignore]\nfn bogus() {}'
        with (self.root/'src/lib.rs').open('a') as f:
            f.write('\n/* nested /* x */ '+fake+' */\nconst X: &str = r###"'+fake+'"###;\n')
            f.write('const Y: &str = "escaped \\\" #[ignore]";\nconst C: char = \'"\';\n')
        self.assertEqual(self.plan()['sqlx_cases'],['database_case'])

    def test_unterminated_rust_string_or_comment_refused(self):
        for text in ('r##"unfinished','/* unfinished','"unfinished'):
            with self.subTest(text=text),self.assertRaises(ValueError):qa.rust_structure(text)

    def test_unclassified_cfg_attr_ignore_is_not_silently_missed(self):
        (self.root/'tests/new.rs').write_text('#[test]\n#[cfg_attr(any(),ignore)]\nfn unknown() {}')
        with self.assertRaises(ValueError):self.plan()

    def test_database_result_requires_every_case_actual_ok(self):
        plan=self.plan()
        self.assertTrue(qa.database_output_passed(plan,'test database_case ... ok\n'))
        for text in ('','test database_case ... ignored\n','test database_case ... FAILED\n',
                     'test database_case ... ok\ntest other ... ok\n',self.log()):
            with self.subTest(text=text):self.assertFalse(qa.database_output_passed(plan,text))

    def test_database_duplicate_names_are_counted_not_collapsed(self):
        plan={'sqlx_cases':['same','same']}
        self.assertFalse(qa.database_output_passed(plan,'test same ... ok\n'))
        self.assertTrue(qa.database_output_passed(plan,'test a::same ... ok\ntest b::same ... ok\n'))

    def test_docker_log_requires_individual_names_target_and_actual_summary(self):
        self.assertTrue(qa.docker_output_passed(self.log()))
        for text in (self.log().replace('9 passed','0 passed'),self.log().replace('0 ignored','9 ignored'),
                     self.log().replace('tests/sandbox.rs','tests/other.rs'),
                     self.log().replace(sorted(qa.CASES)[0],'unrelated'),self.log()+self.log()):
            with self.subTest(text=text):self.assertFalse(qa.docker_output_passed(text))

    def test_new_stage_is_mandatory_without_losing_old_ui_stages(self):
        from management_browser import CASES
        manifest={'sources':{qa.SDK:'a'*64,qa.TEST:'b'*64,
            **{case['script']:'c'*64 for case in CASES.values()}}}
        stages=engineering.required_stages_for(manifest)
        self.assertEqual(len(stages),27)
        self.assertIn('sandbox-tests',stages)
        self.assertTrue(set(engineering.REQUIRED_STAGES)<=set(stages))
        self.assertFalse(engineering.mandatory_stages_pass(
            [{'name':s,'status':'passed'} for s in stages if s!='sandbox-tests'],manifest))

    def test_clean_child_environment_has_admission_but_no_database_or_provider(self):
        with patch.dict(os.environ,{'DATABASE_URL':'synthetic_database','OPENAI_API_KEY':'synthetic_key',
                                    'DOCKER_CONFIG':'synthetic_config','ALIA_QA_LOCK_FD':'999'}):
            env=qa.child_environment(self.base/'target',self.base/'out',5)
        self.assertEqual(env['ALIA_QA_LOCK_FD'],'5')
        self.assertEqual(env['ALIA_SANDBOX_ACTUAL'],'1')
        self.assertEqual(env['RUST_TEST_THREADS'],'1')
        for key in ('DATABASE_URL','OPENAI_API_KEY','DOCKER_CONFIG'):self.assertNotIn(key,env)

    def test_outcome_contract_positive(self):
        path=self.base/'outcomes';self.outcomes(path)
        self.assertEqual(len(qa.validate_outcomes(path)),13)

    def test_outcome_contract_rejects_cleanup_pin_size_status_types_and_reused_handle(self):
        path=self.base/'outcomes';self.outcomes(path)
        file=path/'normal.json';original=read_json(file)
        for change in ({'cleanup_passed':False},{'image_digest':'python:latest'},{'isolation_verified':False},
                       {'duration_ms':True},{'stdout':'x'*65537},{'status':'timed_out'},
                       {'exit_code':1},{'execution_id':read_json(path/'network.json')['execution_id']}):
            file.write_text(json.dumps({**original,**change}))
            with self.subTest(change=list(change)),self.assertRaises(ValueError):qa.validate_outcomes(path)
        file.write_text(json.dumps(original))

    def test_missing_extra_and_symlinked_outcome_files_refused(self):
        path=self.base/'outcomes';self.outcomes(path)
        (path/'extra.json').write_text('{}')
        with self.assertRaises(ValueError):qa.validate_outcomes(path)
        (path/'extra.json').unlink()
        p=path/'foreign_cleanup.json';data=p.read_text();p.unlink()
        with self.assertRaises(ValueError):qa.validate_outcomes(path)
        outside=self.base/'foreign.json';outside.write_text(data);p.symlink_to(outside)
        with self.assertRaises(ValueError):qa.validate_outcomes(path)

    def test_foreign_cleanup_must_prove_preserved_then_owned_teardown(self):
        path=self.base/'outcomes';self.outcomes(path)
        p=path/'foreign_cleanup.json';value=read_json(p);value['manual_fixture_cleanup']=False
        p.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError,'foreign'):qa.validate_outcomes(path)

    def owned(self):
        name='alia-sandbox-'+'a'*32
        return {'id':'b'*64,'name':'/'+name,'image':qa.IMAGE,
                'labels':{'io.alia.sandbox.execution':name,'io.alia.sandbox.role':'oneshot'}}

    def test_cleanup_recovers_only_exact_owned_id_and_reports_recovery(self):
        value=self.owned();calls=[]
        resources=iter(([value],[value],[]))
        records,recovered=qa.cleanup_resources(lambda:next(resources),calls.append)
        self.assertTrue(recovered);self.assertTrue(all(v['passed'] for v in records))
        self.assertEqual(calls,[['container','rm','--force',value['id']]])

    def test_cleanup_refuses_foreign_owner_and_changed_identity(self):
        for changed in (False,True):
            value=self.owned();other=copy.deepcopy(value)
            other['labels']['io.alia.sandbox.execution']='foreign'
            calls=[];seq=iter(([value],[other],[other]) if changed else ([other],[other]))
            records,_=qa.cleanup_resources(lambda:next(seq),calls.append)
            self.assertFalse(all(v['passed'] for v in records));self.assertEqual(calls,[])

    def test_cleanup_query_and_remove_failure_keep_exact_known_handles(self):
        value=self.owned()
        for fail_query in (False,True):
            seq=iter(([value],RuntimeError('query failed'),[value]) if fail_query else ([value],[value],[value]))
            def resources():
                result=next(seq)
                if isinstance(result,Exception):raise result
                return result
            def remove(argv):raise RuntimeError('removal failed')
            records,_=qa.cleanup_resources(resources,remove)
            self.assertFalse(all(v['passed'] for v in records))
            self.assertTrue(any(v.get('id')==value['id'] for v in records))

    def report_fixture(self):
        out=self.base/'report';out.mkdir();self.outcomes(out/'outcomes')
        (out/'cargo.log').write_text(self.log())
        manifest={'sources':{qa.SDK:'a'*64,qa.TEST:'b'*64}}
        report={'format':'alia-p03-sandbox-tests/1','domain':'actual-Docker-not-SQLx',
            'passed':True,'final_integrity':True,'private_runtime_cleaned':True,'model_calls':0,
            'image_digest':qa.IMAGE,'checkpoint_sha256':'c'*64,'sources':manifest['sources'],
            'expected_cases':sorted(qa.CASES),'child_exit':0,'actual_tests':9,'outcome_records':13,
            'cleanup':[{'passed':True}]}
        write_new(out/'parent-report.json',report)
        return out,manifest,report

    def test_child_report_binding_positive(self):
        out,m,_=self.report_fixture()
        self.assertTrue(qa.stage_report_passed(out,m,'c'*64))

    def test_exit_zero_report_cannot_replace_bound_evidence(self):
        out,m,original=self.report_fixture()
        for change in ({'passed':False},{'sources':{}},{'checkpoint_sha256':'d'*64},
                       {'model_calls':False},{'cleanup':[]},{'actual_tests':0},
                       {'expected_cases':[]},{'private_runtime_cleaned':False}):
            (out/'parent-report.json').write_text(json.dumps({**original,**change}))
            with self.subTest(change=change):self.assertFalse(qa.stage_report_passed(out,m,'c'*64))

    def test_runner_declared_stage_requires_child_protocol_even_after_exit_zero(self):
        run=engineering.Run.__new__(engineering.Run)
        run.manifest={'sources':{qa.SDK:'a'*64,qa.TEST:'b'*64}}
        run.output=self.base;run.snapshot=self.base/'checkpoint';run.source=self.root;run.python=sys.executable
        run.report={'stages':[],'checkpoint_sha256':'c'*64}
        calls=[]
        def stage(name,argv,**kw):
            calls.append((name,argv,kw));run.report['stages'].append({'name':name,'status':'passed'})
            return True
        run.stage=stage
        run.run_sandbox_tests(self.base/'target')
        self.assertEqual(run.report['stages'][-1]['status'],'failed')
        self.assertEqual(calls[0][0],'sandbox-tests')
        self.assertIn('scripts/qa/sandbox_tests.py',calls[0][1])
        self.assertNotIn('--test-db-state',calls[0][1])

    def test_runner_legacy_never_starts_a_docker_stage(self):
        run=engineering.Run.__new__(engineering.Run);run.manifest={'sources':{}}
        with patch.object(run,'stage') as stage:
            run.run_sandbox_tests(self.base/'target')
            stage.assert_not_called()

    def mocked_stage(self,mode):
        checkpoint=self.base/'checkpoint';freeze.freeze(self.root,checkpoint)
        output=self.base/'stage';target=self.base/'target'
        args=SimpleNamespace(checkpoint=checkpoint,source_root=self.root,output=output,target_dir=target)
        calls=[]
        @contextmanager
        def locked(*resources, **kwargs):
            class Locks:
                queue_seconds=0.0
                pass_fds=(7,)
                def child_environment(self, environment=None):return dict(environment or {})
            yield Locks()
        def docker(command,**kwargs):
            calls.append(command)
            self.assertEqual(kwargs['pass_fds'],(7,))
            self.assertNotIn('DATABASE_URL',kwargs['env'])
            if 'inspect' in command and 'image' in command:
                return SimpleNamespace(returncode=0,stdout=json.dumps([qa.IMAGE]))
            return SimpleNamespace(returncode=0,stdout='')
        owner=self
        class Process:
            pid=123;returncode=None
            def __init__(self,argv,**kw):
                owner.assertEqual(kw['pass_fds'],(7,))
                owner.assertEqual(argv,['cargo','test','--locked','--test','sandbox','--','--ignored','--nocapture'])
                owner.assertNotIn('DATABASE_URL',kw['env'])
                path=Path(kw['env']['ALIA_SANDBOX_REPORT_DIR']);path.rmdir();owner.outcomes(path)
                if mode=='partial':(path/'normal.json').unlink()
            def communicate(self,timeout):
                if mode=='interrupt' and self.returncode is None:
                    self.returncode=-signal.SIGKILL
                    raise InterruptedError('injected interruption')
                self.returncode=0
                return owner.log(),''
            def poll(self):return self.returncode
        original_cleanup=qa.cleanup_resources
        def cleanup(*args):
            if mode=='cleanup_failure':raise RuntimeError('injected daemon query failure')
            return original_cleanup(*args)
        with patch.object(qa,'resource_locks',locked),patch.object(qa.subprocess,'run',side_effect=docker),\
                patch.object(qa.subprocess,'Popen',Process),patch.object(qa,'cleanup_resources',side_effect=cleanup):
            result=qa.run_stage(args)
        report=read_json(output/'parent-report.json')
        if mode=='cleanup_failure':
            self.assertFalse(report['private_runtime_cleaned'])
            recovery=Path(report['private_recovery_directory'])
            self.assertTrue(recovery.exists())
            self.assertEqual(len(report['reported_execution_handles']),13)
            # Only this unit's synthetic private files; no real Docker resource exists.
            shutil.rmtree(recovery)
        else:self.assertTrue(report['private_runtime_cleaned'])
        self.assertTrue(report['final_integrity'])
        self.assertFalse(any('pull' in c for c in calls))
        self.assertTrue(qa.stage_report_passed(output,freeze.verify(checkpoint),digest(checkpoint/'manifest.json'))
                        if mode=='success' else not report['passed'])
        return result,report

    def test_mocked_stage_positive_end_to_end_protocol(self):
        result,report=self.mocked_stage('success')
        self.assertEqual(result,0);self.assertEqual(report['actual_tests'],9)

    def test_mocked_partial_outcomes_fail_and_still_clean_private_runtime(self):
        result,report=self.mocked_stage('partial')
        self.assertEqual(result,1);self.assertIn('failure',report)

    def test_mocked_interruption_fails_with_report_and_cleanup(self):
        result,report=self.mocked_stage('interrupt')
        self.assertEqual(result,1);self.assertIn('failure',report)

    def test_mocked_cleanup_failure_retains_private_recovery_and_known_handles(self):
        result,report=self.mocked_stage('cleanup_failure')
        self.assertEqual(result,1);self.assertFalse(report['passed'])


if __name__=='__main__':unittest.main()
