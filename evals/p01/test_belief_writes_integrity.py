"""Free synthetic protocol/driver spies. No real DB/model/binary execution."""
import copy
from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from uuid import UUID

from belief_writes_integrity import *
ROOT=Path(__file__).resolve().parents[2]
_spec=importlib.util.spec_from_file_location('belief_trial_driver',ROOT/'scripts/test_live_beliefs.py')
driver=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(driver)
STAMP='2026-11-12T12:00:00Z'


def uid(n):return str(UUID(int=n))


def fixture_suite():
    """Build declared observations, never create database beliefs or call a model."""
    spec=load_cases();owner=uid(10);beliefs=[];inputs=[];ledger=[];belief_events=[];stages=[]
    def snapshot(version,number):
        return {'capture_id':uid(2000+number),'observed_at':STAMP,'transaction':'repeatable_read_read_only',
            'state':{'individual_id':owner,'state_version':version,'privacy_epoch':0,'config_version':1,'budget_frozen':False,
                     'limits':{'budgets':{'total_tokens_per_day':200000,'background_tokens_per_day':50000},'max_model_calls_per_cycle':3,'max_tool_calls_per_cycle':3}},
            'belief_count':len(beliefs),'input_count':len(inputs),'blocked_object_count':0,'source_count':len(inputs),
            'belief_event_count':len(belief_events),'beliefs':copy.deepcopy(beliefs),'inputs':copy.deepcopy(inputs),
            'source_events':copy.deepcopy(inputs),'belief_events':copy.deepcopy(belief_events),
            'coverage':'synthetic fixture records, not runtime capture'}
    def event(eid,kind,source,actor,payload,run):
        return {'id':eid,'individual_id':owner,'sequence':len(inputs)+len(belief_events)+1,'kind':kind,'source_kind':source,
            'actor_id':actor,'occurred_at':STAMP,'recorded_at':STAMP,'payload':payload,'correlation_id':run,'causation_id':None}
    for index,phase in enumerate(PHASES):
        before=snapshot(1+2*index,index*2);run=uid(100+index);question=uid(200+index);conversation=uid(300+index)
        author_res=uid(900+index*2);review_res=uid(901+index*2);decision=uid(700+index);review_id=uid(800+index)
        inputs.append(event(question,'message.received','reported_claim','owner',{'text':spec['stages'][index]['input'],'conversation_id':conversation},run))
        selected=[{k:copy.deepcopy(b[k])for k in BELIEF_FIELDS}for b in before['beliefs']if b['status']=='current'and b['scope']=='general']
        current='event:'+question;drafts=[]
        if phase!='restore-unknown':
            drafts=[{'statement':('初步限定理解：可丢弃原型倾向直接覆写。','新理解：通常偏向同目录替换，但承认成本和未测持久性。','当前局部原型接受中断风险，暂用直接覆写。')[index],
                'scope':current if phase=='local'else 'general','position':'tentative','brief_basis':'合成fixture依据，不是世界事实。',
                'support_event_ids':[question],'counter_event_ids':[inputs[0]['id']]if phase=='revise'else [],
                'supersedes':selected[0]['id']if phase=='revise'else None}]
        declared=([selected[0]['id']]if selected else [question])
        p={'schema_version':1,'reply':'合成候选：当前解释有范围，不能当世界事实；未知共同经历没有依据。',
            'reply_evidence_ids':declared,'memory_changes':[],'entity_changes':[],'goal_changes':[],
            'tool_intents':[],'development':None,'brief_reason':'synthetic spy, not real model'}
        if drafts:p['belief_changes']=copy.deepcopy(drafts)
        wanted={question}|set(declared)
        for b in selected:wanted.update(b['support_event_ids']+b['counter_event_ids'])
        for b in drafts:wanted.update(b['support_event_ids']+b['counter_event_ids'])
        events=[copy.deepcopy(e)for e in inputs if e['id']in wanted]
        c={'individual_id':owner,'run_id':run,'state_version':2+2*index,'privacy_epoch':0,'config_version':1,'current_scope':current,
            'current_event_id':question,'input_manifest':[owner,run]+[e['id']for e in events]+[b['id']for b in selected],
            'execution_budget':None,'context_sha256':canonical({'fixture_context':index}),
            'selected_beliefs':{'items':selected,'has_more':False}if selected else None,
            'visible_event_ids':[e['id']for e in events],'visible_memory_ids':[],'events':events,'memories':[],
            'coverage':'actual_selected_beliefs_and_target_material_only','global_history_complete':False}
        by_id={e['id']:e for e in events};selected_map={b['id']:b for b in selected};targets=[]
        material=[]
        for eid in declared:
            if eid in selected_map:
                b=selected_map[eid];basis=[]
                for source in sorted(set(b['support_event_ids']+b['counter_event_ids'])):
                    basis.append({'roles':[r for r in ('support','counter')if source in b[r+'_event_ids']],'record':copy.deepcopy(by_id[source])})
                material.append({'type':'belief','proof_scope':'individual_stance_not_world_fact','record':copy.deepcopy(b),'basis_events':basis})
            else:material.append({'type':'event','record':copy.deepcopy(by_id[eid])})
        targets.append({'target':{'kind':'reply'},'allowed_evidence_ids':declared[:],'evidence':material,'memory_under_review':None,'belief_under_review':None})
        for i,d in enumerate(drafts):
            ids=sorted(set(d['support_event_ids']+d['counter_event_ids']));comparison=None
            if d['supersedes']:
                old=selected_map[d['supersedes']]
                comparison={'record':copy.deepcopy(old),'basis_events':[copy.deepcopy(by_id[e])for e in sorted(set(old['support_event_ids']+old['counter_event_ids']))],
                    'proof_scope':'previous_stance_not_world_fact'}
            targets.append({'target':{'kind':'belief','index':i},'allowed_evidence_ids':ids,
                'evidence':[{'type':'event','record':copy.deepcopy(by_id[e])}for e in ids],
                'memory_under_review':None,'belief_under_review':{'origin':'proposed_interpretation_not_evidence','proposal':copy.deepcopy(d),
                    'previous_comparison_not_additional_citations':comparison}})
        packet={'packet_version':'target-citations/5','individual_id':owner,'run_id':run,'current_scope':current,
            'decision_review':{'author_reservation_id':author_res,'proposal':copy.deepcopy(p),'expected_targets':[t['target']for t in targets]},
            'runtime_atomic_commit_contract':{'publication_requires_successful_commit':True,'does_not_certify_tool_execution_or_goal_completion':True,
                'memory_change_indices':[],'belief_change_indices':list(range(len(drafts))),'goal_creation_indices':[]},
            'request_not_evidence':copy.deepcopy(by_id[question]),'evidence_by_target':targets}
        assessment={'schema_version':1,'checks':[{'target':t['target'],'verdict':'supported','reason':'fixture protocol opinion, not semantic acceptance','evidence_ids':t['allowed_evidence_ids'][:]}for t in targets]}
        rc=copy.deepcopy(c);rc['context_sha256']=canonical({'fixture_review_context':index})
        observations=[{'role':'author','run_id':run,'context':copy.deepcopy(c),'outcome':{'status':'parsed_candidate_not_commit_proof','proposal':copy.deepcopy(p),'proposal_sha256':canonical(p),'usage':{'input':10,'output':5,'total':15},'provider_mode':'live','provider_model':'synthetic-transport-spy'}},
            {'role':'decision_review','run_id':run,'context':rc,'packet':packet,'outcome':{'status':'returned_assessment_not_commit_proof','assessment':copy.deepcopy(assessment),'usage':{'input':10,'output':5,'total':15},'provider_mode':'live','provider_model':'synthetic-transport-spy'}}]
        new_ids=[]
        for d in drafts:
            bid=uid(500+index);new_ids.append(bid)
            if d['supersedes']:
                old=next(b for b in beliefs if b['id']==d['supersedes']);old.update(status='superseded',version=old['version']+1,superseded_at=STAMP)
            beliefs.append({'id':bid,'individual_id':owner,'version':1,'status':'current','created_at':STAMP,'superseded_at':None,**copy.deepcopy(d)})
            belief_events.append(event(uid(600+index),'belief.revised'if d['supersedes']else 'belief.formed','inference','individual',{'belief_id':bid,'supersedes':d['supersedes'],'position':d['position']},run))
        after=snapshot(3+2*index,index*2+1)
        message=uid(1000+index)
        receipt={'decision_event_id':decision,'memory_ids':[],'goal_ids':[],'action_ids':[],'message_id':message,'message_status':'delivered','state_version':after['state']['state_version'],'run_status':'committed'}
        if new_ids:receipt['belief_ids']=new_ids
        commit={'reservation_id':author_res,'individual_id':owner,'run_id':run,'lease_generation':1,'privacy_epoch':0,'proposal_digest':canonical(p),'decision_event_id':decision,'receipt':receipt,'created_at':STAMP}
        reviewed={'author_reservation_id':author_res,'review_reservation_id':review_res,'individual_id':owner,'run_id':run,'event_id':review_id,'lease_generation':1,'state_version':c['state_version'],'privacy_epoch':0,'config_version':1,'draft_digest':canonical(p),'context_digest':c['context_sha256'],'review_context_digest':rc['context_sha256'],'assessment':copy.deepcopy(assessment),'supported':True,'checker_version':spec['baseline']['review_checker_version'],'provider_mode':'live','created_at':STAMP}
        for rid in (author_res,review_res):
            ledger.append({'id':rid,'individual_id':owner,'run_id':run,'window_id':uid(50),'mode':'foreground','status':'settled','input_bound':10,'output_cap':4096,'actual_input':10,'actual_output':5,'actual_total':15,'charged_tokens':15,'usage_unknown':False,'dispatched_at':STAMP,'request_digest':canonical({'fixture_reservation':rid})})
        stages.append({'format':'alia-p01-belief-stage/1','data_mode':'fixture','phase':phase,'packet_version':'target-citations/5','cases_sha256':CASES_SHA256,
            'process_id':100+index,'process_nonce':uid(1100+index),'started_at':STAMP,'finished_at':STAMP,'worker_committed':True,'failure':None,'semantic_verdict':'unverified','live_model_pass':None,
            'before':before,'after':after,'input_accounting':'glm-pinned-text/1','submitted':{'individual_id':owner,'event_id':question,'operation_id':run,'version':c['state_version']},
            'conversation_id':conversation,'worker_outcome':'Committed('+run+')','provider_observations':observations,'commits':[commit],'reviews':[reviewed],
            'replies':[event(message,'message.sent','generated','alia',{'text':p['reply'],'evidence_ids':declared[:],'conversation_id':conversation,'decision_id':decision},run)],
            'decision_events':[event(decision,'decision.accepted','generated','alia',{'provider_model':'synthetic-transport-spy','grounding_review_id':review_id,'grounding_status':'supported'},run)],
            'run':{'id':run,'individual_id':owner,'trigger_event_id':question,'mode':'foreground','status':'committed','model_calls':2,'tool_calls':0},'diagnostics':[],
            'ledger_snapshot':{'reservation_count':len(ledger),'records':copy.deepcopy(ledger),'cumulative_do_not_sum_across_stages':True}})
    return stages


