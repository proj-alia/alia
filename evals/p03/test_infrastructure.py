"""Failure-oriented tests for isolation/publication/identity checks. No paid calls."""
from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
import freeze
import database
from artifacts import verify_artifacts, REQUIRED_BINARIES
from common import write_new, read_json, digest
import common
import run_engineering
from run_engineering import redact, test_counts, Run


class Infrastructure(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='alia-p03-unit-')
        self.base = Path(self.temp.name)
        self.root = self.base / 'development'
        (self.root / 'src').mkdir(parents=True)
        (self.root / 'src/lib.rs').write_text('// frozen v1\n')
        (self.root / 'Cargo.toml').write_text('[package]\nname="fixture"\n')
        (self.root / '.env').write_text('SECRET=excluded\n')
        (self.root / 'data').mkdir()
        (self.root / 'data/secret.json').write_text('{}')
        (self.root / 'contexts').mkdir()
        (self.root / 'contexts/01.md').write_text('immutable original')
        (self.root / 'contexts/03.md').write_text('not required')
        self.out = self.base / 'checkpoint'

    def tearDown(self):
        for base, dirs, files in os.walk(self.base):
            if not Path(base).is_symlink():
                Path(base).chmod(0o700)
            for name in files:
                file = Path(base)/name
                if not file.is_symlink():
                    file.chmod(0o600)
        self.temp.cleanup()

    def test_freeze_excludes_runtime_and_only_copies_required_originals(self):
        m = freeze.freeze(self.root, self.out)
        self.assertEqual(set(m['sources']), {'Cargo.toml','src/lib.rs','contexts/01.md'})
        self.assertEqual((self.out/'source/contexts/01.md').read_text(), 'immutable original')
        self.assertEqual(freeze.verify(self.out), m)

    def test_later_development_changes_do_not_change_frozen_source(self):
        m = freeze.freeze(self.root, self.out)
        (self.root/'src/lib.rs').write_text('// development v2')
        (self.root/'src/new.rs').write_text('// new source')
        self.assertEqual(freeze.verify(self.out), m)
        self.assertEqual((self.out/'source/src/lib.rs').read_text(), '// frozen v1\n')

    def test_concurrent_source_mutation_refuses_manifest_publication(self):
        with self.assertRaisesRegex(ValueError, 'changed during'):
            freeze.freeze(self.root, self.out, lambda: (self.root/'src/lib.rs').write_text('// changed'))
        self.assertFalse((self.out/'manifest.json').exists())
        self.assertTrue((self.out/'freeze-failure.json').exists())

    def test_old_checkpoint_and_report_refuse_overwrite(self):
        freeze.freeze(self.root, self.out)
        before = (self.out/'manifest.json').read_bytes()
        with self.assertRaises(FileExistsError):
            freeze.freeze(self.root, self.out)
        self.assertEqual((self.out/'manifest.json').read_bytes(), before)
        p = self.base/'report.json'
        write_new(p, {'usage_unknown':None, 'passed':False})
        with self.assertRaises(FileExistsError):
            write_new(p, {'passed':True})
        self.assertIsNone(read_json(p)['usage_unknown'])
        self.assertFalse(read_json(p)['passed'])

    def test_corrupt_manifest_path_traversal_or_hash_refused(self):
        freeze.freeze(self.root, self.out)
        p = self.out/'manifest.json'
        p.chmod(0o600)
        original = read_json(p)
        for source in ({'../secret':'0'*64}, {'src/lib.rs':'0'*64}, {'src/lib.rs':'invalid'}):
            with self.subTest(source=source):
                p.write_text(json.dumps({**original, 'sources':source}))
                with self.assertRaises(ValueError):
                    freeze.verify(self.out)

    def test_extra_hidden_file_or_symlink_rejected(self):
        freeze.freeze(self.root,self.out)
        source = self.out/'source'
        source.chmod(0o700)
        (source/'.env').write_text('injected secret')
        with self.assertRaises(ValueError):
            freeze.verify(self.out)
        (source/'.env').unlink()
        (source/'escape').symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            freeze.verify(self.out)
        (source/'escape').unlink()

    def test_source_symlinks_refused(self):
        outside = self.base/'outside.txt'
        outside.write_text('external fixture')
        (self.root/'src/escape.rs').symlink_to(outside)
        with self.assertRaisesRegex(ValueError,'symlink'):
            freeze.freeze(self.root,self.out)

    def state(self):
        owner='a'*32
        return {'format':'alia-p03-db/1','username':'aliaqa','owner':owner,'role':'engineering','container':f'alia-p03-{owner}-engineering',
                'volume':f'alia-p03-{owner}-engineering-data','container_id':'owned-container',
                'password':'never_print_this','status':'ready'}

    def test_cleanup_refuses_unknown_names_or_mismatched_labels(self):
        state = self.state()
        path = self.base/'private.json'
        write_new(path,state,private=True)
        with patch.object(database,'inspect',return_value={'Config':{'Labels':{}},'Id':'owned-container'}), patch.object(database,'docker') as remove:
            with self.assertRaises(ValueError):
                database.cleanup(path)
            remove.assert_not_called()
        state['container']='production-db'
        with self.assertRaises(ValueError):
            database.validate_state(state)

    def test_cleanup_failure_retains_credentials_and_diagnostic_state(self):
        path=self.base/'private.json'
        write_new(path,self.state(),private=True)
        with patch.object(database,'assert_owned',return_value={'owned':True}), patch.object(database,'docker',side_effect=RuntimeError('injected removal failure')):
            result=database.cleanup(path)
        self.assertFalse(result['passed'])
        self.assertEqual(len(result['errors']),2)
        self.assertEqual(database.load_state(path)['password'],'never_print_this')
        self.assertFalse(database.load_state(path)['cleanup']['passed'])

    def test_cleanup_success_revokes_stored_password(self):
        path=self.base/'private.json'
        write_new(path,self.state(),private=True)
        with patch.object(database,'assert_owned',return_value=None):
            self.assertTrue(database.cleanup(path)['passed'])
        self.assertNotIn('password',database.load_state(path))

    def test_artifacts_cannot_claim_a_different_checkpoint_or_be_overwritten(self):
        freeze.freeze(self.root,self.out)
        artifacts=self.base/'artifacts'
        (artifacts/'bin').mkdir(parents=True)
        (artifacts/'bin/alia').write_text('fixed binary')
        write_new(artifacts/'artifacts.json',{'format':'alia-p03-artifacts/1','checkpoint_sha256':'wrong','files':{'bin/alia':'0'*64}})
        with self.assertRaisesRegex(ValueError,'checkpoint'):
            verify_artifacts(self.out,artifacts/'bin')
        with self.assertRaises(FileExistsError):
            write_new(artifacts/'artifacts.json',{})

    def test_accounting_redaction_and_ignored_are_not_passes(self):
        text='test result: ok. 3 passed; 0 failed; 172 ignored; 0 measured; 0 filtered out'
        self.assertEqual(test_counts(text)['passed'],3)
        self.assertEqual(test_counts(text)['ignored'],172)
        raw='postgres://test:never_print_this@127.0.0.1:1234/postgres Bearer ABCD never_print_this'
        redacted=redact(raw,['never_print_this'])
        self.assertNotIn('never_print_this',redacted)
        self.assertNotIn('ABCD',redacted)
        self.assertNotIn('postgres://',redacted)

    def artifact_fixture(self):
        freeze.freeze(self.root,self.out)
        base=self.base/'artifacts'
        for name in REQUIRED_BINARIES:
            file=base/'bin'/name
            file.parent.mkdir(parents=True,exist_ok=True)
            file.write_text('frozen binary '+name)
        (base/'web').mkdir()
        (base/'web/index.html').write_text('<html></html>')
        files={p.relative_to(base).as_posix():digest(p) for p in base.rglob('*') if p.is_file()}
        m={'format':'alia-p03-artifacts/1','checkpoint_sha256':digest(self.out/'manifest.json'),'files':files}
        write_new(base/'artifacts.json',m)
        return base,m

    def test_unrelated_only_manifest_does_not_certify_present_service(self):
        freeze.freeze(self.root,self.out)
        base=self.base/'artifacts'
        (base/'bin').mkdir(parents=True)
        (base/'bin/alia').write_text('unlisted service')
        (base/'web').mkdir()
        (base/'unrelated').write_text('unrelated')
        write_new(base/'artifacts.json',{'format':'alia-p03-artifacts/1',
            'checkpoint_sha256':digest(self.out/'manifest.json'),'files':{'unrelated':digest(base/'unrelated')}})
        with self.assertRaisesRegex(ValueError,'required runtime'):
            verify_artifacts(self.out,base/'bin',base/'web')

    def test_complete_artifact_tree_verifies_then_unlisted_web_asset_is_rejected(self):
        base,m=self.artifact_fixture()
        verify_artifacts(self.out,base/'bin',base/'web')
        (base/'web/unlisted.js').write_text('unlisted')
        with self.assertRaisesRegex(ValueError,'file set'):
            verify_artifacts(self.out,base/'bin',base/'web')

    def test_missing_required_web_or_binary_digest_is_rejected(self):
        base,m=self.artifact_fixture()
        (base/'bin/alia').write_text('modified service')
        with self.assertRaisesRegex(ValueError,'hash mismatch'):
            verify_artifacts(self.out,base/'bin',base/'web')
        (base/'bin/alia').write_text('frozen binary alia')
        del m['files']['web/index.html']
        (base/'artifacts.json').write_text(json.dumps(m))
        with self.assertRaisesRegex(ValueError,'required runtime'):
            verify_artifacts(self.out,base/'bin',base/'web')

    def test_artifact_parent_directory_symlink_cannot_escape(self):
        base,m=self.artifact_fixture()
        (base/'bin').rename(base/'real-bin')
        (base/'bin').symlink_to(base/'real-bin',target_is_directory=True)
        with self.assertRaisesRegex(ValueError,'symlink'):
            verify_artifacts(self.out,base/'bin',base/'web')
        (base/'bin').unlink()

    def test_absent_docker_objects_are_case_insensitive_but_daemon_errors_fail(self):
        from types import SimpleNamespace
        for kind,error in [('container','No such container: test'),('volume','get test: no such volume')]:
            with self.subTest(kind=kind),patch.object(database.subprocess,'run',return_value=SimpleNamespace(returncode=1,stderr=error,stdout='[]')):
                self.assertIsNone(database.inspect(kind,'exact-owned-fixture'))
        with patch.object(database.subprocess,'run',return_value=SimpleNamespace(returncode=1,stderr='Cannot connect to daemon',stdout='')):
            with self.assertRaises(RuntimeError):
                database.inspect('volume','exact-owned-fixture')

    def test_publish_then_verify_round_trip_includes_p01_compatibility_manifest(self):
        freeze.freeze(self.root,self.out)
        run=Run(self.out,self.base/'run',Path(sys.executable),None)
        target=self.base/'target'
        for name in REQUIRED_BINARIES:
            file=target/'debug'/name
            file.parent.mkdir(parents=True,exist_ok=True)
            file.write_text('actual frozen artifact '+name)
        (run.output/'web-dist').mkdir()
        (run.output/'web-dist/index.html').write_text('<html>frozen web</html>')
        run.publish_artifacts(target,True)
        base=run.output/'artifacts'
        _,artifacts=verify_artifacts(self.out,base/'bin',base/'web')
        self.assertIn('p01-manifest.json',artifacts['files'])
        self.assertEqual(artifacts['files']['p01-manifest.json'],digest(base/'p01-manifest.json'))
        (base/'p01-manifest.json').write_text('{}')
        with self.assertRaisesRegex(ValueError,'hash mismatch'):
            verify_artifacts(self.out,base/'bin',base/'web')

    def test_nested_lock_supports_existing_flock_cli_descriptor_without_flag(self):
        from common import resource_locks
        with resource_locks(('unit-model','nested')) as outer:
            with patch.dict(os.environ):
                os.environ.pop('ALIA_QA_LOCK_FD',None)
                with resource_locks(('unit-model','nested')) as inner:
                    key=next(iter(outer.descriptors))
                    self.assertEqual(os.fstat(outer.descriptors[key]).st_ino,
                                     os.fstat(inner.descriptors[key]).st_ino)

    def test_resource_lock_classes_are_independent_and_child_handoff_is_identity_checked(self):
        with tempfile.TemporaryDirectory(prefix='alia-lock-root-') as root, \
             patch.object(common, 'LOCK_ROOT', Path(root)):
            with common.resource_locks(('database','shared')) as database_lock:
                with common.resource_locks(('target',str(Path(root)/'target'))) as target_lock:
                    self.assertNotEqual(database_lock.pass_fds, target_lock.pass_fds)
                    child_environment=database_lock.child_environment({})
                    with common.resource_locks(('database','shared'),
                            environment=child_environment) as child_lock:
                        key=next(iter(database_lock.descriptors))
                        self.assertEqual(child_lock.descriptors[key],database_lock.descriptors[key])
                    invalid=dict(child_environment)
                    invalid[common.LOCK_ENV]=json.dumps({key:target_lock.pass_fds[0]})
                    with self.assertRaisesRegex(ValueError,'identity mismatch'):
                        with common.resource_locks(('database','shared'),environment=invalid):
                            self.fail('mismatched descriptor admitted')

    def test_independent_resources_overlap_while_same_resource_waits(self):
        with tempfile.TemporaryDirectory(prefix='alia-lock-root-') as root, \
             patch.object(common, 'LOCK_ROOT', Path(root)):
            held=threading.Event(); release=threading.Event()
            target_acquired=threading.Event(); same_acquired=threading.Event()
            def database_owner():
                with common.resource_locks(('database','one')):
                    held.set(); release.wait(2)
            def target_user():
                if held.wait(2):
                    with common.resource_locks(('target','one')):
                        target_acquired.set()
            def same_database_user():
                if held.wait(2):
                    with common.resource_locks(('database','one')):
                        same_acquired.set()
            owner=threading.Thread(target=database_owner)
            target=threading.Thread(target=target_user)
            same=threading.Thread(target=same_database_user)
            owner.start(); target.start(); same.start()
            try:
                self.assertTrue(target_acquired.wait(1), 'independent target lock was blocked')
                time.sleep(.05)
                self.assertFalse(same_acquired.is_set(), 'same database lock was not exclusive')
            finally:
                release.set()
                for thread in (owner,target,same):thread.join(2)
            self.assertFalse(any(thread.is_alive() for thread in (owner,target,same)))
            self.assertTrue(same_acquired.is_set())

    def test_connection_url_rejects_port_not_bound_to_owned_container(self):
        state={**self.state(),'port':12345}
        obj={'Config':{'Labels':database.labels(state['owner'],state['role'])},
             'Id':state['container_id'],'NetworkSettings':{'Ports':{'5432/tcp':[
                 {'HostIp':'127.0.0.1','HostPort':'54321'}]}}}
        with patch.object(database,'inspect',return_value=obj):
            with self.assertRaisesRegex(ValueError,'host port differs'):
                database.url(state)
        del state['format']
        with self.assertRaisesRegex(ValueError,'state format'):
            database.validate_state(state)

    def test_runner_preserves_virtualenv_interpreter_path(self):
        freeze.freeze(self.root,self.out)
        python=self.base/'venv/bin/python'
        python.parent.mkdir(parents=True)
        python.symlink_to(sys.executable)
        run=Run(self.out,self.base/'run',python,None)
        self.assertEqual(run.python,str(python.absolute()))
        self.assertNotEqual(run.python,str(python.resolve()))

    def test_database_entry_preserves_recorded_build_profile_but_not_provider_secret(self):
        from types import SimpleNamespace
        spec=importlib.util.spec_from_file_location('p03_database_driver',ROOT/'scripts/test_database.py')
        driver=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(driver)
        freeze.freeze(self.root,self.out)
        argv=['test_database.py','--checkpoint',str(self.out),'--test-db-state',str(self.base/'private.json'),
              '--target-dir',str(self.base/'isolated-target')]
        with patch.object(sys,'argv',argv),patch.dict(os.environ,{'CARGO_BUILD_JOBS':'4','CARGO_PROFILE_DEV_DEBUG':'0',
                'CARGO_PROFILE_TEST_DEBUG':'0','OPENCODE_API_KEY':'private-fixture-never-inherit'}),patch.object(driver,'load_state',
                return_value={'role':'engineering'}),patch.object(driver,'assert_owned'),patch.object(driver,'url',
                return_value='fixture-test-db-url'),patch.object(driver.subprocess,
                'run',return_value=SimpleNamespace(returncode=0)) as command:
            self.assertEqual(driver.main(),0)
        env=command.call_args.kwargs['env']
        for name,value in [('CARGO_BUILD_JOBS','4'),('CARGO_PROFILE_DEV_DEBUG','0'),('CARGO_PROFILE_TEST_DEBUG','0'),('RUST_TEST_THREADS','1')]:
            self.assertEqual(env[name],value)
        self.assertNotIn('OPENCODE_API_KEY',env)
        self.assertEqual(env['DATABASE_URL'],'fixture-test-db-url')
        self.assertEqual(env['CARGO_TARGET_DIR'],str(self.base/'isolated-target'))
        argv=command.call_args.args[0]
        self.assertIn('--workspace',argv)
        self.assertIn('--tests',argv)  # includes newly accepted memory_revisions HTTP/PG test
        self.assertIn('--ignored',argv)
        self.assertNotIn('--test',argv)  # no hardcoded target list can omit a new test module

    def test_partial_startup_carries_exact_recovery_state_and_can_clean_owned_volume(self):
        private=self.base/'private'
        private.mkdir(mode=0o700)
        with patch.object(database,'image_from_compose',return_value='pinned-image'), patch.object(database,'docker',side_effect=['owned-volume',RuntimeError('run failed')]):
            with self.assertRaises(database.ProvisionError) as raised:
                database.provision(self.root,'engineering',private)
        path=raised.exception.state_path
        state=database.load_state(path)
        self.assertEqual(state['status'],'startup_failed')
        self.assertIn('password',state)
        def objects(kind,name):
            return None if kind=='container' else {'Labels':database.labels(state['owner'],state['role'])}
        with patch.object(database,'inspect',side_effect=objects), patch.object(database,'docker') as remove:
            self.assertTrue(database.cleanup(path)['passed'])
        remove.assert_called_once_with(['volume','rm',state['volume']])

    def test_cleanup_and_final_verification_failure_are_reported_under_same_lock(self):
        freeze.freeze(self.root,self.out)
        run=Run(self.out,self.base/'run',Path(sys.executable),None)
        run.states=[(self.base/'private.json',self.state())]
        active=[]
        @contextmanager
        def lock(*resources, **kwargs):
            active.append(True)
            try:
                yield type('Locks',(),{'queue_seconds':0,'pass_fds':(),
                    'child_environment':staticmethod(lambda environment=None: dict(environment or {}))})()
            finally:
                active.pop()
        def cleanup(path):
            self.assertEqual(active,[True])
            return {'passed':False,'errors':[{'error':'injected'}]}
        with patch.object(run_engineering,'resource_locks',lock), patch.object(run,'stage',side_effect=RuntimeError('stage failed')), patch.object(run_engineering,'cleanup',side_effect=cleanup), patch.object(run_engineering,'verify',side_effect=ValueError('snapshot damaged')):
            self.assertEqual(run.run(),1)
        report=read_json(self.base/'run/report.json')
        self.assertFalse(report['full_engineering_pass'])
        self.assertFalse(report['snapshot_verified_after'])
        self.assertEqual(report['snapshot_verified_after_error'],'ValueError')
        self.assertFalse(report['cleanup'][0]['passed'])
        self.assertEqual(active,[])

    def test_required_unverified_stage_cannot_be_full_pass(self):
        freeze.freeze(self.root,self.out)
        run=Run(self.out,self.base/'run',Path(sys.executable),None)
        run.skipped('P02-dedicated-browser','injected missing assets')
        with patch.object(run,'stage',side_effect=RuntimeError('stop before work')):
            self.assertEqual(run.run(),1)
        self.assertFalse(read_json(self.base/'run/report.json')['full_engineering_pass'])


if __name__=='__main__':
    unittest.main()
