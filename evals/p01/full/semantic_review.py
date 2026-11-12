"""Independent annotation bindings and blinded templates; never grade language by keywords."""
import hashlib
import hmac
if __package__:
    from .check_materials import BLOCKERS,canonical,sha_shape
else:
    from check_materials import BLOCKERS,canonical,sha_shape


def response_binding(unit):
    return canonical({k:v for k,v in unit.items()if k not in ('proofs','independent_review')})


def review_template(spec, unit=None, *, salt):
    if not isinstance(salt,str)or len(salt)<16:raise ValueError('explicit_blinding_salt_required')
    alias=hmac.new(salt.encode(),spec['unit_id'].encode(),hashlib.sha256).hexdigest()[:24]
    return {'review_alias':alias,'kind':spec['kind'],'criteria':spec['criteria'],
            'inputs':unit.get('inputs')if unit else spec['inputs'],
            'responses':unit.get('responses')if unit else None,
            'input_source_event_ids':unit.get('input_event_ids')if unit else None,
            'program_evidence_refs':unit.get('proofs')if unit else None,
            'proof_reference_format':'capture SHA + record pointer + record SHA + actual object ID; open the explicit safe proof artifact, do not infer success from an ID',
            'selected_case_source_snapshot':unit.get('source_snapshot')if unit else None,
            'candidate_response_binding_sha256':response_binding(unit)if unit else None,
            'reviewer_id':None,'reviewer_kind':None,'independent_origin_proof':None,
            'criteria_verdicts':[{'id':c['id'],'verdict':None,'reason':None,'source_ids':[]}for c in spec['criteria']],
            'blocking_findings':{name:None for name in BLOCKERS},'status':'not_reviewed',
            'instruction':'Compare exact outputs and actual source/receipt evidence. Do not use provider supported/self-score. Growth condition and group label withheld.'}


def check_review(spec,unit,proof_check):
    issues=[];verdicts={};blockers={}
    try:
        review=unit['independent_review']
        if review['reviewer_kind']not in ('human','independent_model')or not review['reviewer_id']or review['reviewer_id']==unit['generator_id']:issues.append('independent_reviewer_origin_missing')
        if not proof_check(review['independent_origin_proof'],'independent_review_origin',response_binding(unit)):issues.append('independent_review_origin_proof_missing')
        if review['candidate_response_binding_sha256']!=response_binding(unit):issues.append('independent_review_candidate_source_mismatch')
        if review['criteria_sha256']!=canonical(spec['criteria']):issues.append('review_criteria_version_mismatch')
        items=review['criteria_verdicts'];verdicts={c['id']:c['verdict']for c in items}
        if set(verdicts)!={c['id']for c in spec['criteria']}or len(verdicts)!=len(items):issues.append('review_criteria_coverage_missing')
        if any(c['verdict']not in ('pass','fail','uncertain')or not isinstance(c['reason'],str)or not c['reason'].strip()for c in items):issues.append('independent_semantic_verdict_unverified')
        allowed=set(unit['input_event_ids'])|{r['object_id']for r in unit['proofs'].values()}|{r['actual_id']for r in unit['source_snapshot']['material_records']if r['logical_id']in unit['source_snapshot']['selected_logical_source_ids']}
        if any(not isinstance(c['source_ids'],list)or not set(c['source_ids'])<=allowed or (c['verdict']=='pass'and not c['source_ids'])for c in items):issues.append('independent_review_source_ids_unbound')
        blockers=review['blocking_findings']
        if set(blockers)!=set(BLOCKERS)or any(type(v)is not bool for v in blockers.values()):issues.append('blocking_findings_unverified')
        if review['reviewer_kind']=='independent_model'and not review.get('accounted_review_run_ids'):issues.append('independent_model_review_fee_missing')
    except (KeyError,TypeError,ValueError,AttributeError):issues.append('independent_review_missing_or_malformed')
    return {'complete':not issues,'issues':issues,'criteria':verdicts,'blockers':blockers,
            'all_criteria_pass':not issues and all(v=='pass'for v in verdicts.values()),
            'limit':'Independent origin is a capture declaration; only a future qualified adapter can establish trusted runtime provenance.'}