class IntegrityTests(unittest.TestCase):
    def setUp(self):self.suite=fixture_suite()
    def bad(self,stage,code=None):
        r=check_stage(stage);self.assertFalse(r['machine_inventory_ready'],r)
        if code:self.assertIn(code,r['issues'],r)
        self.assertEqual(r['semantic_verdict'],'unverified');self.assertIsNone(r['live_model_pass']);return r
    def test_all_four_fixture_inventories_are_not_runtime_or_semantic_pass(self):
        r=check_suite(self.suite);self.assertTrue(r['machine_inventory_ready'],r)
        self.assertFalse(r['runtime_execution_verified']);self.assertIsNone(r['live_model_pass'])
        self.assertTrue(all(s['semantic_verdict']=='unverified'and s['declared_data_mode']=='fixture'for s in r['stages']))
    def test_missing_reordered_or_duplicate_stages_cannot_pass(self):
        for stages in (self.suite[:-1],[],self.suite[::-1],self.suite+[self.suite[-1]]):self.assertFalse(check_suite(stages)['machine_inventory_ready'])
    def test_v4_or_unknown_versions_do_not_reuse_v4_missing_write_assumption(self):
        for version in ('target-citations/4','target-citations/6'):
            o=copy.deepcopy(self.suite[1]);o['packet_version']=version
            r=self.bad(o,'version_or_frozen_cases_unverified');self.assertNotIn('belief_write_interface_unavailable',r['issues'])
    def test_missing_actual_candidate_review_or_commit_receipt(self):
        for key in ('provider_observations','reviews','commits','replies','decision_events'):
            o=copy.deepcopy(self.suite[1]);o[key]=[];self.bad(o)
        o=copy.deepcopy(self.suite[1]);o['commits'][0]['receipt'].pop('belief_ids');self.bad(o,'actual_belief_receipt_missing_or_future_ids')
    def test_memory_or_other_run_receipt_is_not_a_belief_commit(self):
        o=copy.deepcopy(self.suite[1]);o['commits'][0]['receipt']['memory_ids']=o['commits'][0]['receipt'].pop('belief_ids');self.bad(o,'unexpected_receipt_other_writes')
        o=copy.deepcopy(self.suite[1]);o['commits'][0]['run_id']=uid(9999);self.bad(o,'persisted_receipt_snapshot_mismatch')
    def test_review_must_cover_belief_target_and_both_roles(self):
        for which in ('target','counter'):
            o=copy.deepcopy(self.suite[1]);stored=o['reviews'][0]['assessment'];observed=o['provider_observations'][1]['outcome']['assessment']
            if which=='target':stored['checks'].pop();observed['checks'].pop()
            else:stored['checks'][1]['evidence_ids']=[uid(201)];observed['checks'][1]['evidence_ids']=[uid(201)]
            self.bad(o,'review_target_coverage_mismatch'if which=='target'else 'review_omits_counter_or_support')
    def test_candidate_counter_basis_or_previous_comparison_cannot_be_stripped(self):
        for which in ('target_event','previous_event','previous_record','role'):
            o=copy.deepcopy(self.suite[1]);targets=o['provider_observations'][1]['packet']['evidence_by_target']
            if which=='target_event':targets[1]['evidence'].pop(0)
            elif which=='previous_event':targets[1]['belief_under_review']['previous_comparison_not_additional_citations']['basis_events']=[]
            elif which=='previous_record':targets[1]['belief_under_review']['previous_comparison_not_additional_citations']=None
            else:targets[0]['evidence'][0]['basis_events'][0]['roles']=[]
            self.bad(o)
    def test_selected_counter_metadata_and_source_body_are_bound(self):
        o=copy.deepcopy(self.suite[2]);o['provider_observations'][0]['context']['selected_beliefs']['items'][0]['counter_event_ids']=[];self.bad(o,'selected_stance_version_scope_mismatch')
        o=copy.deepcopy(self.suite[2]);o['provider_observations'][0]['context']['events'][0]['payload']['text']='';self.bad(o)
    def test_superseded_history_and_general_scope_stay_separate(self):
        o=copy.deepcopy(self.suite[1]);o['after']['beliefs'][0]['statement']='rewritten old statement';self.bad(o,'old_stance_content_or_version_changed')
        o=copy.deepcopy(self.suite[2]);o['after']['beliefs'][1]['status']='superseded';o['after']['beliefs'][1]['superseded_at']=STAMP;self.bad(o,'local_exception_rewrites_general_or_missing')
        o=copy.deepcopy(self.suite[2]);o['after']['beliefs'][2]['scope']='general';self.bad(o,'local_exception_rewrites_general_or_missing')
    def test_original_input_cannot_be_shortened_or_silently_replaced(self):
        for which in ('question','history'):
            o=copy.deepcopy(self.suite[1]);o['after']['inputs'][1 if which=='question'else 0]['payload']['text']='changed source'
            self.bad(o,'fixed_input_altered_or_unbound'if which=='question'else 'original_input_altered')
        o=copy.deepcopy(self.suite[1]);o['before']['inputs'][0]['payload']['text']='changed prior capture'
        r=check_stage(o,self.suite[0]);self.assertIn('cross_process_state_not_restored',r['issues'])
    def test_receipt_cannot_invent_future_id_and_packet_cannot_borrow_nested_sources(self):
        o=copy.deepcopy(self.suite[1]);o['commits'][0]['receipt']['belief_ids']=[uid(500)];self.bad(o,'actual_belief_receipt_missing_or_future_ids')
        o=copy.deepcopy(self.suite[1]);o['provider_observations'][1]['packet']['evidence_by_target'][0]['allowed_evidence_ids'].append(uid(200));self.bad(o,'target_declared_material_mismatch')
    def test_restart_restores_new_general_and_never_old_or_wrong_local_as_current(self):
        o=copy.deepcopy(self.suite[3]);o['provider_observations'][0]['context']['selected_beliefs']['items'][0]['id']=uid(500);self.bad(o,'selected_stance_version_scope_mismatch')
        o=copy.deepcopy(self.suite[3]);o['process_nonce']=self.suite[2]['process_nonce'];self.assertFalse(check_suite(self.suite[:3]+[o])['machine_inventory_ready'])
    def test_unknown_experience_is_an_independent_semantic_question_not_keywords(self):
        o=copy.deepcopy(self.suite[3]);p=o['provider_observations'][0]['outcome']['proposal'];p['reply']='虚构的错误回答也必须留给独立语义复核。'
        sha=canonical(p);o['provider_observations'][0]['outcome']['proposal_sha256']=sha;o['commits'][0]['proposal_digest']=sha;o['reviews'][0]['draft_digest']=sha
        o['provider_observations'][1]['packet']['decision_review']['proposal']=copy.deepcopy(p);o['replies'][0]['payload']['text']=p['reply']
        r=check_stage(o);self.assertTrue(r['machine_inventory_ready'],r);self.assertEqual(r['semantic_verdict'],'unverified');self.assertIsNone(r['live_model_pass'])
    def test_supported_or_keyword_labels_cannot_repair_missing_receipt(self):
        o=copy.deepcopy(self.suite[1]);o['supported']=True;o['keyword_score']=1;o['commits']=[];self.bad(o,'actual_commit_count_mismatch')
    def test_budget_unknown_usage_failure_and_duplicate_ledger_are_not_zero(self):
        original=self.suite[-1]['ledger_snapshot']
        self.assertEqual(check_ledger(original)['charged_tokens'],120)
        for change in ('unknown','pending','missing','duplicate','charge','input','overflow'):
            ledger=copy.deepcopy(original)
            if change=='unknown':ledger['records'][0]['usage_unknown']=True
            elif change=='pending':ledger['records'][0]['status']='dispatched'
            elif change=='missing':ledger['records'].pop()
            elif change=='duplicate':ledger['records'].append(copy.deepcopy(ledger['records'][0]));ledger['reservation_count']+=1
            elif change=='charge':ledger['records'][0]['charged_tokens']=0
            elif change=='input':ledger['records'][0]['actual_input']=9
            else:ledger['records'][0]['actual_output']=9000
            with self.subTest(change=change):
                r=check_ledger(ledger);self.assertFalse(r['complete']);self.assertIsNone(r['charged_tokens'])
        ledger=copy.deepcopy(original);ledger['records']+=copy.deepcopy(ledger['records'][:2]);ledger['records'][-2]['id']=uid(9998);ledger['records'][-1]['id']=uid(9999);ledger['reservation_count']+=2
        self.assertIn('per_run_three_call_limit',check_ledger(ledger)['issues'])
    def test_snapshot_limits_counts_versions_and_ownership(self):
        for field,value in (('belief_count',999),('blocked_object_count',1)):
            o=copy.deepcopy(self.suite[1]);o['after'][field]=value;self.bad(o)
        o=copy.deepcopy(self.suite[1]);o['after']['state']['limits']['budgets']['total_tokens_per_day']=200001;self.bad(o,'original_limits_changed')
        o=copy.deepcopy(self.suite[1]);o['after']['beliefs'][1]['individual_id']=uid(9999);self.bad(o,'foreign_snapshot_record')
    def test_malformed_fields_do_not_echo_private_values_or_crash(self):
        for value in (None,[],{}, {'phase':None},dict(self.suite[0],after=None)):
            r=check_stage(value);self.assertFalse(r['machine_inventory_ready']);self.assertNotIn('apikey',json.dumps(r))
        for text in ('{"x":1,"x":2}','{"x":NaN}'):
            with self.assertRaises(ValueError):strict_json(text)
    def test_missing_typed_event_fields_or_policy_binding_cannot_pass(self):
        o=copy.deepcopy(self.suite[1]);o['after']['source_events'][0].pop('occurred_at');self.bad(o,'typed_event_projection_missing')
        o=copy.deepcopy(self.suite[1]);o['reviews'][0]['checker_version']='decision_grounding/1:'+canonical('other policy');self.bad(o,'v5_review_policy_binding_mismatch')
    def test_one_event_can_fill_both_belief_roles_without_duplicate_citations(self):
        o=copy.deepcopy(self.suite[0]);p=o['provider_observations'][0]['outcome']['proposal']
        p['belief_changes'][0]['counter_event_ids']=[uid(200)];sha=canonical(p)
        o['provider_observations'][0]['outcome']['proposal_sha256']=sha;o['commits'][0]['proposal_digest']=sha;o['reviews'][0]['draft_digest']=sha
        packet=o['provider_observations'][1]['packet'];packet['decision_review']['proposal']=copy.deepcopy(p)
        packet['evidence_by_target'][1]['belief_under_review']['proposal']=copy.deepcopy(p['belief_changes'][0])
        o['after']['beliefs'][0]['counter_event_ids']=[uid(200)]
        self.assertTrue(check_stage(o)['machine_inventory_ready'],check_stage(o))
    def test_suite_malformed_containers_remain_unverified(self):
        for value in (None,{},[None],[{'process_nonce':[]}]):
            r=check_suite(value);self.assertFalse(r['machine_inventory_ready']);self.assertIsNone(r['live_model_pass'])
    def test_actual_review_request_selection_and_call_observation_are_required(self):
        o=copy.deepcopy(self.suite[1]);o['provider_observations'][1]['packet']['request_not_evidence']['payload']['text']='changed request';self.bad(o,'review_request_projection_mismatch')
        o=copy.deepcopy(self.suite[1]);o['provider_observations'][1]['context']['selected_beliefs']=None;self.bad(o,'review_author_selection_changed')
        o=copy.deepcopy(self.suite[1]);o['provider_observations'].reverse();self.bad(o,'author_review_order_mismatch')
        o=copy.deepcopy(self.suite[1]);o['run']['model_calls']=3;self.bad(o,'actual_provider_call_observation_missing')
    def test_future_receipt_ids_cannot_be_in_the_author_manifest(self):
        o=copy.deepcopy(self.suite[1]);o['provider_observations'][0]['context']['input_manifest'].append(uid(501));self.bad(o,'future_commit_objects_in_author_manifest')
    def test_n6_compatibility_keeps_all_92_pins_and_exactly_five_deltas(self):
        data=load_cases();baseline=data['baseline'];compat=baseline['compatibility_from']
        expected={
            'src/adapters.rs':('4452bad8f8ac9f25e4044bded69038af1bad50c168b22b45fdec4f4b61519c64','829779d98bf6262592df424504751df104dd09e1c27c36bc98f28bd28f08091c'),
            'src/adapters/context.rs':('86e9295ae56397a7b3f18abdf53deb851762a86f6a22507e7f400e539cb74d06','39280b2be431fefcad46860bb86fdba0d0ea5175634fe1e22aad45cb510b4cc3'),
            'src/adapters/memory_commands.rs':('307ba5c872ae59fc1a2ad88e806da86b59476153f77656732b86cfba4361cd4a','593b14905113d0be9ef36ad37ebaed3c9e6878c82816e9ddc4e91aec801d9779'),
            'src/service.rs':('c1a101e52fe39e90d4173595641aa985168701005519a1b2050240561956b965','53933355387a21611a42048da5456ff021467dbc8c67693db1e3772ba7c0fbfc'),
            'src/service/memory_commands.rs':('d41baf8462d9c16a64e658b2c7f98a305961376ea3681d01e2ece7399aaf7dc8','9ade8cde38e3fa2a7635063ad9fab8d33015a9ec24c4fcd7354250dbf27a3bfb')}
        self.assertEqual(len(baseline['core_sources']),92);self.assertEqual(compat['unchanged_core_source_count'],87)
        self.assertEqual(set(compat['changed_core_sources']),set(expected))
        for path,(old,new)in expected.items():
            self.assertEqual(compat['changed_core_sources'][path]['before_sha256'],old)
            self.assertEqual(compat['changed_core_sources'][path]['after_sha256'],new)
            self.assertEqual(baseline['core_sources'][path],new)
        self.assertEqual(compat['accepted_b02_manifest_sha256'],'7ebb986f418d3249e2da5f8359b0099df688a446c51b94b47801881383bb6ce9')
    def test_original_four_inputs_criteria_cannot_change_even_with_new_case_hash(self):
        self.assertEqual(ORIGINAL_CRITERIA_SHA256,'d416008ce4fa6263e22f539f6e84ec75be0103fc6669f8d959d894270ad911fa')
        data=load_cases();self.assertEqual(criteria_fingerprint(data),ORIGINAL_CRITERIA_SHA256)
        data['stages'][0]['input']='A shortened task is not compatibility'
        with tempfile.TemporaryDirectory()as d:
            path=Path(d)/'cases.json';path.write_text(json.dumps(data))
            with patch('belief_writes_integrity.CASES',path),patch('belief_writes_integrity.CASES_SHA256',digest(path)):
                with self.assertRaisesRegex(ValueError,'original_belief_criteria_changed'):load_cases()
    def test_compatibility_does_not_adopt_autobiography_or_change_v5_policy(self):
        b=load_cases()['baseline'];paths=b['core_sources']
        self.assertEqual(b['packet_version'],'target-citations/5');self.assertTrue(b['model_belief_write_interface_available'])
        self.assertEqual(b['review_checker_version'],'decision_grounding/1:6a1adc532e3c502d2ef5b2767f74a951a195901119f240015627ab55ee97d1c7')
        self.assertEqual(len([p for p in paths if p.startswith('migrations/')]),22)
        self.assertFalse(any('0023' in p or 'autobiography' in p for p in paths))
    def test_case_hash_mutation_is_rejected_before_trial(self):
        with tempfile.TemporaryDirectory()as d:
            path=Path(d)/'case.json';path.write_text('{}')
            with patch('belief_writes_integrity.CASES',path):
                with self.assertRaises(ValueError):load_cases()
    def test_collector_source_has_no_fixture_belief_write_or_clock_rewrite(self):
        text=(ROOT/'examples/live_belief_revisions.rs').read_text()
        for bad in ('record_belief(', 'INSERT INTO beliefs','UPDATE beliefs','DELETE FROM','setval(', 'clock_timestamp() -'):
            self.assertNotIn(bad,text)
        self.assertRegex(text,r'store\s*\.submit_message\(');self.assertRegex(text,r'worker\s*\.run_once\(')


