#!/usr/bin/env python3
"""v5 evidence inventory only. Neither fixtures nor supported reviews prove semantics."""
import argparse
from datetime import datetime
import hashlib
import json
from collections import Counter, defaultdict
from pathlib import Path
from uuid import UUID

CASES = Path(__file__).with_name('belief_writes_cases_v1.json')
CASES_SHA256 = '0ad7bcfc4d771d4523993dc01cc54149217aca5a0c77dfe3a174cb6f02701fdc'
ORIGINAL_CRITERIA_SHA256 = 'd416008ce4fa6263e22f539f6e84ec75be0103fc6669f8d959d894270ad911fa'
PHASES = ('form', 'revise', 'local', 'restore-unknown')
BELIEF_FIELDS = ('id','version','statement','scope','position','brief_basis','support_event_ids','counter_event_ids')
DRAFT_FIELDS = ('statement','scope','position','brief_basis','support_event_ids','counter_event_ids','supersedes')


def digest(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    return h.hexdigest()


def canonical(value):
    return hashlib.sha256(json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':'),allow_nan=False).encode()).hexdigest()


def strict_json(text):
    def pairs(items):
        result={}
        for key,value in items:
            if key in result:raise ValueError('duplicate_json_key')
            result[key]=value
        return result
    return json.loads(text,object_pairs_hook=pairs,parse_constant=lambda _:(_ for _ in ()).throw(ValueError('nonfinite_json')))


def read_json(path):
    data=Path(path).read_bytes()
    if len(data)>8*1024*1024:raise ValueError('observation_size_limit')
    return strict_json(data)


def criteria_fingerprint(data):
    return canonical({k:v for k,v in data.items()if k!='baseline'})


def load_cases():
    if digest(CASES)!=CASES_SHA256:raise ValueError('frozen_case_hash_mismatch')
    data=read_json(CASES)
    if criteria_fingerprint(data)!=ORIGINAL_CRITERIA_SHA256 or data['baseline']['compatibility_from']['preserved_criteria_sha256']!=ORIGINAL_CRITERIA_SHA256:
        raise ValueError('original_belief_criteria_changed')
    if tuple(s['id']for s in data['stages'])!=PHASES:raise ValueError('fixed_phase_order_changed')
    return data


def valid_id(value):
    try:return isinstance(value,str) and UUID(value).int!=0 and str(UUID(value))==value
    except (ValueError,AttributeError):return False


def same_ids(a,b):
    return isinstance(a,list) and isinstance(b,list) and len(a)==len(set(a)) and len(b)==len(set(b)) and set(a)==set(b)


def event_shape(e):
    fields={'individual_id','id','sequence','kind','source_kind','actor_id','occurred_at','recorded_at','payload','correlation_id','causation_id'}
    try:
        if set(e)!=fields or not all(valid_id(e[k])for k in ('individual_id','id','correlation_id')) or not (e['causation_id']is None or valid_id(e['causation_id'])):return False
        if type(e['sequence'])is not int or e['sequence']<1 or not isinstance(e['kind'],str) or not e['kind'] or not isinstance(e['actor_id'],str) or not e['actor_id'] or not isinstance(e['payload'],dict):return False
        if e['source_kind']not in ('direct_observation','reported_claim','external_document','inference','generated','fictional_background'):return False
        return all(isinstance(e[k],str) and datetime.fromisoformat(e[k]).tzinfo is not None for k in ('occurred_at','recorded_at'))
    except (KeyError,TypeError,ValueError,AttributeError):return False


