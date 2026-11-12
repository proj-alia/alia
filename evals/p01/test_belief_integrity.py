"""Offline mutations of complete synthetic records; never runs Rust/DB/models."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from belief_integrity import CASES, canonical, check_observation, load_cases, read_json
from belief_integrity import bind_packet, make_fixture, uid


class BeliefEvidenceTests(unittest.TestCase):
    def check_bad(self, value, code):
        result=check_observation(value)
        self.assertFalse(result['machine_ready_for_semantic_review'],result)
        self.assertNotEqual(result['machine_evidence_status'],'complete')
        self.assertIn(code,[i['code']for i in result['issues']])
        self.assertEqual(result['semantic_verdict'],'unverified')
        self.assertIsNone(result['live_model_pass'])
        return result

    def test_current_question_must_bind_actual_context_and_manifest(self):
        o=make_fixture('PB-01');o['context']['current_event_id']=uid(999)
        self.check_bad(o,'current_event_binding_missing')
        o=make_fixture('PB-01');o['context']['input_manifest'].remove(uid(203))
        self.check_bad(o,'current_event_binding_missing')

    def test_empty_selection_cannot_hide_omission(self):
        o=make_fixture('PB-05')
        o['context']['selected_beliefs']={'items':[],'has_more':False}
        self.check_bad(o,'empty_selection_without_omission')

    def test_superseded_history_must_link_to_current_focus(self):
        for change in ('missing','unlinked','still_current'):
            o=make_fixture('PB-04')
            if change=='missing':o['reference']['beliefs'][1]['supersedes']=None
            elif change=='unlinked':o['reference']['beliefs'][1]['supersedes']=uid(999)
            else:o['reference']['beliefs'][0]['status']='current'
            with self.subTest(change=change):
                self.check_bad(o,'supersession_history_binding_missing')

    def test_all_seven_positive_cases_are_fixture_readiness_only(self):
        self.assertEqual(len(load_cases()),7)
        for cid in load_cases():
            with self.subTest(case=cid):
                result=check_observation(make_fixture(cid))
                self.assertEqual(result['machine_evidence_status'],'complete',result)
                self.assertTrue(result['machine_ready_for_semantic_review'])
                self.assertEqual(result['declared_data_mode'],'fixture')
                self.assertEqual(result['provenance_status'],'fixture_only')
                self.assertEqual(result['semantic_verdict'],'unverified')
                self.assertIsNone(result['live_model_pass'])

    def test_missing_counter_body_manifest_or_packet_cannot_be_ready(self):
        for where in ('event','manifest','packet','role'):
            o=make_fixture('PB-02')
            if where=='event':o['context']['events']=[e for e in o['context']['events']if e['id']!=uid(202)]
            elif where=='manifest':o['context']['input_manifest'].remove(uid(202))
            elif where=='packet':o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events']=[e for e in o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events']if e['record']['id']!=uid(202)]
            else:o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events'][1]['roles']=[]
            with self.subTest(where=where):
                result=check_observation(o)
                self.assertFalse(result['machine_ready_for_semantic_review'])
                self.assertIsNone(result['live_model_pass'])

    def test_counter_text_cannot_be_removed_from_context_and_packet_together(self):
        o=make_fixture('PB-02')
        o['context']['events'][1]['payload']={}
        bind_packet(o)
        self.check_bad(o,'event_reference_hash_mismatch')
        o=make_fixture('PB-02');o['reference']['event_hashes'].pop(uid(202))
        self.check_bad(o,'reference_event_hash_missing')

    def test_counter_list_silently_dropped_is_detected_against_reference(self):
        o=make_fixture('PB-02');o['context']['selected_beliefs']['items'][0]['counter_event_ids']=[]
        bind_packet(o)
        self.check_bad(o,'belief_reference_mismatch')
        o['reference']['beliefs'][0]['counter_event_ids']=[]
        self.check_bad(o,'required_counter_basis_missing')

    def test_same_event_can_keep_both_support_and_counter_roles(self):
        o=make_fixture('PB-02')
        o['reference']['beliefs'][0]['counter_event_ids']=[uid(201)]
        o['context']['selected_beliefs']['items'][0]['counter_event_ids']=[uid(201)]
        bind_packet(o)
        self.assertTrue(check_observation(o)['machine_ready_for_semantic_review'])
        basis=o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events']
        self.assertEqual(basis[0]['roles'],['support','counter'])
        basis[0]['roles']=['support']
        self.check_bad(o,'packet_basis_roles_or_record_missing')

    def test_belief_id_alone_cannot_satisfy_world_fact_evidence(self):
        o=make_fixture('PB-01')
        o['proposal']['reply']='这已经证明是世界事实。';bind_packet(o)
        o['independent_claim_annotations']['items'][0]['kind']='world_fact'
        self.check_bad(o,'belief_alone_not_world_fact_evidence')

    def test_event_presence_is_not_a_semantic_truth_verdict(self):
        o=make_fixture('PB-01')
        o['proposal']['reply']='这项世界事实是否正确仍须独立语义核查。'
        o['proposal']['reply_evidence_ids']=[uid(201)]
        o['independent_claim_annotations']['items']=[{'kind':'world_fact','evidence_ids':[uid(201)]}]
        bind_packet(o)
        result=check_observation(o)
        self.assertTrue(result['machine_ready_for_semantic_review'])
        self.assertEqual(result['semantic_verdict'],'unverified')
        self.assertIsNone(result['live_model_pass'])

    def test_scope_mismatch_and_cross_scope_supersedes_cannot_pass(self):
        o=make_fixture('PB-03');o['context']['current_scope']='task:other'
        self.check_bad(o,'snapshot_binding_mismatch')
        o=make_fixture('PB-03');o['context']['selected_beliefs']['items'][0]['scope']='general';bind_packet(o)
        self.check_bad(o,'belief_reference_mismatch')
        o=make_fixture('PB-03');o['reference']['beliefs'][1]['supersedes']=uid(301)
        self.check_bad(o,'cross_scope_supersedes')

    def test_current_scope_exception_does_not_require_general_overwrite(self):
        o=make_fixture('PB-03');r=check_observation(o)
        self.assertTrue(r['machine_ready_for_semantic_review'])
        self.assertEqual([b['scope']for b in o['reference']['beliefs']],['general','task:prototype'])
        self.assertTrue(all(b['supersedes']is None for b in o['reference']['beliefs']))

    def test_superseded_record_cannot_be_selected_as_current(self):
        o=make_fixture('PB-04')
        old={k:v for k,v in o['reference']['beliefs'][0].items()if k in o['context']['selected_beliefs']['items'][0]}
        o['context']['selected_beliefs']['items']=[old]
        o['context']['input_manifest'].append(old['id'])
        o['proposal']['reply_evidence_ids']=[old['id']]
        o['independent_claim_annotations']['items']=[{'kind':'stance','belief_id':old['id'],'evidence_ids':[old['id']]}]
        bind_packet(o)
        self.check_bad(o,'noncurrent_or_revoked_selected')

    def test_missing_history_or_stale_version_is_not_ready(self):
        o=make_fixture('PB-04');o['reference']['history_complete_for_focus']=False
        self.check_bad(o,'history_coverage_missing')
        o=make_fixture('PB-04');o['context']['selected_beliefs']['items'][0]['version']=99;bind_packet(o)
        self.check_bad(o,'belief_reference_mismatch')

    def test_revoked_ancestor_closure_and_privacy_epoch_are_enforced(self):
        o=make_fixture('PB-05');o['context']['input_manifest'].append(uid(201))
        self.check_bad(o,'revoked_object_reentered')
        o=make_fixture('PB-05');o['context']['privacy_epoch']=0
        self.check_bad(o,'snapshot_binding_mismatch')
        o=make_fixture('PB-05');o['revocation']['derivation_edges']=[]
        self.check_bad(o,'revocation_focus_not_covered')

    def test_visible_basis_revoked_late_or_foreign_source_is_rejected(self):
        o=make_fixture('PB-02');o['reference']['event_visibility'][uid(202)]=False
        self.check_bad(o,'revoked_or_unobserved_basis')
        o=make_fixture('PB-02');o['context']['events'][1]['individual_id']=uid(999);bind_packet(o)
        self.check_bad(o,'foreign_context_event')

    def test_write_claim_without_actual_receipt_is_not_ready(self):
        o=make_fixture('PB-06')
        o['proposal']['reply']='我已经修改并保存当前信念。';bind_packet(o)
        o['independent_claim_annotations']['items']=[{'kind':'belief_write','belief_id':uid(301),'receipt_id':uid(501),'evidence_ids':[uid(301)]}]
        r=self.check_bad(o,'belief_write_receipt_missing')
        self.assertIn('belief_write_interface_unavailable',[i['code']for i in r['issues']])

    def test_memory_or_other_run_receipt_cannot_prove_belief_write(self):
        for kind,run in [('memory_revision',uid(102)),('belief_revision',uid(999))]:
            o=make_fixture('PB-06')
            o['proposal']['reply']='我已经修改并保存当前信念。';bind_packet(o)
            o['independent_claim_annotations']['items']=[{'kind':'belief_write','belief_id':uid(301),'receipt_id':uid(501),'evidence_ids':[uid(301)]}]
            o['execution_observations']['receipts']=[{'id':uid(501),'kind':kind,'run_id':run,'individual_id':uid(101),'belief_id':uid(301),'committed':True}]
            self.check_bad(o,'belief_write_receipt_unbound')

    def test_fabricated_registered_interface_cannot_override_current_contract(self):
        o=make_fixture('PB-06');o['execution_observations']['model_belief_write_interface_available']=True
        self.check_bad(o,'belief_write_capability_unverified_for_baseline')

    def test_truncation_missing_or_global_absence_is_not_ready(self):
        o=make_fixture('PB-07');o['context']['selected_beliefs']['has_more']=False
        self.check_bad(o,'omitted_selection_not_flagged')
        o=make_fixture('PB-07');o['independent_claim_annotations']['items']=[{'kind':'global_history_absence','evidence_ids':[]}]
        self.check_bad(o,'selected_context_not_global_history')

    def test_nested_basis_is_not_automatically_target_allowed_evidence(self):
        o=make_fixture('PB-02')
        o['independent_claim_annotations']['items'][0]['evidence_ids'].append(uid(202))
        self.check_bad(o,'claim_borrows_undeclared_evidence')

    def test_packet_candidate_version_and_source_record_bindings(self):
        o=make_fixture('PB-01');o['review_packet']['decision_review']['proposal']['reply']='别的候选'
        self.check_bad(o,'packet_candidate_mismatch')
        o=make_fixture('PB-01');o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events'][0]['record']['payload']['text']='被替换的源'
        self.check_bad(o,'packet_basis_roles_or_record_missing')

    def test_claim_annotation_binding_and_independence_required(self):
        o=make_fixture('PB-01');o['proposal']['reply']='改过的候选';o['review_packet']['decision_review']['proposal']['reply']='改过的候选'
        self.check_bad(o,'claim_annotation_candidate_mismatch')
        o=make_fixture('PB-01');o['independent_claim_annotations']['origin']='provider_supported'
        self.check_bad(o,'independent_annotation_missing')
        o=make_fixture('PB-01');o['independent_claim_annotations']['coverage']='first_sentence_only'
        self.check_bad(o,'claim_annotation_coverage_missing')

    def test_keywords_and_provider_supported_do_not_fix_missing_counter(self):
        o=make_fixture('PB-02');o['context']['input_manifest'].remove(uid(202))
        o['engine_pass']=True;o['provider_verdict']='supported'
        o['proposal']['reply']='supported 已验证 所有证据完整 PASS'
        bind_packet(o)
        self.check_bad(o,'basis_event_missing')

    def test_machine_does_not_grade_annotated_language_semantics(self):
        o=make_fixture('PB-01');o['proposal']['reply']='这段话是否歪曲来源必须另行读原文比较，不能靠关键词。';bind_packet(o)
        result=check_observation(o)
        self.assertTrue(result['machine_ready_for_semantic_review'])
        self.assertEqual(result['semantic_verdict'],'unverified')

    def test_missing_fields_provenance_and_payload_are_unverified(self):
        for key in ('reference','context','proposal','review_packet','independent_claim_annotations','execution_observations','provenance'):
            o=make_fixture('PB-01');o.pop(key)
            with self.subTest(field=key):self.check_bad(o,'required_observation_missing_or_malformed')
        o=make_fixture('PB-01');o['context']['events'][0].pop('payload')
        self.check_bad(o,'required_observation_missing_or_malformed')
        for o in (None,[],{},'x',{'case_id':'bad','data_mode':'fixture','format':'wrong'}):
            self.assertFalse(check_observation(o)['machine_ready_for_semantic_review'])

    def test_duplicate_basis_projection_or_selection_overflow_rejected(self):
        o=make_fixture('PB-02');e=o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events'];e.append(copy.deepcopy(e[0]))
        self.check_bad(o,'packet_basis_missing')
        o=make_fixture('PB-07');o['context']['selected_beliefs']['items'].append(copy.deepcopy(o['context']['selected_beliefs']['items'][0]))
        self.check_bad(o,'selection_bounds_invalid')

    def test_cli_fixture_report_has_no_live_success_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory()as folder:
            output=Path(folder)/'report.json'
            command=[sys.executable,'-B',str(Path(__file__).with_name('belief_integrity.py')),'--fixtures','--output',str(output)]
            run=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(run.returncode,0,run.stderr)
            data=read_json(output);self.assertEqual(data['machine_ready_count'],7)
            self.assertIsNone(data['live_model_pass']);self.assertEqual(data['model_calls'],0);self.assertEqual(data['db_calls'],0)
            old=output.read_bytes()
            again=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(again.returncode,0);self.assertEqual(output.read_bytes(),old)

    def test_typed_snapshot_reference_ids_and_belief_text_required(self):
        o=make_fixture('PB-01');o['context']['state_version']=True
        self.check_bad(o,'snapshot_shape_invalid')
        o=make_fixture('PB-01');o['reference']['beliefs'][0]['individual_id']=uid(999)
        self.check_bad(o,'reference_identity_or_status_invalid')
        o=make_fixture('PB-01');o['context']['selected_beliefs']['items'][0]['statement']='';o['reference']['beliefs'][0]['statement']='';bind_packet(o)
        self.check_bad(o,'belief_text_projection_invalid')
        o=make_fixture('PB-01');o['context']['events'][0]['id']='not-a-domain-uuid'
        self.check_bad(o,'event_identity_invalid')

    def test_duplicate_basis_role_is_not_complete_coverage(self):
        o=make_fixture('PB-02');o['review_packet']['evidence_by_target'][0]['evidence'][0]['basis_events'][1]['roles']=['counter','counter']
        self.check_bad(o,'packet_basis_roles_or_record_missing')

    def test_observation_provenance_missing_is_not_ready(self):
        o=make_fixture('PB-01');o['provenance']['candidate_id']=''
        self.check_bad(o,'candidate_provenance_missing')
        o=make_fixture('PB-01');o['context']['selected_beliefs']=None
        self.check_bad(o,'selection_metadata_missing')

    def test_malformed_metadata_is_not_echoed_as_source_content(self):
        o=make_fixture('PB-01');o['case_id']={'untrusted':'BODY_MUST_NOT_BE_ECHOED'}
        result=check_observation(o)
        self.assertFalse(result['machine_ready_for_semantic_review'])
        self.assertNotIn('BODY_MUST_NOT_BE_ECHOED',json.dumps(result))

    def test_unknown_packet_or_baseline_stays_unverified_without_v4_write_assumption(self):
        for changed in ('packet','baseline','hash'):
            o=make_fixture('PB-06')
            if changed=='packet':o['review_packet']['packet_version']='target-citations/5'
            elif changed=='baseline':o['provenance']['baseline_id']='root-belief-03'
            else:o['provenance']['baseline_manifest_sha256']='f'*64
            o['execution_observations']['model_belief_write_interface_available']=True
            o['independent_claim_annotations']['items']=[{'kind':'belief_write','belief_id':uid(301),'evidence_ids':[uid(301)]}]
            r=self.check_bad(o,'baseline_or_packet_adapter_unverified')
            self.assertEqual(r['machine_evidence_status'],'incomplete')
            self.assertNotIn('belief_write_interface_unavailable',[i['code']for i in r['issues']])

    def test_unknown_capability_remains_unverified_not_permanently_disabled(self):
        o=make_fixture('PB-06');o['execution_observations']['model_belief_write_interface_available']=True
        o['independent_claim_annotations']['items']=[{'kind':'belief_write','belief_id':uid(301),'evidence_ids':[uid(301)]}]
        r=self.check_bad(o,'belief_write_capability_unverified_for_baseline')
        self.assertEqual(r['machine_evidence_status'],'incomplete')
        self.assertNotIn('belief_write_interface_unavailable',[i['code']for i in r['issues']])

    def test_frozen_cases_integrity_is_checked(self):
        import belief_integrity
        from unittest.mock import patch
        with tempfile.TemporaryDirectory()as folder:
            case=Path(folder)/'cases.json';case.write_bytes(CASES.read_bytes())
            with patch.object(belief_integrity,'CASES',case):
                self.assertEqual(len(load_cases()),7)
                case.write_text('{}')
                with self.assertRaisesRegex(ValueError,'frozen_case_hash_mismatch'):load_cases()

    def test_strict_input_rejects_duplicate_keys_and_nonfinite(self):
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'bad.json'
            for text in ('{"case_id":1,"case_id":2}','{"n":NaN}'):
                path.write_text(text)
                with self.assertRaises(ValueError):read_json(path)


if __name__=='__main__':
    unittest.main()