class SpyProvisionError(RuntimeError):
    def __init__(self,path):self.state_path=path


class DatabaseSpy:
    ProvisionError=SpyProvisionError
    def __init__(self,stages,events):
        self.stages=stages;self.events=events;self.alive=False;self.executed=0
        self.fail_start=False;self.interrupt_start=False;self.fail_cleanup=False;self.fail_drop=False;self.unknown_usage=False
    def provision(self,source,role,directory):
        self.events.append('provision');self.alive=True;self.state_path=directory/'behavior.json'
        owner='a'*32;name='alia-p03-'+owner+'-behavior'
        state={'format':'alia-p03-db/1','owner':owner,'role':role,'container':name,'container_id':'fixture-container',
            'image_id':'fixture-image','volume':name+'-data','image':'fixture@sha256:'+'1'*64,
            'username':'aliaqa','password':'SYNTHETIC-PRIVATE-DB-TOKEN','port':5432,'status':'ready'}
        self._save(state)
        if self.interrupt_start:raise KeyboardInterrupt()
        if self.fail_start:raise SpyProvisionError(self.state_path)
        return self.state_path,state
    def _save(self,state):
        self.state_path.write_text(json.dumps(state));self.state_path.chmod(0o600)
    def load_state(self,path):return json.loads(Path(path).read_text())
    def assert_owned(self,state,kind):return {'Id':'fixture-'+kind}if self.alive else None
    def public(self,state):return {k:state[k]for k in ('owner','role','container','container_id','volume','image_id','port')}
    def url(self,state,database):return 'postgres://aliaqa:SYNTHETIC-PRIVATE-DB-TOKEN@127.0.0.1:5432/'+database
    def sql(self,state,statement,database):
        if statement.startswith('CREATE DATABASE '):self.events.append('create');return ''
        if statement.startswith('DROP DATABASE '):
            self.events.append('drop')
            if self.fail_drop:raise RuntimeError('private detail not to expose')
            return ''
        if statement==driver.LEDGER_SQL:
            self.events.append('ledger')
            data=copy.deepcopy(self.stages[self.executed-1]['ledger_snapshot'])if self.executed else {'reservation_count':0,'records':[]}
            if self.unknown_usage and data['records']:data['records'][0]['usage_unknown']=True
            return json.dumps(data)
        raise AssertionError('Unexpected SQL in driver spy')
    def cleanup(self,path):
        self.events.append('cleanup');state=self.load_state(path)
        if self.fail_cleanup:return {'passed':False}
        self.alive=False;state.pop('password',None);state['status']='cleaned';self._save(state);return {'passed':True}


class FrozenDriverTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.base=Path(self.temp.name)
        self.checkpoint=self.base/'checkpoint';source=self.checkpoint/'source';source.mkdir(parents=True)
        # Explicit tiny fixture candidate. Real QA freeze/artifact verification is
        # exercised; the core map below is patched only in these free spies.
        for relative in driver.OWN_SOURCES+tuple('scripts/qa/'+n+'.py'for n in ('common','freeze','artifacts','database')):
            target=source/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/relative,target)
        core=source/'src/domain/grounding.rs';core.parent.mkdir(parents=True,exist_ok=True);core.write_text('// synthetic v5 core file binding only\n')
        sources={p.relative_to(source).as_posix():digest(p)for p in source.rglob('*')if p.is_file()}
        (self.checkpoint/'manifest.json').write_text(json.dumps({'format':'alia-p03-checkpoint/1','id':'fixture-v5-observer','sources':sources}))
        self.qa=driver.load_frozen_qa(self.checkpoint)
        self.binary_dir=self.base/'artifacts/bin';self.binary_dir.mkdir(parents=True)
        for name in self.qa.artifacts.REQUIRED_BINARIES|{'examples/live_belief_revisions'}:
            target=self.binary_dir/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b'nonexecuted-free-artifact-spy\n');target.chmod(0o700)
        files={p.relative_to(self.binary_dir.parent).as_posix():digest(p)for p in self.binary_dir.rglob('*')if p.is_file()}
        (self.binary_dir.parent/'artifacts.json').write_text(json.dumps({'format':'alia-p03-artifacts/1','checkpoint_sha256':digest(self.checkpoint/'manifest.json'),'files':files,'build_commands':['fixture-no-build'],'build_environment':{'fixture':True}}))
        self.tokenizer=self.base/'tokenizer.json';self.tokenizer.write_text('fixture-no-real-tokenizer')
        self.credentials=self.base/'credentials.txt';self.credentials.write_text('opencode-go\napikey: SYNTHETIC-PRIVATE-MODEL-TOKEN\n');self.credentials.chmod(0o600)
        self.stages=fixture_suite();self.events=[];self.db=DatabaseSpy(self.stages,self.events);self.qa.database=self.db
        @contextmanager
        def lock():
            self.events.append('lock_enter')
            with open('/dev/null')as descriptor:
                try:yield descriptor.fileno(),0.0
                finally:self.events.append('lock_exit')
        self.qa.common=SimpleNamespace(heavy_lock=lock,clean_env=lambda:{'PATH':os.environ['PATH']})
        fixture_spec=load_cases();fixture_spec['baseline']['core_sources']={'src/domain/grounding.rs':digest(core)}
        fixture_spec['baseline']['tokenizer_sha256']=digest(self.tokenizer)
        self.addCleanup(patch.stopall)
        patch.object(driver._integrity,'load_cases',return_value=fixture_spec).start()
        patch.object(driver,'TOKENIZER_SHA256',digest(self.tokenizer)).start()
        self.child_count=0;self.child_mode=None
    def args(self,execute=True):
        args=['--checkpoint',str(self.checkpoint),'--binary-dir',str(self.binary_dir),'--tokenizer',str(self.tokenizer),'--output',str(self.base/'report.json')]
        if execute:args+=['--execute','--job-id','P01-BELIEF-SPY-01','--credentials',str(self.credentials),'--db-work-dir',str(self.base/'private-resource')]
        return args
    def run_child(self,command,**kwargs):
        self.assertEqual(command,[str(self.binary_dir/'examples/live_belief_revisions')]);self.assertEqual(kwargs['cwd'],self.checkpoint/'source')
        self.assertEqual(kwargs['pass_fds'],(int(kwargs['env']['ALIA_QA_LOCK_FD']),));os.fstat(kwargs['pass_fds'][0])
        self.assertEqual(kwargs['env']['ALIA_TOKENIZER_PATH'],str(self.tokenizer))
        index=self.child_count;self.child_count+=1;self.db.executed=self.child_count;self.events.append('child_'+str(index))
        value=copy.deepcopy(self.stages[index]);value['data_mode']='synthetic_inputs_real_provider'
        if self.child_mode=='timeout':raise subprocess.TimeoutExpired(command,180,output=b'private incomplete supplier wire')
        if self.child_mode=='malformed':return SimpleNamespace(returncode=0,stdout='not json private supplier wire',stderr='private')
        if self.child_mode=='nonzero':return SimpleNamespace(returncode=1,stdout=json.dumps(value),stderr='private')
        if self.child_mode=='fixture':value['data_mode']='fixture'
        if self.child_mode=='damage':(self.binary_dir/'examples/live_belief_revisions').write_bytes(b'damaged-after-fees')
        return SimpleNamespace(returncode=0,stdout=json.dumps(value),stderr='private')
    def invoke(self,execute=True):
        status=driver.main(self.args(execute),run=self.run_child,qa_loader=lambda _:self.qa)
        report=json.loads((self.base/'report.json').read_text());return status,report
    def test_file_preparation_has_no_db_provider_or_child_calls(self):
        code,r=self.invoke(False);self.assertEqual(code,0);self.assertTrue(r['preparation_ready']);self.assertFalse(r['engine_pass'])
        self.assertEqual(r['model_calls'],0);self.assertEqual(r['db_calls'],0);self.assertEqual(self.child_count,0)
        self.assertEqual(self.events,['lock_enter','lock_exit'])
    def test_four_stage_parent_spy_keeps_lock_through_one_ledger_and_cleanup(self):
        code,r=self.invoke();self.assertEqual(code,0,r);self.assertTrue(r['engine_pass']);self.assertEqual(self.child_count,4)
        self.assertIsNone(r['behavior_pass']);self.assertIsNone(r['live_model_pass']);self.assertEqual(r['semantic_verdict'],'unverified')
        self.assertEqual(r['accounting']['charged_tokens'],120) # final 8 rows, not 30+60+90+120
        self.assertEqual(self.events,['lock_enter','provision','create','child_0','child_1','child_2','child_3','ledger','drop','cleanup','lock_exit'])
        self.assertTrue(r['resource_cleanup']['password_removed']);self.assertFalse(self.db.alive)
        serialized=json.dumps(r);self.assertNotIn('SYNTHETIC-PRIVATE-',serialized)
    def test_artifact_tamper_or_missing_ninth_observer_fails_before_any_db(self):
        for mode in ('damage','unlisted'):
            with self.subTest(mode=mode):
                if mode=='damage':(self.binary_dir/'examples/live_belief_revisions').write_bytes(b'damaged')
                else:
                    manifest=json.loads((self.binary_dir.parent/'artifacts.json').read_text());manifest['files'].pop('bin/examples/live_belief_revisions');(self.binary_dir.parent/'artifacts.json').write_text(json.dumps(manifest))
                if (self.base/'report.json').exists():(self.base/'report.json').unlink()
                code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,0);self.assertNotIn('provision',self.events)
    def test_core_source_binding_mismatch_is_not_a_new_version_adapter(self):
        path=self.checkpoint/'source/src/domain/grounding.rs';path.write_text('changed core')
        # Also update checkpoint hash to prove the independent core baseline check.
        m=json.loads((self.checkpoint/'manifest.json').read_text());m['sources']['src/domain/grounding.rs']=digest(path);(self.checkpoint/'manifest.json').write_text(json.dumps(m))
        a=json.loads((self.binary_dir.parent/'artifacts.json').read_text());a['checkpoint_sha256']=digest(self.checkpoint/'manifest.json');(self.binary_dir.parent/'artifacts.json').write_text(json.dumps(a))
        code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,0);self.assertNotIn('provision',self.events)
    def test_tokenizer_damage_fails_before_resources(self):
        self.tokenizer.write_text('wrong tokenizer');code,r=self.invoke();self.assertEqual(code,1);self.assertNotIn('provision',self.events)
    def test_frozen_source_extra_or_symlink_is_refused(self):
        path=self.checkpoint/'source/extra.py';path.write_text('extra unbound file')
        code,r=self.invoke();self.assertEqual(code,1);self.assertNotIn('provision',self.events)
        path.unlink();(self.base/'report.json').unlink();binary=self.binary_dir/'examples/live_belief_revisions';other=self.base/'binary';binary.rename(other);binary.symlink_to(other)
        code,r=self.invoke();self.assertEqual(code,1);self.assertNotIn('provision',self.events)
    def test_timeout_preserves_actual_spy_fee_ledger_without_relaunch(self):
        self.child_mode='timeout';code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,1)
        self.assertEqual(r['accounting']['charged_tokens'],30);self.assertEqual(r['trial_database_cleanup']['status'],'passed');self.assertEqual(r['resource_cleanup']['status'],'passed')
        self.assertNotIn('private incomplete',json.dumps(r))
    def test_exit_zero_with_damaged_projection_does_not_pass_or_lose_fees(self):
        self.child_mode='malformed';code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,1)
        self.assertEqual(r['accounting']['charged_tokens'],30);self.assertEqual(r['resource_cleanup']['status'],'passed');self.assertNotIn('private supplier',json.dumps(r))
    def test_nonzero_or_fixture_child_does_not_restart(self):
        self.child_mode='fixture';code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,1);self.assertEqual(r['failure']['code'],'fixture_not_actual_runtime')
    def test_known_usage_does_not_hide_cleanup_failure(self):
        self.db.fail_cleanup=True;code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(r['accounting']['charged_tokens'],120)
        self.assertEqual(r['resource_cleanup']['status'],'failed');self.assertTrue(self.db.alive)
    def test_drop_failure_still_attempts_exact_outer_cleanup(self):
        self.db.fail_drop=True;code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(r['trial_database_cleanup']['status'],'failed')
        self.assertEqual(r['resource_cleanup']['status'],'passed');self.assertEqual(r['accounting']['charged_tokens'],120)
    def test_unknown_usage_remains_unknown_with_observed_charges(self):
        self.db.unknown_usage=True;code,r=self.invoke();self.assertEqual(code,1);self.assertFalse(r['accounting']['complete'])
        self.assertIsNone(r['accounting']['charged_tokens']);self.assertEqual(r['accounting']['observed_ledger_charged_tokens'],120)
    def test_partial_start_and_startup_interrupt_cleanup_exact_private_state(self):
        self.db.interrupt_start=True;code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,0)
        self.assertFalse(self.db.alive);self.assertEqual(r['resource_cleanup']['status'],'passed');self.assertIn('cleanup',self.events)
    def test_artifact_mutation_after_child_fees_does_not_erase_accounting(self):
        self.child_mode='damage';code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,1)
        self.assertEqual(r['accounting']['charged_tokens'],30);self.assertEqual(r['final_verification']['status'],'failed');self.assertFalse(self.db.alive)
    def test_output_and_private_work_dir_are_exclusive(self):
        (self.base/'private-resource').mkdir();code,r=self.invoke();self.assertEqual(code,1);self.assertNotIn('provision',self.events)
        with self.assertRaises(SystemExit):driver.main(self.args(),run=self.run_child,qa_loader=lambda _:self.qa)
    def test_missing_explicit_inputs_or_release_marker_does_not_start_anything(self):
        with self.assertRaises(SystemExit):driver.main(['--execute'],run=self.run_child,qa_loader=lambda _:self.qa)
        args=self.args();args[args.index('--job-id')+1]='invalid marker'
        with self.assertRaises(SystemExit):driver.main(args,run=self.run_child,qa_loader=lambda _:self.qa)
        self.assertEqual(self.events,[])
    def test_nonzero_child_with_complete_projection_still_fails_once(self):
        self.child_mode='nonzero';code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,1)
        self.assertEqual(r['failure']['code'],'observer_nonzero');self.assertEqual(r['accounting']['charged_tokens'],30);self.assertFalse(self.db.alive)
    def test_provision_error_preserves_partial_identity_for_cleanup(self):
        self.db.fail_start=True;code,r=self.invoke();self.assertEqual(code,1);self.assertEqual(self.child_count,0)
        self.assertEqual(r['failure']['code'],'owned_resource_startup_failed');self.assertFalse(self.db.alive);self.assertTrue(r['resource_cleanup']['password_removed'])
    def test_each_of_five_changed_core_pins_is_still_mandatory(self):
        source=self.checkpoint/'source';spec=driver._integrity.load_cases();contents={}
        for name in load_cases()['baseline']['compatibility_from']['changed_core_sources']:
            path=source/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('synthetic compatibility source '+name)
            contents[name]=path.read_text();spec['baseline']['core_sources'][name]=digest(path)
        def rebind():
            path=self.checkpoint/'manifest.json';m=json.loads(path.read_text());m['sources']=self.qa.freeze.inventory(source);path.write_text(json.dumps(m))
            path=self.binary_dir.parent/'artifacts.json';a=json.loads(path.read_text());a['checkpoint_sha256']=digest(self.checkpoint/'manifest.json');path.write_text(json.dumps(a))
        rebind()
        for name,body in contents.items():
            with self.subTest(path=name):
                path=source/name;path.write_text(body+' unauthorized change');rebind()
                code,r=self.invoke(False);self.assertEqual(code,1);self.assertEqual(r['failure']['code'],'frozen_core_binding_mismatch')
                self.assertNotIn('provision',self.events);self.assertEqual(self.child_count,0)
                (self.base/'report.json').unlink();path.write_text(body);rebind()
    def test_preparation_provenance_names_n6_and_preserves_original_lineage(self):
        code,r=self.invoke(False);self.assertEqual(code,0);p=r['provenance']
        self.assertEqual(p['baseline_assembly_id'],'individual-workspaces-v5-20261112-n6-base')
        self.assertEqual(p['baseline_assembly_sha256'],'c5e45965abaaa2aafd80ca1fec4d20656f216afdad32765a4ddb055a72b2cadc')
        self.assertEqual(p['previous_baseline_assembly_sha256'],'7104055475ecaa386020052e01c8883f111115f295455f6e62a92eae63d320fc')
        self.assertEqual(p['preserved_criteria_sha256'],ORIGINAL_CRITERIA_SHA256)
        self.assertNotIn('n5_assembly_sha256',p);self.assertEqual(self.child_count,0)
    def test_raw_projection_fields_and_ambiguous_json_are_rejected(self):
        for text in ('{"raw_wire":"never retain"}','{}\n{}','{"phase":"x","phase":"y"}','[]','{"phase":NaN}'):
            with self.assertRaises((ValueError,TypeError)):driver.parse_stage(text)
    def test_explicit_credentials_never_fall_back_to_another_provider(self):
        self.credentials.write_text('other-provider\napikey: SYNTHETIC-OTHER\n')
        code,r=self.invoke();self.assertEqual(code,1);self.assertNotIn('provision',self.events);self.assertNotIn('SYNTHETIC-OTHER',json.dumps(r))


if __name__=='__main__':unittest.main()