def check_ledger(ledger, expected_runs=None):
    """One final reservation inventory, never a sum of cumulative stage snapshots."""
    issues=[];rows=[];count=None;observed=None;charged=None
    try:
        rows=ledger['records'];count=ledger['reservation_count']
        if type(count)is not int or count!=len(rows) or not isinstance(rows,list) or len(rows)>32:issues.append('ledger_truncated_or_malformed')
        ids=[r['id']for r in rows]
        if len(set(ids))!=len(ids) or not all(valid_id(i)for i in ids):issues.append('ledger_duplicate_or_invalid_id')
        runs=Counter(r['run_id']for r in rows)
        if any(n>3 for n in runs.values()):issues.append('per_run_three_call_limit')
        if expected_runs is not None and (set(runs)!=set(expected_runs) or not rows):issues.append('ledger_run_coverage')
        windows=defaultdict(int);background=defaultdict(int)
        for r in rows:
            if not valid_id(r['run_id']) or not valid_id(r['individual_id']) or not valid_id(r['window_id']):issues.append('ledger_identity_missing')
            if r['status']!='settled' or r['usage_unknown'] is not False or not r['dispatched_at']:issues.append('ledger_usage_not_known_settled')
            for name in ('actual_input','actual_output','actual_total','charged_tokens','input_bound','output_cap'):
                if type(r[name])is not int or r[name]<0:issues.append('ledger_numeric_usage_missing')
            if all(type(r[k])is int for k in ('actual_input','actual_output','actual_total','charged_tokens','input_bound','output_cap')):
                if r['input_bound']<1 or not 1<=r['output_cap']<=8192 or r['actual_input']!=r['input_bound'] or r['actual_output']>r['output_cap']:issues.append('ledger_input_count_or_output_bound')
                if r['actual_total']!=r['actual_input']+r['actual_output'] or r['charged_tokens']!=r['actual_total']:issues.append('ledger_charge_mismatch')
                windows[r['window_id']]+=r['charged_tokens']
                if r['mode']=='background':background[r['window_id']]+=r['charged_tokens']
            if r['mode']!='foreground':issues.append('unexpected_background_or_unknown_mode')
        if any(n>200000 for n in windows.values()) or any(n>50000 for n in background.values()):issues.append('original_daily_budget_exceeded')
        if not any(code in issues for code in ('ledger_duplicate_or_invalid_id','ledger_truncated_or_malformed')):
            observed=sum(r['charged_tokens']for r in rows if type(r['charged_tokens'])is int and r['charged_tokens']>=0)
        if not issues:charged=sum(r['charged_tokens']for r in rows)
    except (KeyError,TypeError,ValueError,AttributeError):issues.append('ledger_required_fields_missing')
    return {'complete':not issues,'reservation_count':count,'charged_tokens':charged,
            'observed_ledger_charged_tokens':observed,'monetary_cost':None,'issues':sorted(set(issues)),
            'accounting_scope':'one final unique reservation inventory; cumulative snapshots not added'}


