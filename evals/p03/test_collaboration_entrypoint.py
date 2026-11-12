"""Real r4 metadata-gate integration with SYNTHETIC reviews, no actual approval."""
from contextlib import contextmanager
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts/qa'))
import collaboration
import run_engineering as eng
from common import digest,read_json
from freeze import freeze, inventory

CARD,REGISTRY,ROOT_REVIEW,P01_REVIEW,EVIDENCE,BINDING=eng.CAPSULE_PATHS


class CollaborationEntrypoint(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='alia-p03-card-entry-');self.addCleanup(self.cleanup)
        self.base=Path(self.temp.name);self.source=self.base/'development';self.source.mkdir()
        for name in eng.COLLABORATION_R4:
            p=self.source/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/name,p)
        (self.source/'src').mkdir();(self.source/'src/lib.rs').write_text('// synthetic task source\n')
        p=self.source/EVIDENCE;p.parent.mkdir(parents=True)
        p.write_text(json.dumps({'format':'alia-candidate-validation/1','text':'synthetic validation'}))
        for name,reviewer in [(ROOT_REVIEW,'Root'),(P01_REVIEW,'P01')]:
            (self.source/name).write_text(json.dumps({'format':'alia-candidate-review/1','reviewer':reviewer,'text':'synthetic review'}))
        (self.source/'src/outside.rs').write_text('// outside task.sources, still reviewed candidate content\n')
        self.card={'format':'alia-task-card/1','task_id':'SYNTHETIC-QA','revision':1,'owner':'P03',
            'reviewers':['Root','P01'],'risk':'high','baseline':'synthetic-checkpoint','sources':{'src/lib.rs':digest(self.source/'src/lib.rs')},
            'dependencies':{'SYNTHETIC-DEP':{'revision':1,'subject_sha256':'d'*64}},
            'interfaces':{'task-card':'alia-task-card/1'},'scope':['src/lib.rs'],'goal':'Synthetic entry gate test',
            'acceptance':['No work before bound metadata'],'delta':'Initial synthetic fixture'}
        self.registry={'format':'alia-task-registry/1','interfaces':{'task-card':'alia-task-card/1'},'tasks':{
            'SYNTHETIC-DEP':{'revision':1,'subject_sha256':'d'*64,'state':'accepted'},
            'SYNTHETIC-QA':{'revision':1,'subject_sha256':collaboration.subject_digest(self.card),
                            'state':'engineering_pending','released_by':'Root'}}}
        evidence={EVIDENCE:digest(self.source/EVIDENCE)};subject=collaboration.subject_digest(self.card)
        self.card['reviews']=[{'reviewer':reviewer,'subject_sha256':subject,'decision':'approved',
            'open_blockers':0,'evidence':evidence} for reviewer in ('Root','P01')]
        self.card['validation']={'subject_sha256':subject,'status':'passed','commands':[{'command':'synthetic-test','exit_code':0}],
            'not_run':['This is not a real review or product test'],'evidence':evidence}
        self.checkpoint=self.base/'checkpoint'
        self.rebind_synthetic_release()

    def rebind_synthetic_release(self):
        content=eng.candidate_content(inventory(self.source))
        (self.source/BINDING).write_text(json.dumps({'format':'alia-candidate-binding/1',
            'algorithm':'alia-candidate-content/1','capsule_paths':list(eng.CAPSULE_PATHS),'content_sha256':content}))
        self.card['interfaces']['candidate-content']=content
        self.registry['interfaces']['candidate-content']=content
        subject=collaboration.subject_digest(self.card)
        self.registry['tasks']['SYNTHETIC-QA'].update(revision=self.card['revision'],subject_sha256=subject)
        for review,path in zip(self.card['reviews'],[ROOT_REVIEW,P01_REVIEW]):
            review.update(subject_sha256=subject,evidence={path:digest(self.source/path)})
        self.card['validation'].update(subject_sha256=subject,evidence={EVIDENCE:digest(self.source/EVIDENCE)})

    def cleanup(self):
        import os
        for base,dirs,files in os.walk(self.base):
            Path(base).chmod(0o700)
            for name in files:(Path(base)/name).chmod(0o600)
        self.temp.cleanup()

    def frozen(self):
        for name,value in ((CARD,self.card),(REGISTRY,self.registry)):
            (self.source/name).write_text(json.dumps(value))
        freeze(self.source,self.checkpoint)
        return self.checkpoint/'source'

    def check(self,card=CARD,registry=REGISTRY):
        source=self.frozen();return eng.collaboration_inputs(self.checkpoint,source,card,registry)

    def test_bound_real_r4_ready_gate_before_engineering(self):
        result=self.check();self.assertTrue(result['passed']);self.assertEqual(result['gate'],'ready')
        self.assertEqual(result['mode'],'COLLAB-02/r4');self.assertEqual(result['reviewers'],['Root','P01'])
        self.assertIn('not established',result['engineering_acceptance'])

    def test_new_checkpoint_cannot_claim_legacy_without_cards(self):
        with self.assertRaisesRegex(ValueError,'new candidate requires'):self.check(None,None)

    def test_old_checkpoint_name_does_not_authorize_new_source(self):
        self.checkpoint=self.base/'belief-context-errors-20261112-n5r3'
        with self.assertRaisesRegex(ValueError,'new candidate requires'):self.check(None,None)

    def test_partial_metadata_pair_rejected(self):
        with self.assertRaisesRegex(ValueError,'supplied together'):self.check(CARD,None)

    def test_relative_metadata_paths_cannot_escape(self):
        with self.assertRaisesRegex(ValueError,'unsafe relative path'):self.check('../escape.json',REGISTRY)

    def test_absolute_metadata_paths_are_not_frozen_aliases(self):
        with self.assertRaisesRegex(ValueError,'unsafe relative path'):self.check(str(self.source/CARD),REGISTRY)

    def test_unreviewed_helper_source_cannot_replace_r4(self):
        (self.source/'scripts/qa/collaboration.py').write_text('# unreviewed successor\n')
        with self.assertRaisesRegex(ValueError,'r4 runtime binding mismatch'):self.check()

    def test_unreviewed_common_dependency_cannot_replace_r4(self):
        (self.source/'scripts/qa/common.py').write_text('# unreviewed dependency\n')
        with self.assertRaisesRegex(ValueError,'r4 runtime binding mismatch'):self.check()

    def test_missing_independent_review_cannot_use_implement_gate(self):
        self.card['reviews']=self.card['reviews'][:1]
        with self.assertRaisesRegex(ValueError,'independent review missing'):self.check()

    def test_author_cannot_review_own_source(self):
        self.card['reviewers']=['P03']
        with self.assertRaisesRegex(ValueError,'author'):self.check()

    def test_root_release_is_required(self):
        self.registry['tasks']['SYNTHETIC-QA']['released_by']=None
        with self.assertRaisesRegex(ValueError,'Root release required'):self.check()

    def test_stale_revision_is_rejected(self):
        self.registry['tasks']['SYNTHETIC-QA']['revision']=2
        with self.assertRaisesRegex(ValueError,'stale'):self.check()

    def test_dependency_must_be_accepted_at_exact_version(self):
        self.registry['tasks']['SYNTHETIC-DEP']['subject_sha256']='e'*64
        with self.assertRaisesRegex(ValueError,'dependency'):self.check()

    def test_failed_or_unknown_command_is_not_ready(self):
        self.card['validation']['commands'][0]['exit_code']=None
        with self.assertRaisesRegex(ValueError,'commands'):self.check()

    def test_changed_evidence_is_not_ready_even_in_new_freeze(self):
        (self.source/EVIDENCE).write_text('{"format":"alia-candidate-validation/1","text":"changed"}')
        with self.assertRaisesRegex(ValueError,'changed source/evidence'):self.check()

    def test_p01_original_added_outside_source_counterexample_is_rejected(self):
        first=self.check();subject=first['subject_sha256'];original=digest(self.checkpoint/'manifest.json')
        (self.source/'src/unreviewed.rs').write_text('// P01 original harmless counterexample\n')
        self.checkpoint=self.base/'second-checkpoint'
        with self.assertRaisesRegex(ValueError,'candidate-content binding mismatch'):self.check()
        self.assertNotEqual(original,digest(self.checkpoint/'manifest.json'))
        self.assertEqual(subject,collaboration.subject_digest(self.card))

    def test_deleted_outside_source_invalidates_old_release(self):
        (self.source/'src/outside.rs').unlink()
        with self.assertRaisesRegex(ValueError,'candidate-content binding mismatch'):self.check()

    def test_non_code_candidate_additions_are_also_bound(self):
        for index,name in enumerate(['migrations/0001.sql','config/new.json','docs/new.md']):
            with self.subTest(name=name):
                p=self.source/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('{}')
                self.checkpoint=self.base/f'non-code-{index}'
                with self.assertRaisesRegex(ValueError,'candidate-content binding mismatch'):self.check()
                p.unlink()

    def test_recomputed_content_without_new_root_release_is_rejected(self):
        original=json.loads(json.dumps(self.registry['tasks']['SYNTHETIC-QA']))
        (self.source/'src/outside.rs').write_text('// changed\n')
        self.card['revision']=2;self.rebind_synthetic_release()
        self.registry['tasks']['SYNTHETIC-QA']=original
        with self.assertRaisesRegex(ValueError,'stale or unregistered'):self.check()

    def test_modified_outside_source_invalidates_old_release(self):
        (self.source/'src/outside.rs').write_text('// changed\n')
        with self.assertRaisesRegex(ValueError,'candidate-content binding mismatch'):self.check()

    def test_content_rebinding_without_new_independent_reviews_is_rejected(self):
        (self.source/'src/outside.rs').write_text('// changed\n')
        old_reviews=json.loads(json.dumps(self.card['reviews']))
        self.card['revision']=2;self.rebind_synthetic_release();self.card['reviews']=old_reviews
        with self.assertRaisesRegex(ValueError,'stale review'):self.check()

    def test_new_revision_release_and_independent_reviews_allow_changed_candidate(self):
        old=collaboration.subject_digest(self.card)
        (self.source/'src/outside.rs').write_text('// newly reviewed\n')
        self.card['revision']=2;self.rebind_synthetic_release()
        result=self.check();self.assertTrue(result['passed']);self.assertNotEqual(old,result['subject_sha256'])

    def test_capsule_cannot_exclude_code_migration_configuration_or_wildcards(self):
        value=read_json(self.source/BINDING)
        for index,name in enumerate(['src/outside.rs','migrations/0001.sql','config/runtime.json','src/*','src/']):
            with self.subTest(name=name):
                changed={**value,'capsule_paths':value['capsule_paths']+[name]}
                (self.source/BINDING).write_text(json.dumps(changed));self.checkpoint=self.base/f'exclusion-{index}'
                with self.assertRaisesRegex(ValueError,'exclusion schema mismatch'):self.check()

    def test_capsule_directory_does_not_exclude_arbitrary_json(self):
        (self.source/(eng.CAPSULE_BASE+'extra.json')).write_text('{}')
        with self.assertRaisesRegex(ValueError,'candidate-content binding mismatch'):self.check()

    def test_capsule_json_cannot_smuggle_unknown_configuration(self):
        value=read_json(self.source/EVIDENCE);value['runtime_config']={'allow':True}
        (self.source/EVIDENCE).write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError,'validation schema mismatch'):self.check()

    def test_capsule_fixed_paths_cannot_be_aliased(self):
        alias='evals/p03/alias.json';(self.source/alias).write_text(json.dumps(self.card))
        self.rebind_synthetic_release()
        with self.assertRaisesRegex(ValueError,'fixed card/registry paths'):self.check(alias,REGISTRY)

    def test_capsule_required_metadata_cannot_be_missing(self):
        (self.source/ROOT_REVIEW).unlink()
        with self.assertRaisesRegex(ValueError,'missing or unbound candidate capsule'):self.check()

    def test_run_rejects_before_resource_lock_process_or_database(self):
        self.frozen();run=eng.Run(self.checkpoint,self.base/'run',Path(sys.executable),None)
        with patch.object(eng,'resource_locks') as locks,patch.object(eng.subprocess,'Popen') as process,patch.object(eng,'provision') as db:
            self.assertEqual(run.run(),1);locks.assert_not_called();process.assert_not_called();db.assert_not_called()
        self.assertFalse(run.report['full_engineering_pass']);self.assertFalse(run.report['subset_commands_pass'])
        self.assertTrue(all(s['status']=='unverified' for s in run.report['stages']))

    def test_run_binds_ready_gate_before_candidate_lock(self):
        self.frozen();run=eng.Run(self.checkpoint,self.base/'run',Path(sys.executable),None,task_card=Path(CARD),task_registry=Path(REGISTRY))
        @contextmanager
        def locked(*args,**kwargs):
            self.assertTrue(run.report['collaboration']['passed'])
            self.assertEqual(args,(('candidate','full-matrix'),))
            yield type('Locks',(),{'queue_seconds':0})()
        with patch.object(eng,'resource_locks',side_effect=locked),patch.object(run,'run_locked',return_value=77):
            self.assertEqual(run.run(),77)


if __name__=='__main__':unittest.main()
