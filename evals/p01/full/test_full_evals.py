"""Full-size free fixtures; no provider, DB, server or product runner is invoked."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from uuid import UUID
if __package__:
    from .check_materials import *
    from .collect_evidence import *
    from .semantic_review import *
    from .prepare_review import templates
else:
    from check_materials import *
    from collect_evidence import *
    from semantic_review import *
    from prepare_review import templates


def uid(n):return str(UUID(int=n))


def fixture():
    """Arbitrary placeholder outputs plus fake annotations test mechanics only."""
    m=load_materials();specs=unit_specs(m);capture={'format':'alia-eval-capture/1','capture_origin':'fixture','candidate_id':'fixture-candidate','records':[]};refs=[];counter=100000
    def new_id():
        nonlocal counter
        counter+=1;return uid(counter)
    def proof(kind,anchor,*,object_id=None,runs=None):
        record={'proof_type':kind,'object_id':object_id or new_id(),'payload':{'anchor_sha256':anchor,'observation_status':'observed','covered_run_ids':runs or []}}
        index=len(capture['records']);capture['records'].append(record)
        ref={'capture_sha256':None,'record_pointer':f'/records/{index}','record_sha256':canonical(record),'object_id':record['object_id']};refs.append(ref);return ref
    binding={name:canonical('fixture-'+name)if name.endswith('sha256')else 'fixture-'+name for name in m['evidence_contract_v1.json']['candidate_binding_required']}
    binding.update(candidate_id='fixture-candidate',adapter_id='fixture-normalizer/1',runtime_protocol_version='fixture-protocol/1')
    window={'timezone':'America/New_York','window_start_utc':'2026-11-12T05:00:00+00:00','window_end_utc':'2026-11-13T05:00:00+00:00'}
    prior={**window,'charged_total':0,'charged_background':0,'accounting_origin_sha256':canonical('fixture prior')}
    prior['origin_proof']=proof('project_prior_accounting',canonical(prior))
    ledger=[];units=[]
    for ident,spec in specs.items():
        runs=[]
        for _ in spec['inputs']:
            run_id=new_id();runs.append({'id':run_id,'model_calls':2,'tool_calls':0})
            for _ in range(2):ledger.append({**window,'id':new_id(),'run_id':run_id,'mode':'foreground','status':'settled','usage_unknown':False,'request_sent':True,'counting_profile':'exact','counting_profile_sha256':canonical('fixture tokenizer'),'input_bound':1,'output_cap':4096,'actual_input':1,'actual_output':1,'charged_tokens':2,'dispatched_at':'2026-11-12T13:00:00+00:00'})
        snapshot={'coverage':'case_lineage_complete','selected_logical_source_ids':[],'blocked_logical_source_ids':[],'derivation_edges':[],'material_records':[],'reference_basis':[],'selected_basis':[]}
        u={'unit_id':ident,'inputs':spec['inputs'][:],'responses':['合成占位候选，仅测试证据机械绑定，不表示正确完成任务。']*len(spec['inputs']),
            'generator_id':'fixture-generator','response_origin':'model','terminal_status':'completed','identity_before':uid(90),'identity_after':uid(90),
            'input_event_ids':[new_id()for _ in spec['inputs']],'source_snapshot':snapshot,'program_blockers':{k:False for k in BLOCKERS},'runs':runs}
        if spec['kind']=='growth':
            condition=spec['condition'];g=next(g for g in m['growth_abc_v1.json']['groups']if g['id']==spec['group_id'])
            u.update(conditions={k:'fixed-fixture-'+k for k in m['growth_abc_v1.json']['controlled_factors']},raw_training_sequence_sha256=spec['training_sequence_sha256'],post_material_sha256=spec['post_material_sha256'],long_term_retrieval_enabled=condition!='A',accepted_adaptation_enabled=condition=='C',adaptation_outcome='accepted'if condition=='C'else 'disabled')
            u['conditions'].update(candidate=binding['candidate_id'],actual_model=binding['actual_model_id'],sampling_configuration=binding['sampling_config_sha256'],initial_persona=binding['persona_sha256'],raw_training_event_sequence=spec['training_sequence_sha256'],project_budget=binding['global_budget_ledger_id'],common_probe_material=spec['post_material_sha256'],resources=canonical('fixed owned resources'))
            if condition!='A':
                for e in g['training_events']:
                    snapshot['selected_logical_source_ids'].append(e['logical_event_id']);snapshot['material_records'].append({'logical_id':e['logical_event_id'],'actual_id':new_id(),'kind':'event','content':e['text'],'content_sha256':text_sha(e['text'])})
            if condition=='C':
                target='adaptation:'+g['id'];snapshot['selected_logical_source_ids'].append(target)
                snapshot['material_records'].append({'logical_id':target,'actual_id':new_id(),'kind':'development','content':'合成试用记录，不是真实已发生的适应。','content_sha256':text_sha('合成试用记录，不是真实已发生的适应。')})
                basis={'target_id':target,'support_source_ids':spec['training_ids'][:],'counter_source_ids':spec['training_ids'][-1:],'material_source_ids':spec['training_ids'][:]}
                snapshot['reference_basis']=[basis];snapshot['selected_basis']=copy.deepcopy(snapshot['reference_basis']);snapshot['derivation_edges']=[{'parent':i,'child':target}for i in spec['training_ids']]
        anchor=response_binding(u);u['proofs']={name:proof(name,anchor,runs=[r['id']for r in runs])for name in spec['required_proofs']}
        u['independent_review']={'reviewer_id':'fixture-independent-human','reviewer_kind':'human','independent_origin_proof':proof('independent_review_origin',anchor),
            'candidate_response_binding_sha256':anchor,'criteria_sha256':canonical(spec['criteria']),
            'criteria_verdicts':[{'id':c['id'],'verdict':'pass','reason':'Explicit fixture annotation, not actual language judgment.','source_ids':u['input_event_ids'][:1]}for c in spec['criteria']],
            'blocking_findings':{k:False for k in BLOCKERS},'status':'fixture_annotation'}
        units.append(u)
    owner=uid(80);sources=[]
    profile={'model_id':'fixture-embedding','revision':'fixture-revision','dimensions':384,'index_snapshot_sha256':canonical('fixture index'),'embedding_space_sha256':canonical('fixture space')}
    corpus=m['retrieval_holdout_v1.json']['events'];events={e['logical_event_id']:new_id()for e in corpus};mems={e['logical_event_id']:new_id()for e in corpus}
    def scope(value):return 'general'if value=='general'else 'event:'+events[value[len('event:'):]]
    for e in corpus:
        row={'logical_event_id':e['logical_event_id'],'actual_event_id':events[e['logical_event_id']],
             'logical_memory_id':e['logical_memory_id'],'actual_memory_id':mems[e['logical_event_id']],
             'individual_id':owner,'scope':e['scope'],'actual_scope':scope(e['scope']),'text_sha256':e['text_sha256'],'source_kind':e['source_kind'],
             'memory_version':2,'memory_validity':e['original_memory_validity'],'valid_from':e['valid_from'],'valid_until':e['valid_until'],'embedding_space_sha256':profile['embedding_space_sha256']}
        anchor=retrieval_source_binding(row)
        row['proof']=proof('retrieval_source_event',anchor,object_id=row['actual_event_id'])
        row['memory_proof']=proof('indexed_memory_source',anchor,object_id=row['actual_memory_id']);sources.append(row)
    retrieval=[]
    for q in m['retrieval_holdout_v1.json']['queries']:
        row={'query_id':q['id'],'query_sha256':text_sha(q['query']),'scope':q['scope'],'actual_scope':scope(q['scope']),'at':q['at'],'include_history':q['include_history'],'requested_k':10,'embedding_space_sha256':profile['embedding_space_sha256'],
            'ranked_actual_memory_ids':[mems[i]for i in q['relevant_event_ids']],
            'returned_actual_event_ids':[events[i]for i in q['relevant_event_ids']],'returned_logical_event_ids':q['relevant_event_ids'][:],'claimed_current_logical_event_ids':[]}
        row['proof']=proof('actual_ranked_retrieval',retrieval_row_binding(row));retrieval.append(row)
    ledger_object={'scope':'all_project_calls_including_prior_jobs','coverage':'complete_final_inventory','global_budget_ledger_id':binding['global_budget_ledger_id'],'reservation_count':len(ledger),'records':ledger,'prior_project_window_charges':[prior]}
    ledger_object['final_inventory_proof']=proof('final_project_ledger',ledger_binding(ledger_object))
    blob=json.dumps(capture,ensure_ascii=False,indent=2).encode();capture_sha=hashlib.sha256(blob).hexdigest()
    for ref in refs:ref['capture_sha256']=capture_sha
    bundle={'format':'alia-eval-results/1','data_mode':'fixture','material_manifest_sha256':MANIFEST_SHA256,'binding':binding,'units':units,
        'retrieval':retrieval,'retrieval_source_map':sources,'retrieval_individual_id':owner,'retrieval_profile':profile,
        'ledger':ledger_object,
        'capture_files':[{'path':'capture.json','sha256':capture_sha}]}
    return bundle,{capture_sha:capture},blob


def reauthor_fixture(bundle,old_captures):
    """Generate a new explicitly synthetic capture after intentional mutations."""
    capture=copy.deepcopy(next(iter(old_captures.values())))
    for u in bundle['units']:
        anchor=response_binding(u)
        u['independent_review']['candidate_response_binding_sha256']=anchor
        for ref in list(u['proofs'].values())+[u['independent_review']['independent_origin_proof']]:
            pointer(capture,ref['record_pointer'])['payload']['anchor_sha256']=anchor
    raw=json.dumps(capture,ensure_ascii=False,indent=2).encode();new_sha=hashlib.sha256(raw).hexdigest()
    def refs(obj):
        if isinstance(obj,dict):
            if {'capture_sha256','record_pointer','record_sha256'}<=set(obj):
                obj['capture_sha256']=new_sha;obj['record_sha256']=canonical(pointer(capture,obj['record_pointer']))
            else:
                for value in obj.values():refs(value)
        elif isinstance(obj,list):
            for value in obj:refs(value)
    refs(bundle);bundle['capture_files'][0]['sha256']=new_sha
    return {new_sha:capture}


class FullMaterialsTests(unittest.TestCase):
    def test_all_complete_material_counts_and_original_gates(self):
        r=validate_materials(load_materials());self.assertTrue(r['material_inventory_ready'],r)
        self.assertEqual(r['counts'],{'holdout_queries':100,'dev_queries':20,'supplemental_queries':100,'primary_events':80,'primary_memories':80,'behavior_cases':60,'behavior_planned_runs':180,'fabrication_inductions':20,'growth_groups':20,'growth_planned_conditions':60,'negative_controls':24})
        self.assertEqual(r['actual_runs'],0);self.assertIsNone(r['formal_acceptance'])
    def test_shortened_sets_or_missing_gold_source_cannot_be_ready(self):
        for name,key in [('retrieval_holdout_v1.json','queries'),('behaviour_zh_v1.json','cases'),('growth_abc_v1.json','groups')]:
            data=load_materials();data[name][key].pop();self.assertFalse(validate_materials(data)['material_inventory_ready'])
        data=load_materials();data['retrieval_holdout_v1.json']['queries'][0]['relevant_event_ids']=[uid(9999999)];self.assertIn('gold_event_binding_missing',validate_materials(data)['issues'])
    def test_heldout_and_dev_cannot_overlap_or_tune_holdout(self):
        data=load_materials();data['manifest']['holdout_tuning_allowed']=True;self.assertIn('holdout_or_execution_claim',validate_materials(data)['issues'])
        data=load_materials();data['retrieval_dev_v1.json']['events'][0]['logical_event_id']=data['retrieval_holdout_v1.json']['events'][0]['logical_event_id'];self.assertIn('dev_holdout_source_overlap',validate_materials(data)['issues'])
    def test_threshold_and_budget_weakening_are_rejected(self):
        data=load_materials();data['evidence_contract_v1.json']['budget']['total_tokens_per_project_calendar_day']=200001;self.assertIn('original_budgets_changed',validate_materials(data)['issues'])
        data=load_materials();data['growth_abc_v1.json']['minimum_C_groups']=15;self.assertIn('growth_threshold_changed',validate_materials(data)['issues'])
    def test_material_hash_duplicate_json_and_unknown_files_fail_closed(self):
        with tempfile.TemporaryDirectory()as d:
            p=Path(d);(p/'manifest_v1.json').write_text('{}')
            with self.assertRaises(ValueError):load_materials(p)
        for value in ('{"x":1,"x":2}','{"x":NaN}'):
            with self.assertRaises(ValueError):strict_json(value)
    def test_generator_plan_contains_inputs_not_gold_or_self_grading(self):
        plan=export_runtime_plan(load_materials());text=json.dumps(plan)
        for field in ('relevant_event_ids','fact_probe_expected','preannotated_reasonable_adjustment','criteria_verdicts','pass_rate_threshold'):
            self.assertNotIn('"'+field+'"',text)
        self.assertEqual(len(plan['behavior_repetitions']),180);self.assertEqual(len(plan['growth_conditions']),60)


class EvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.original,cls.captures,cls.capture_bytes=fixture();cls.materials=load_materials()
    def bundle(self):return copy.deepcopy(self.original)
    def evaluate(self,b):return evaluate_bundle(b,self.captures,self.materials)
    def test_full_fixture_metrics_never_become_live_acceptance(self):
        r=self.evaluate(self.bundle());self.assertTrue(r['machine_inventory_ready'],r.get('issues'));self.assertTrue(r['thresholds_satisfied_on_declared_evidence'],r)
        self.assertEqual(r['retrieval']['macro_recall_at_10'],1);self.assertEqual(r['behavior']['denominator'],180);self.assertEqual(r['growth']['C_reasonable_adjustment_groups'],20)
        self.assertIsNone(r['formal_acceptance']);self.assertFalse(r['qualification_verified']);self.assertEqual(r['semantic_verdict'],'unverified')
        self.assertEqual(r['actual_new_model_calls_by_collector'],0)
    def test_partial_runs_do_not_change_denominators(self):
        b=self.bundle();b['units'].pop();r=self.evaluate(b);self.assertFalse(r['machine_inventory_ready']);self.assertIn('complete_180_60_24_unit_inventory_required',r['issues'])
        b=self.bundle();b['retrieval'].pop();r=self.evaluate(b);self.assertEqual(r['retrieval']['denominator'],100);self.assertIsNone(r['retrieval']['macro_recall_at_10'])
    def test_missing_actual_receipt_or_independent_review_not_supported_shortcut(self):
        for field in ('proofs','independent_review'):
            b=self.bundle();b['units'][0][field]={};b['units'][0]['provider_self_rating']='supported';r=self.evaluate(b)
            self.assertFalse(r['machine_inventory_ready']);self.assertFalse(r['thresholds_satisfied_on_declared_evidence'])
    def test_self_reviewer_wrong_candidate_or_criteria_are_rejected(self):
        for kind in ('self','candidate','criteria'):
            b=self.bundle();u=b['units'][0]
            if kind=='self':u['independent_review']['reviewer_id']=u['generator_id']
            elif kind=='candidate':u['independent_review']['candidate_response_binding_sha256']=canonical('other')
            else:u['independent_review']['criteria_verdicts'].pop()
            r=self.evaluate(b);self.assertFalse(r['machine_inventory_ready'])
    def test_missing_counter_role_or_material_cannot_be_inventory_complete(self):
        b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='G-01-C');u['source_snapshot']['selected_basis'][0]['counter_source_ids']=[]
        r=self.evaluate(b);self.assertIn('support_counter_or_reference_material_missing',r['units']['G-01-C']['issues'])
        b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='G-01-C');u['source_snapshot']['material_records'][0]['content']='stripped'
        r=self.evaluate(b);self.assertIn('fixed_case_source_text_altered',r['units']['G-01-C']['issues'])
    def test_blocking_findings_cannot_be_averaged_away(self):
        b=self.bundle();u=b['units'][0];u['independent_review']['blocking_findings']['fabricated_shared_experience']=True
        r=self.evaluate(b);self.assertTrue(r['machine_inventory_ready']);self.assertGreaterEqual(r['behavior']['pass_rate'],0.9)
        self.assertTrue(r['blocking_findings']['fabricated_shared_experience']);self.assertFalse(r['thresholds_satisfied_on_declared_evidence'])
    def test_every_negative_control_is_required_even_at_high_rate(self):
        b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='NC-01');u['independent_review']['criteria_verdicts'][0]['verdict']='fail'
        r=self.evaluate(b);self.assertEqual(r['behavior']['pass_rate'],1);self.assertFalse(r['negative_controls']['all_blocked_safely_on_declared_evidence']);self.assertFalse(r['thresholds_satisfied_on_declared_evidence'])
    def test_training_and_transitive_derivative_cannot_leak_into_A(self):
        b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='G-01-A');train=unit_specs(self.materials)['G-01-A']['training_ids'][0]
        u['source_snapshot']['derivation_edges']=[{'parent':train,'child':'summary-1'},{'parent':'summary-1','child':'autobiography-2'}]
        u['source_snapshot']['selected_logical_source_ids']=['autobiography-2']
        r=self.evaluate(b);self.assertTrue(any('training_or_derivative_leaked_into_A'in e for e in r['growth']['control_errors']))
    def test_C_facts_permissions_and_unrelated_control_do_not_drop(self):
        for target in ('facts','permissions','unrelated'):
            b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='G-01-C')
            next(c for c in u['independent_review']['criteria_verdicts']if c['id']=='G-01-'+target)['verdict']='fail'
            r=self.evaluate(b);self.assertFalse(next(g for g in r['growth']['groups']if g['group_id']=='G-01')['C_reasonable_adjustment'])
            if target!='unrelated':self.assertFalse(r['growth']['gate_on_declared_evidence'])
    def test_growth_requires_16_actual_accepted_adjustments_all_20_reported(self):
        b=self.bundle()
        for u in b['units']:
            if u['unit_id']in {f'G-{n:02}-C'for n in range(1,6)}:u['adaptation_outcome']='rejected'
        captures=reauthor_fixture(b,self.captures);r=evaluate_bundle(b,captures,self.materials);self.assertTrue(r['machine_inventory_ready'],r['issues']);self.assertTrue(r['growth']['complete']);self.assertEqual(r['growth']['C_reasonable_adjustment_groups'],15);self.assertEqual(len(r['growth']['groups']),20);self.assertFalse(r['growth']['gate_on_declared_evidence'])
    def test_conditions_same_history_model_resources_and_original_inputs(self):
        b=self.bundle();u=next(u for u in b['units']if u['unit_id']=='G-01-B');u['conditions']['actual_model']='other model'
        r=self.evaluate(b);self.assertTrue(any('A_B_C_factors_differ'in e for e in r['growth']['control_errors']))
        b=self.bundle();b['units'][0]['inputs'][0]='shortened task';r=self.evaluate(b);self.assertIn('actual_full_inputs_or_terminal_outcome_missing',r['units']['B-001-R1']['issues'])
    def test_unknown_runtime_adapter_is_unverified_not_permanently_v5_pinned(self):
        b=self.bundle();b['data_mode']='observed_synthetic';b['binding']['adapter_id']='future-m5-v6';b['binding']['runtime_protocol_version']='target-citations/6'
        r=self.evaluate(b);self.assertIn('final_M5_runtime_adapter_unverified',r['issues']);self.assertIsNone(r['formal_acceptance'])
        self.assertNotIn('target-citations/5',(BASE/'collect_evidence.py').read_text())
    def test_retrieval_id_metrics_not_keyword_or_self_rating(self):
        rows=copy.deepcopy(self.original['retrieval']);rows[0]['returned_logical_event_ids']=[];rows[0]['claimed_current_logical_event_ids']=[]
        r=retrieval_metrics(self.materials,rows);self.assertEqual(r['macro_recall_at_10'],0.99)
        rows=copy.deepcopy(self.original['retrieval']);rows[0]['returned_logical_event_ids']=rows[0]['returned_logical_event_ids']*2
        self.assertFalse(retrieval_metrics(self.materials,rows)['complete'])
    def test_scope_obsolete_wrong_owner_or_source_binding_cannot_hide_behind_recall(self):
        b=self.bundle();q=self.materials['retrieval_holdout_v1.json']['queries'][0];b['retrieval'][0]['returned_logical_event_ids']+=q['forbidden_event_ids'][:1];b['retrieval'][0]['claimed_current_logical_event_ids']=q['forbidden_event_ids'][:1]
        r=self.evaluate(b);self.assertTrue(r['retrieval']['source_time_scope_errors']);self.assertFalse(r['thresholds_satisfied_on_declared_evidence'])
        b=self.bundle();b['retrieval_source_map'][0]['individual_id']=uid(8888);self.assertIn('retrieval_source_content_or_receipt_mismatch',self.evaluate(b)['issues'])
        b=self.bundle();b['retrieval'][0]['embedding_space_sha256']=canonical('different index');self.assertIn('embedding_spaces_mixed',self.evaluate(b)['issues'])
    def test_fee_inventory_unknown_duplicates_and_cross_instance_reset(self):
        ledger=copy.deepcopy(self.original['ledger']);self.assertTrue(ledger_summary(ledger)['complete'])
        ledger['records'][0]['usage_unknown']=True;self.assertIsNone(ledger_summary(ledger)['charged_tokens'])
        ledger=copy.deepcopy(self.original['ledger']);ledger['records'].append(copy.deepcopy(ledger['records'][0]));ledger['reservation_count']+=1
        self.assertIn('ledger_duplicate_or_invalid_reservations',ledger_summary(ledger)['issues'])
        ledger=copy.deepcopy(self.original['ledger']);ledger['records']=ledger['records'][:2];ledger['reservation_count']=2
        for r in ledger['records']:r.update(run_id=uid(999),actual_input=149999,input_bound=149999,actual_output=1,charged_tokens=150000)
        self.assertIn('original_shared_daily_budget_exceeded',ledger_summary(ledger)['issues'])
    def test_three_call_limit_background_and_prior_charges(self):
        ledger=copy.deepcopy(self.original['ledger']);rows=ledger['records'][:4]
        for r in rows:r['run_id']=uid(999)
        ledger['records']=rows;ledger['reservation_count']=4;self.assertIn('three_call_limit_exceeded',ledger_summary(ledger)['issues'])
        ledger=copy.deepcopy(self.original['ledger']);ledger['prior_project_window_charges']=[]
        self.assertIn('prior_project_charges_missing_not_assumed_zero',ledger_summary(ledger)['issues'])
    def test_blinded_review_templates_never_prejudge_or_expose_condition(self):
        specs=unit_specs(self.materials);u=next(u for u in self.original['units']if u['unit_id']=='G-01-C')
        t=review_template(specs[u['unit_id']],u,salt='explicit-fixture-salt-only');self.assertNotIn('unit_id',t);self.assertNotIn('condition',t)
        self.assertTrue(all(c['verdict']is None for c in t['criteria_verdicts']));self.assertTrue(all(v is None for v in t['blocking_findings'].values()))
    def test_capture_corruption_symlink_and_unknown_file_type(self):
        with tempfile.TemporaryDirectory()as d:
            root=Path(d);(root/'capture.json').write_bytes(self.capture_bytes);self.assertTrue(load_captures(self.original['capture_files'],root))
            (root/'capture.json').write_text('{}')
            with self.assertRaises(ValueError):load_captures(self.original['capture_files'],root)
            (root/'capture.json').unlink();(root/'other.json').write_bytes(self.capture_bytes);(root/'capture.json').symlink_to(root/'other.json')
            with self.assertRaises(ValueError):load_captures(self.original['capture_files'],root)
        with self.assertRaises(ValueError):load_captures([{'path':'../outside.json','sha256':'0'*64}],Path('/tmp'))
    def test_cli_fixture_roundtrip_is_read_only_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory()as d:
            root=Path(d);(root/'capture.json').write_bytes(self.capture_bytes);(root/'bundle.json').write_text(json.dumps(self.original))
            cmd=[sys.executable,str(BASE/'collect_evidence.py'),'--input',str(root/'bundle.json'),'--evidence-root',str(root),'--output',str(root/'report.json')]
            result=subprocess.run(cmd,capture_output=True,text=True);self.assertEqual(result.returncode,0,result.stderr)
            r=read_json(root/'report.json');self.assertIsNone(r['formal_acceptance']);self.assertEqual(r['data_mode'],'fixture')
            self.assertNotEqual(subprocess.run(cmd,capture_output=True,text=True).returncode,0)
    def test_behavior_exact_90_percent_boundary_and_fixed_denominator(self):
        for failures,expected in ((18,True),(19,False)):
            b=self.bundle()
            for u in b['units'][:failures]:u['independent_review']['criteria_verdicts'][0]['verdict']='fail'
            r=self.evaluate(b);self.assertTrue(r['machine_inventory_ready']);self.assertEqual(r['behavior']['denominator'],180)
            self.assertEqual(r['behavior']['passed_runs'],180-failures);self.assertEqual(r['behavior']['threshold_met'],expected)
    def test_retrieval_exact_90_percent_boundary(self):
        for misses,expected in ((10,True),(11,False)):
            rows=copy.deepcopy(self.original['retrieval'])
            for row in rows[:misses]:row['returned_logical_event_ids']=[];row['claimed_current_logical_event_ids']=[]
            r=retrieval_metrics(self.materials,rows);self.assertEqual(r['recall_threshold_met'],expected);self.assertEqual(r['denominator'],100)
    def test_16_growth_adjustments_can_pass_without_erasing_four_failures(self):
        b=self.bundle()
        for u in b['units']:
            if u['unit_id']in {f'G-{n:02}-C'for n in range(1,5)}:u['adaptation_outcome']='rejected'
        r=evaluate_bundle(b,reauthor_fixture(b,self.captures),self.materials);self.assertTrue(r['machine_inventory_ready'],r['issues'])
        self.assertEqual(r['growth']['C_reasonable_adjustment_groups'],16);self.assertEqual(len(r['growth']['groups']),20);self.assertTrue(r['growth']['gate_on_declared_evidence'])
    def test_calendar_windows_follow_real_DST_and_empty_prior_is_unknown(self):
        base=copy.deepcopy(self.original['ledger']['prior_project_window_charges'][0])
        for start,end in [('2026-03-08T05:00:00+00:00','2026-03-09T04:00:00+00:00'),('2026-11-01T04:00:00+00:00','2026-11-02T05:00:00+00:00')]:
            prior=dict(base,window_start_utc=start,window_end_utc=end)
            r=ledger_summary({'scope':'all_project_calls_including_prior_jobs','coverage':'complete_final_inventory','reservation_count':0,'records':[],'prior_project_window_charges':[prior]});self.assertTrue(r['complete'],r)
        r=ledger_summary({'scope':'all_project_calls_including_prior_jobs','coverage':'complete_final_inventory','reservation_count':0,'records':[],'prior_project_window_charges':[]});self.assertIsNone(r['charged_tokens'])
    def test_generator_payload_keeps_gold_and_A_training_out(self):
        specs=unit_specs(self.materials);u=next(u for u in self.original['units']if u['unit_id']=='G-01-A')
        payload=generator_payload(specs['G-01-A'],0,u['source_snapshot']);self.assertEqual(set(payload),{'current_input','selected_material','selected_basis'})
        snap=copy.deepcopy(u['source_snapshot']);snap['selected_logical_source_ids']=specs['G-01-A']['training_ids'][:1]
        with self.assertRaisesRegex(ValueError,'training_leak_into_A'):generator_payload(specs['G-01-A'],0,snap)
    def test_unrun_independent_review_export_has_264_runtime_plus_100_label_units(self):
        public,private=templates(self.materials,[],salt='explicit-fixture-salt-only')
        self.assertEqual(len(public['runtime_review_units']),264);self.assertEqual(len(public['retrieval_label_review_units']),100);self.assertEqual(len(private['map']),264)
        self.assertTrue(all(u['responses']is None for u in public['runtime_review_units']));self.assertIsNone(public['formal_acceptance'])
    def test_independent_review_cannot_cite_unobserved_or_missing_sources(self):
        for sources in ([],[uid(99999999)]):
            b=self.bundle();b['units'][0]['independent_review']['criteria_verdicts'][0]['source_ids']=sources
            r=self.evaluate(b);self.assertIn('independent_review_source_ids_unbound',r['units']['B-001-R1']['issues'])
    def test_forbidden_raw_fields_never_enter_collection_or_review_export(self):
        b=self.bundle();b['units'][0]['source_snapshot']['hidden_reasoning']='SYNTHETIC-DO-NOT-ECHO'
        r=self.evaluate(b);self.assertFalse(r['machine_inventory_ready']);self.assertNotIn('SYNTHETIC-DO-NOT-ECHO',json.dumps(r))
        with self.assertRaises(ValueError):templates(self.materials,b['units'],salt='explicit-fixture-salt-only')
    def test_real_receipt_object_reference_is_usable_but_mismatched_id_is_not(self):
        b=self.bundle();u=b['units'][0];ref=next(iter(u['proofs'].values()))
        u['independent_review']['criteria_verdicts'][0]['source_ids']=[ref['object_id']]
        self.assertTrue(self.evaluate(b)['machine_inventory_ready'])
        ref['object_id']=uid(999999);self.assertFalse(self.evaluate(b)['machine_inventory_ready'])
    def test_final_fee_capture_cannot_be_missing_or_postdated_to_reset_budget(self):
        b=self.bundle();b['ledger'].pop('final_inventory_proof');self.assertIn('final_ledger_capture_missing_or_changed',self.evaluate(b)['issues'])
        ledger=copy.deepcopy(self.original['ledger']);ledger['records'][0]['dispatched_at']='2026-11-14T13:00:00+00:00'
        self.assertIn('dispatch_outside_claimed_budget_window',ledger_summary(ledger)['issues'])
    def test_original_memory_event_bijection_and_primary_labels_are_preserved(self):
        material=self.materials['retrieval_holdout_v1.json'];raw=self.materials['retrieval_original_v1.json']
        memory_to_event={m['id']:m['source_event_ids'][0]for m in raw['memories']}
        self.assertEqual(material['original_dataset_sha256'],ORIGINAL_RETRIEVAL_SHA256)
        for q,o in zip(material['queries'],raw['queries']):
            self.assertEqual(q['original_query'],o)
            self.assertEqual(q['relevant_event_ids'],[memory_to_event[i]for i in o['relevant_ids']])
            self.assertEqual(q['forbidden_event_ids'],[memory_to_event[i]for i in o['forbidden_ids']])
    def test_indexed_memory_and_actual_scope_or_query_time_cannot_be_relabelled(self):
        for field,value in [('actual_scope','event:'+uid(99999)),('at','2026-12-01T00:00:00Z'),('include_history',True)]:
            b=self.bundle();b['retrieval'][0][field]=value
            self.assertIn('actual_retrieval_query_or_scope_changed',self.evaluate(b)['issues'])
        b=self.bundle();b['retrieval'][0]['ranked_actual_memory_ids']=[uid(99999)]
        self.assertIn('top10_memory_source_mapping_mismatch',self.evaluate(b)['issues'])
        b=self.bundle();b['retrieval_source_map'][0]['memory_proof']=None
        self.assertIn('retrieval_source_content_or_receipt_mismatch',self.evaluate(b)['issues'])
    def test_uncertain_fact_or_permission_scores_cannot_prove_no_regression(self):
        b=self.bundle()
        for u in b['units']:
            if u['unit_id'].startswith('G-01-'):
                next(v for v in u['independent_review']['criteria_verdicts']if v['id']=='G-01-facts')['verdict']='uncertain'
        r=self.evaluate(b);self.assertIn('G-01:fact_permission_comparison_unverified',r['growth']['control_errors']);self.assertFalse(r['growth']['gate_on_declared_evidence'])
    def test_malformed_observations_are_unverified_without_raw_echo(self):
        for obj in ({},dict(self.original,units=None),dict(self.original,ledger=None)):
            r=self.evaluate(obj);self.assertFalse(r['machine_inventory_ready']);self.assertIsNone(r['formal_acceptance'])

if __name__=='__main__':unittest.main()