def check_stage(o, previous=None):
    result={'phase':None,'machine_inventory_ready':False,'integrity_status':'incomplete',
            'semantic_verdict':'unverified','live_model_pass':None,'provider_supported_is_semantic_pass':False,
            'runtime_execution_verified':False,'issues':[],'focus_belief_id':None,'criteria_sha256':digest(CASES)}
    def check(ok,code):
        if not ok and code not in result['issues']:result['issues'].append(code)
    try:
        spec=load_cases();phase=o['phase'];result['phase']=phase
        if phase not in PHASES or o['format']!='alia-p01-belief-stage/1':
            check(False,'unsupported_stage_format');return result
        if o['packet_version']!=spec['baseline']['packet_version'] or o['cases_sha256']!=CASES_SHA256:
            check(False,'version_or_frozen_cases_unverified');return result
        check(o['data_mode']in ('fixture','synthetic_inputs_real_provider'),'data_mode_unverified')
        result['declared_data_mode']=o['data_mode']
        result['provenance_limit']='Input declarations and internal consistency alone do not prove a trusted runtime capture.'
        index=PHASES.index(phase);before=o['before'];after=o['after'];submitted=o['submitted']
        owner=before['state']['individual_id'];run=submitted['operation_id'];event_id=submitted['event_id']
        check(all(valid_id(i)for i in (owner,run,event_id,o['conversation_id'],o['process_nonce'])),'stage_identity_invalid')
        check(type(o['process_id'])is int and o['process_id']>0,'process_observation_missing')
        check(o['worker_committed']is True and o['failure']is None,'worker_not_committed')
        check(o['input_accounting']=='glm-pinned-text/1','pinned_tokenizer_accounting_missing')
        check(submitted['individual_id']==owner and after['state']['individual_id']==owner,'individual_changed')
        check(before['input_count']==index and after['input_count']==index+1,'phase_input_count_mismatch')
        check(submitted['version']==before['state']['state_version']+1,'submitted_version_unbound')
        check(after['state']['state_version']==submitted['version']+1,'unexpected_state_mutation')
        for snap in (before,after):
            state=snap['state'];check(snap['transaction']=='repeatable_read_read_only' and valid_id(snap['capture_id']),'reference_capture_missing')
            check(state['budget_frozen']is False and snap['blocked_object_count']==0,'frozen_or_unexpected_privacy_state')
            check(type(state['state_version'])is int and state['state_version']>0 and type(state['privacy_epoch'])is int and state['privacy_epoch']>=0 and type(state['config_version'])is int and state['config_version']>0,'snapshot_version_shape')
            limits=state['limits']
            check(limits['budgets']=={'total_tokens_per_day':200000,'background_tokens_per_day':50000} and limits['max_model_calls_per_cycle']==3 and limits['max_tool_calls_per_cycle']==3,'original_limits_changed')
            for count,key,limit in (('belief_count','beliefs',32),('input_count','inputs',5),('source_count','source_events',128),('belief_event_count','belief_events',32)):
                check(type(snap[count])is int and snap[count]==len(snap[key]) and len(snap[key])<=limit,'snapshot_truncated_or_count_mismatch')
            for key in ('beliefs','inputs','source_events','belief_events'):
                ids=[r['id']for r in snap[key]]
                check(len(ids)==len(set(ids)) and all(valid_id(i)for i in ids),'snapshot_duplicate_or_invalid_ids')
                check(all(r['individual_id']==owner for r in snap[key]),'foreign_snapshot_record')
                if key!='beliefs':check(all(event_shape(e)for e in snap[key]),'typed_event_projection_missing')
            sources={e['id']:e for e in snap['source_events']}
            for e in snap['inputs']:
                check(sources.get(e['id'])==e and e['kind']=='message.received' and e['source_kind']=='reported_claim' and e['actor_id']=='owner','original_input_source_missing')
            for b in snap['beliefs']:
                check(b['scope'] and isinstance(b['statement'],str) and b['statement'].strip() and isinstance(b['brief_basis'],str) and b['brief_basis'].strip() and type(b['version'])is int and b['version']>0 and b['position']in ('held','tentative','questioned') and b['status']in ('current','superseded'),'belief_record_shape')
                check((b['status']=='superseded')==(b['superseded_at']is not None),'belief_status_time_mismatch')
                basis=b['support_event_ids']+b['counter_event_ids']
                check(0<len(set(basis))<=32 and same_ids(b['support_event_ids'],b['support_event_ids']) and same_ids(b['counter_event_ids'],b['counter_event_ids']),'belief_basis_shape')
                check(all(valid_id(i) and i in sources for i in basis),'belief_source_not_captured')
                check(b['position']!='held' or bool(b['support_event_ids']),'held_without_support')
        check(before['state']['privacy_epoch']==after['state']['privacy_epoch'] and before['state']['config_version']==after['state']['config_version'],'snapshot_epoch_changed')
        check(after['inputs'][-1]['id']==event_id and after['inputs'][-1]['payload']['text']==spec['stages'][index]['input'] and after['inputs'][-1]['payload']['conversation_id']==o['conversation_id'] and after['inputs'][-1]['correlation_id']==run,'fixed_input_altered_or_unbound')
        check(after['inputs'][:-1]==before['inputs'],'original_input_altered')
        after_sources={e['id']:e for e in after['source_events']}
        check(all(after_sources.get(e['id'])==e for e in before['source_events']),'original_source_altered_or_removed')
        if previous is not None:
            check(previous['phase']==PHASES[index-1] if index else False,'previous_stage_order')
            for key in ('state','belief_count','input_count','blocked_object_count','source_count','belief_event_count','beliefs','inputs','source_events','belief_events'):
                check(before[key]==previous['after'][key],'cross_process_state_not_restored')
            check(o['process_nonce']!=previous['process_nonce'] and o['conversation_id']!=previous['conversation_id'],'process_or_session_reused')
        before_b={b['id']:b for b in before['beliefs']};after_b={b['id']:b for b in after['beliefs']}
        observations=o['provider_observations']
        authors=[v for v in observations if v['role']=='author' and v['outcome']['status']=='parsed_candidate_not_commit_proof']
        reviews=[v for v in observations if v['role']=='decision_review']
        check(bool(authors),'actual_parsed_author_candidate_missing')
        check(len(o['commits'])==1,'actual_commit_count_mismatch')
        if len(o['commits'])!=1:return result
        commit=o['commits'][0];receipt=commit['receipt'];author_matches=[a for a in authors if a['outcome']['proposal_sha256']==commit['proposal_digest'] and any(r['author_reservation_id']==commit['reservation_id'] and r['context_digest']==a['context']['context_sha256']for r in o['reviews'])]
        check(len(author_matches)==1,'commit_candidate_binding_missing_or_ambiguous')
        if len(author_matches)!=1:return result
        author=author_matches[0];p=author['outcome']['proposal'];c=author['context'];drafts=p.get('belief_changes',[])
        check(author['outcome']['provider_mode']=='live' and bool(author['outcome']['provider_model']),'live_candidate_mode_missing')
        check(canonical(p)==commit['proposal_digest'],'candidate_digest_mismatch')
        check(not p['memory_changes'] and not p['goal_changes'] and not p['tool_intents'] and not p.get('entity_changes',[]) and p['development']is None,'unexpected_non_belief_writes')
        check(p['reply'].strip() and len(drafts)<=3,'candidate_reply_or_belief_bounds')
        for v in observations:
            check(v['run_id']==run and v['role']in ('author','decision_review'),'observation_run_or_role_mismatch')
            ctx=v['context'];check(ctx['individual_id']==owner and ctx['run_id']==run and ctx['state_version']==submitted['version'] and ctx['privacy_epoch']==before['state']['privacy_epoch'] and ctx['config_version']==before['state']['config_version'] and ctx['current_event_id']==event_id and ctx['current_scope']=='event:'+event_id,'context_snapshot_unbound')
            check(ctx['global_history_complete']is False and ctx['coverage']=='actual_selected_beliefs_and_target_material_only','context_coverage_misrepresented')
        events={e['id']:e for e in c['events']};memories={m['id']:m for m in c['memories']};manifest=c['input_manifest']
        check(same_ids(manifest,manifest) and event_id in manifest and events.get(event_id)==after_sources.get(event_id),'actual_question_or_manifest_missing')
        selected=c['selected_beliefs'];items=selected['items']if selected is not None else [];selected_b={b['id']:b for b in items}
        check(len(items)==len(selected_b) and len(items)<=6,'selected_belief_bounds')
        if selected is not None:check(type(selected['has_more'])is bool and (items or selected['has_more']is True),'selection_omission_metadata')
        for b in items:
            saved=before_b.get(b['id']);check(saved is not None,'selected_belief_not_preexisting')
            if saved is None:continue
            check(set(b)==set(BELIEF_FIELDS) and all(b[k]==saved[k]for k in BELIEF_FIELDS) and saved['status']=='current' and b['scope']in ('general',c['current_scope']),'selected_stance_version_scope_mismatch')
            check(b['id']in manifest,'selected_belief_manifest_missing')
            for eid in set(b['support_event_ids']+b['counter_event_ids']):
                check(eid in manifest and events.get(eid)==after_sources.get(eid) and eid in events,'selected_belief_basis_incomplete')
        eligible={b['id']for b in before_b.values()if b['status']=='current' and b['scope']in ('general',c['current_scope'])}
        if eligible-set(selected_b):check(selected is not None and selected['has_more']is True,'omitted_stance_not_flagged')
        for d in drafts:
            check(set(d)==set(DRAFT_FIELDS) and d['scope']in ('general',c['current_scope']) and d['position']in ('held','tentative','questioned'),'draft_shape_or_scope')
            for role in ('support','counter'):
                ids=d[role+'_event_ids'];check(same_ids(ids,ids),'duplicate_draft_role')
                check(all(e in manifest and e in events and events[e]==after_sources.get(e)for e in ids),'draft_event_source_unbound')
            check(0<len(set(d['support_event_ids']+d['counter_event_ids']))<=32 and (d['position']!='held' or d['support_event_ids']),'draft_basis_invalid')
            if d['supersedes']is not None:
                old=selected_b.get(d['supersedes']);check(old is not None and old['scope']==d['scope'],'draft_predecessor_not_current_same_scope')
        stored=[r for r in o['reviews']if r['author_reservation_id']==commit['reservation_id']]
        check(len(stored)==1,'actual_persisted_review_missing')
        if len(stored)!=1:return result
        reviewed=stored[0]
        review_matches=[r for r in reviews if r['packet'].get('decision_review',{}).get('author_reservation_id')==commit['reservation_id'] and r['outcome']['status']=='returned_assessment_not_commit_proof' and r['outcome']['assessment']==reviewed['assessment']]
        # Same-candidate protocol retries can leave failed captures. Only one
        # successful persisted assessment must match, never silently discard failures.
        check(len(review_matches)==1,'actual_review_observation_binding_missing')
        if len(review_matches)!=1:return result
        review=review_matches[0];packet=review['packet'];assessment=reviewed['assessment']
        check(packet['packet_version']=='target-citations/5' and packet['individual_id']==owner and packet['run_id']==run and packet['current_scope']==c['current_scope'],'v5_packet_snapshot_mismatch')
        check(packet['request_not_evidence']==events.get(event_id),'review_request_projection_mismatch')
        check(all(review['context'][k]==c[k]for k in ('selected_beliefs','input_manifest','visible_event_ids','visible_memory_ids')),'review_author_selection_changed')
        check(observations.index(author)<observations.index(review),'author_review_order_mismatch')
        check(packet['decision_review']['proposal']==p,'v5_packet_candidate_mismatch')
        check(reviewed['draft_digest']==canonical(p) and reviewed['context_digest']==c['context_sha256'] and reviewed['review_context_digest']==review['context']['context_sha256'],'review_context_candidate_digest_mismatch')
        check(reviewed['checker_version']==spec['baseline']['review_checker_version'],'v5_review_policy_binding_mismatch')
        check(reviewed['supported']is True and reviewed['provider_mode']=='live' and review['outcome']['provider_mode']=='live','review_not_live_or_not_committable')
        for row in (commit,reviewed):check(row['individual_id']==owner and row['run_id']==run and row['privacy_epoch']==before['state']['privacy_epoch'],'persisted_receipt_snapshot_mismatch')
        expected={('reply',None):p['reply_evidence_ids']}
        expected.update({('memory',i):m['source_event_ids']for i,m in enumerate(p['memory_changes'])})
        expected.update({('belief',i):sorted(set(d['support_event_ids']+d['counter_event_ids']))for i,d in enumerate(drafts)})
        expected.update({('person_assertion',i):[e['source']['event_id']]for i,e in enumerate(p.get('entity_changes',[]))})
        key=lambda t:(t['kind'],t.get('index'))
        targets={key(t['target']):t for t in packet['evidence_by_target']}
        checks={key(t['target']):t for t in assessment['checks']}
        check(set(targets)==set(expected) and len(targets)==len(packet['evidence_by_target']) and set(checks)==set(expected) and len(checks)==len(assessment['checks']),'review_target_coverage_mismatch')
        check([key(t)for t in packet['decision_review']['expected_targets']]==list(expected),'expected_target_order_mismatch')
        atomic=packet['runtime_atomic_commit_contract']
        check(atomic['publication_requires_successful_commit']is True and atomic['does_not_certify_tool_execution_or_goal_completion']is True and atomic['belief_change_indices']==list(range(len(drafts))) and atomic['memory_change_indices']==list(range(len(p['memory_changes']))),'atomic_contract_not_receipt_or_missing')
        for k,ids in expected.items():
            if k not in targets or k not in checks:continue
            target=targets[k];opinion=checks[k]
            check(same_ids(target['allowed_evidence_ids'],ids) and same_ids([e['record']['id']for e in target['evidence']],ids),'target_declared_material_mismatch')
            check(opinion['verdict']=='supported' and set(opinion['evidence_ids'])<=set(ids) and same_ids(opinion['evidence_ids'],opinion['evidence_ids']),'review_opinion_citation_mismatch')
            if k[0]=='belief':check(same_ids(opinion['evidence_ids'],ids),'review_omits_counter_or_support')
            for evidence in target['evidence']:
                record=evidence['record'];eid=record['id']
                if evidence['type']=='event':check(event_shape(record),'typed_packet_event_projection_missing')
                if evidence['type']=='event':check(record==events.get(eid) and eid in manifest,'packet_event_not_actual_author_source')
                elif evidence['type']=='memory':check(k[0]=='reply' and record==memories.get(eid) and eid in manifest,'packet_memory_unbound')
                elif evidence['type']=='belief':
                    check(k==('reply',None) and record==selected_b.get(eid) and evidence['proof_scope']=='individual_stance_not_world_fact','belief_not_world_fact_or_nonreply_evidence')
                    basis={e['record']['id']:e for e in evidence['basis_events']};all_ids=set(record['support_event_ids']+record['counter_event_ids'])
                    check(set(basis)==all_ids and len(basis)==len(evidence['basis_events']),'packet_selected_basis_missing')
                    for source in all_ids:
                        entry=basis.get(source);roles=[r for r in ('support','counter')if source in record[r+'_event_ids']]
                        check(entry is not None and same_ids(entry['roles'],roles) and entry['record']==events.get(source),'packet_selected_roles_or_source_missing')
                else:check(False,'unsupported_packet_evidence_type')
            if k[0]=='belief':
                draft=drafts[k[1]];view=target['belief_under_review']
                check(view['origin']=='proposed_interpretation_not_evidence' and view['proposal']==draft,'belief_under_review_mismatch')
                comparison=view['previous_comparison_not_additional_citations']
                if draft['supersedes']is None:check(comparison is None,'unexpected_previous_comparison')
                else:
                    old=selected_b.get(draft['supersedes']);check(comparison is not None and comparison['record']==old and comparison['proof_scope']=='previous_stance_not_world_fact','previous_comparison_missing_or_wrong')
                    if comparison is not None and old is not None:
                        old_ids=set(old['support_event_ids']+old['counter_event_ids'])
                        check(same_ids([e['id']for e in comparison['basis_events']],list(old_ids)) and all(e==events.get(e['id'])for e in comparison['basis_events']),'previous_comparison_basis_incomplete')
        new_ids=receipt.get('belief_ids',[])
        check(not set(new_ids)&(set(manifest)|set(p['reply_evidence_ids'])),'future_belief_id_used_as_author_evidence')
        check(same_ids(new_ids,new_ids) and len(new_ids)==len(drafts) and not set(new_ids)&set(before_b),'actual_belief_receipt_missing_or_future_ids')
        check(receipt['decision_event_id']==commit['decision_event_id'] and valid_id(receipt['decision_event_id']) and receipt['state_version']==after['state']['state_version'] and receipt['run_status']=='committed' and receipt['message_status']=='delivered','decision_receipt_not_committed')
        check(not receipt['memory_ids'] and not receipt['goal_ids'] and not receipt['action_ids'],'unexpected_receipt_other_writes')
        check(set(after_b)==set(before_b)|set(new_ids),'unreceipted_belief_rows_or_missing_history')
        updated=set()
        for i,bid in enumerate(new_ids):
            if i>=len(drafts)or bid not in after_b:continue
            draft=drafts[i];saved=after_b[bid]
            check(valid_id(bid) and saved['version']==1 and saved['status']=='current','new_belief_record_invalid')
            check(all(same_ids(saved[k],draft[k])if k in ('support_event_ids','counter_event_ids')else saved[k]==draft[k]for k in DRAFT_FIELDS),'persisted_belief_differs_from_candidate')
            if draft['supersedes']is not None:
                old_id=draft['supersedes'];updated.add(old_id);old=before_b.get(old_id);now=after_b.get(old_id)
                check(old is not None and now is not None,'superseded_history_missing')
                if old and now:
                    check(now['status']=='superseded' and now['version']==old['version']+1 and now['superseded_at']is not None and all(now[k]==value for k,value in old.items()if k not in ('status','version','superseded_at')),'old_stance_content_or_version_changed')
        check(all(after_b.get(i)==b for i,b in before_b.items()if i not in updated),'unrelated_or_general_stance_rewritten')
        old_event_ids={e['id']for e in before['belief_events']};new_events=[e for e in after['belief_events']if e['id']not in old_event_ids]
        check(all(next((n for n in after['belief_events']if n['id']==e['id']),None)==e for e in before['belief_events']),'original_belief_event_altered')
        check(len(new_events)==len(new_ids) and {e['payload']['belief_id']for e in new_events}==set(new_ids),'belief_inference_events_missing')
        future_ids=set(new_ids)|{e['id']for e in new_events}|{receipt['decision_event_id'],receipt['message_id']}
        check(not future_ids&set(manifest),'future_commit_objects_in_author_manifest')
        for e in new_events:
            b=after_b.get(e['payload']['belief_id'])
            check(b is not None and e['source_kind']=='inference' and e['actor_id']=='individual' and e['correlation_id']==run and e['payload']['supersedes']==b['supersedes'] and e['kind']==('belief.revised'if b['supersedes']else 'belief.formed'),'belief_event_not_bound_inference')
        check(len(o['replies'])==1 and len(o['decision_events'])==1,'actual_reply_or_decision_event_missing')
        if len(o['replies'])==1:
            reply=o['replies'][0];check(event_shape(reply),'typed_reply_event_projection_missing');check(reply['id']==receipt['message_id'] and reply['individual_id']==owner and reply['source_kind']=='generated' and reply['correlation_id']==run and reply['payload']['text']==p['reply'] and reply['payload']['evidence_ids']==p['reply_evidence_ids'] and reply['payload']['decision_id']==commit['decision_event_id'],'published_reply_not_candidate_or_receipt')
        if len(o['decision_events'])==1:
            event=o['decision_events'][0];check(event_shape(event),'typed_decision_event_projection_missing');check(event['id']==commit['decision_event_id'] and event['kind']=='decision.accepted' and event['correlation_id']==run and event['individual_id']==owner and event['source_kind']=='generated' and event['payload']['grounding_review_id']==reviewed['event_id'] and event['payload']['provider_model']==author['outcome']['provider_model'],'decision_event_review_not_bound')
        run_row=o['run'];check(run_row['id']==run and run_row['individual_id']==owner and run_row['trigger_event_id']==event_id and run_row['mode']=='foreground' and run_row['status']=='committed' and type(run_row['model_calls'])is int and 1<=run_row['model_calls']<=3 and type(run_row['tool_calls'])is int and run_row['tool_calls']==0,'actual_run_or_limits_mismatch')
        check(len(observations)==run_row['model_calls'],'actual_provider_call_observation_missing')
        ledger=check_ledger(o['ledger_snapshot']);check(ledger['complete'],'stage_ledger_not_known_complete')
        records={r['id']:r for r in o['ledger_snapshot']['records']}
        check(commit['reservation_id']in records and reviewed['review_reservation_id']in records and commit['reservation_id']!=reviewed['review_reservation_id'],'author_or_review_accounting_missing')
        for rid in (commit['reservation_id'],reviewed['review_reservation_id']):
            if rid in records:check(records[rid]['run_id']==run and records[rid]['individual_id']==owner,'receipt_accounting_run_mismatch')
        check(sum(r['run_id']==run for r in records.values())==run_row['model_calls'],'run_call_count_ledger_mismatch')
        general=[b for b in before_b.values()if b['scope']=='general'and b['status']=='current']
        created=[after_b[i]for i in new_ids if i in after_b]
        if phase=='form':
            check(not before_b and len(created)==1 and created[0]['scope']=='general' and created[0]['supersedes']is None,'initial_general_belief_not_formed')
        elif phase=='revise':
            check(len(general)==1 and len(created)==1 and created[0]['scope']=='general' and created[0]['supersedes']==general[0]['id'],'general_revision_not_linked')
            if created:
                check(bool(created[0]['support_event_ids'])and bool(created[0]['counter_event_ids'])and before['inputs'][0]['id']in created[0]['counter_event_ids'],'revision_prior_counter_missing')
        elif phase=='local':
            check(len(general)==1 and len(created)==1 and created[0]['scope']==c['current_scope'] and created[0]['supersedes']is None and after_b.get(general[0]['id'])==general[0],'local_exception_rewrites_general_or_missing')
        else:
            check(not drafts and not new_ids and len(general)==1 and general[0]['id']in selected_b and general[0]['id']in p['reply_evidence_ids'],'restored_general_or_no_write_evidence_missing')
        if created:
            result['focus_belief_id']=created[0]['id']
            check(event_id in created[0]['support_event_ids'],'new_understanding_current_input_support_missing')
        elif general:result['focus_belief_id']=general[0]['id']
    except (KeyError,TypeError,ValueError,AttributeError,IndexError,StopIteration):check(False,'required_observation_missing_or_malformed')
    result['machine_inventory_ready']=not result['issues']
    result['integrity_status']='complete'if not result['issues']else 'incomplete'
    return result


