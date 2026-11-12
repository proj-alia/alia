#!/usr/bin/env python3
"""Validate offline case inventory only; never grade semantic behavior."""
import hashlib
import json
from pathlib import Path

CASES = Path(__file__).with_name('social_write_cases_v1.json')
SHA256 = '761e31d5be41e7c1de091016761a88140ce7c3755d8bb9f949d3997e86d58baf'
TYPES = {'appraisal', 'attitude', 'relationship'}

def read_cases(path=CASES):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SHA256:
        raise ValueError('frozen case hash mismatch')
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate JSON key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique, parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite number')))

def validate(data):
    errors = []
    if data.get('version') != 'alia-social-write-cases/1' or data.get('task_id') != 'P01-SOCIAL-WRITE-CASES-01':
        errors.append('version')
    cases = data.get('cases', [])
    if not isinstance(cases, list) or len(cases) != 8:
        return ['case_count']
    if [c.get('id') for c in cases] != [f'SW-{i:02}' for i in range(1, 9)]:
        errors.append('case_ids')
    types = set()
    for c in cases:
        ident = c.get('id', '?')
        state_types = c.get('state_types', [])
        if not isinstance(state_types, list) or not state_types or len(set(state_types)) != len(state_types) or not set(state_types) <= TYPES:
            errors.append(ident + ':types')
        else:
            types.update(state_types)
        pre = c.get('preconditions', {})
        events = pre.get('events', [])
        if not pre.get('prior_state') or not isinstance(events, list) or len(events) < 2:
            errors.append(ident + ':preconditions')
        else:
            ids = [e.get('id') for e in events]
            if len(set(ids)) != len(ids) or any(not e.get('text') or e.get('source_kind') not in {'reported_claim', 'direct_observation', 'absence_observation'} for e in events):
                errors.append(ident + ':events')
        if not c.get('title') or not c.get('proposed_change') or any(not isinstance(c.get(k), list) or not c[k] or any(not isinstance(v, str) or not v.strip() for v in c[k]) for k in ('allowed_results', 'forbidden_results', 'program_assertions')):
            errors.append(ident + ':criteria')
        review = c.get('semantic_review', {})
        if review.get('required') is not True or not review.get('question') or review.get('verdict') is not None or c.get('execution_status') != 'not_run' or c.get('formal_pass') is not None:
            errors.append(ident + ':unverified')
    if types != TYPES or not any(len(c['state_types']) == 3 for c in cases):
        errors.append('state_coverage')
    if data.get('semantic_verdict') != 'unverified' or data.get('formal_acceptance') is not None or data.get('actual_model_calls') != 0 or data.get('actual_db_calls') != 0:
        errors.append('execution_claim')
    return errors

def main():
    try:
        errors = validate(read_cases())
    except (OSError, ValueError, TypeError, KeyError):
        errors = ['input_invalid']
    print(json.dumps({'task_id':'P01-SOCIAL-WRITE-CASES-01','material_ready':not errors,'issues':errors,'case_count':8,'semantic_verdict':'unverified','formal_acceptance':None,'actual_model_calls':0,'actual_db_calls':0}, ensure_ascii=False))
    return int(bool(errors))

if __name__ == '__main__':
    raise SystemExit(main())