def check_suite(stages):
    if not isinstance(stages,list) or any(not isinstance(s,dict)for s in stages):
        return {'version':'alia-p01-belief-writes-integrity/1','machine_inventory_ready':False,'semantic_verdict':'unverified','live_model_pass':None,'runtime_execution_verified':False,'stages':[],'issues':['stage_list_malformed']}
    results=[]
    for index,stage in enumerate(stages):results.append(check_stage(stage,stages[index-1]if index else None))
    issues=[]
    if [s.get('phase')for s in stages]!=list(PHASES):issues.append('all_four_fixed_stages_required')
    for key in ('process_nonce','conversation_id'):
        values=[s.get(key)for s in stages]
        if not all(valid_id(v)for v in values) or len(set(v for v in values if isinstance(v,str)))!=len(values):issues.append('process_or_conversation_reused')
    if any(not r['machine_inventory_ready']for r in results):issues.append('stage_inventory_incomplete')
    return {'version':'alia-p01-belief-writes-integrity/1','criteria_sha256':digest(CASES),
            'machine_inventory_ready':not issues,'issues':issues,'stages':results,
            'semantic_verdict':'unverified','live_model_pass':None,'runtime_execution_verified':False,
            'limit':'Only declared observations and internal binding checked. Actual fixed runtime and independent semantic review required.'}


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',type=Path,required=True);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
    try:
        data=read_json(args.input);result=check_suite(data['reports']);result['input_sha256']=digest(args.input)
        result['checker_sha256']=digest(Path(__file__))
    except (OSError,ValueError,KeyError,TypeError):result={'machine_inventory_ready':False,'semantic_verdict':'unverified','live_model_pass':None,'issues':['input_or_frozen_cases_unavailable']}
    with args.output.open('x',encoding='utf-8')as f:json.dump(result,f,ensure_ascii=False,indent=2,allow_nan=False)
    return 0 if result['machine_inventory_ready']else 1


if __name__=='__main__':raise SystemExit(main())
