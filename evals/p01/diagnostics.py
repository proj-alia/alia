#!/usr/bin/env python3
"""Create source-bound defect/gap handoffs from an independent P01 review.

No provider, runtime or DB calls. Routing indicates an investigation owner;
it is never proof that a production module is at fault.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path

from check_results import CASES, ROOT, digest, load, pointer

ROUTING = {
    'author_error': {'owner': 'main', 'module': 'author proposal / content grounding',
                     'paths': ['src/adapters/model.rs', 'src/domain/grounding.rs']},
    'review_protocol_error': {'owner': 'main', 'module': 'structured reviewer adapter / review contract',
                              'paths': ['src/adapters/model.rs', 'src/domain/grounding.rs']},
    'semantic_veto': {'owner': 'main', 'module': 'author/reviewer semantic comparison; inspect before changing rules',
                      'paths': ['src/domain/grounding.rs']},
    'runtime_rejection': {'owner': 'main', 'module': 'runtime rejection; determine exact validation code',
                          'paths': ['src/runtime/worker.rs']},
    'driver_fault': {'owner': 'ALIA-P01', 'module': 'behavior acceptance driver',
                     'paths': ['scripts/test_live_lifecycle.py']},
    'unverified': {'owner': 'ALIA-P01', 'module': 'synthetic observation / independent evidence',
                   'paths': ['examples/support/observed_provider.rs', 'evals/p01/check_results.py']},
}


def versions(raw, source):
    provenance = raw.get('provenance') or {}
    provider_models = set()
    for stage in raw.get('reports', []):
        provider_models.update(model for model in stage.get('models', []) if isinstance(model, str))
        units = [stage, *stage.get('cases', [])]
        for unit in units:
            for candidate in unit.get('author_candidates', []):
                if isinstance(candidate.get('provider_model'), str):
                    provider_models.add(candidate['provider_model'])
            for review in unit.get('review_diagnostics', []):
                if isinstance(review.get('provider_model'), str):
                    provider_models.add(review['provider_model'])
    return {'source_report': source['path'], 'source_sha256': source['sha256'],
            'checkpoint_verified': provenance.get('checkpoint_verified') is True,
            'checkpoint_id': provenance.get('checkpoint_id'),
            'checkpoint_sha256': provenance.get('checkpoint_sha256'),
            'artifact_manifest_sha256': provenance.get('artifact_manifest_sha256'),
            'binary_hashes': provenance.get('binaries'),
            'source_hashes': provenance.get('sources'),
            'observed_provider_models': sorted(provider_models),
            'version_limit': None if provenance.get('checkpoint_verified') is True else
                'Historical report is pinned; build/source/config equivalence is not established.'}


def build_packet(review_path, base=ROOT):
    review_path = Path(review_path)
    review = load(review_path)
    if review.get('version') != 'alia-p01-independent-review/1' or review.get('validation_errors'):
        raise ValueError('validated independent review required')
    if review.get('cases_sha256') != digest(CASES) or review.get('method') != 'independent_source_comparison':
        raise ValueError('independent criteria/method mismatch')
    criteria = {item['id']: item['rule'] for item in load(CASES)['criteria']}
    sources, metadata = {}, {}
    for sid, source in review.get('sources', {}).items():
        path = Path(base) / source['path']
        if digest(path) != source['sha256']:
            raise ValueError('source report changed')
        raw = load(path)
        sources[sid] = raw
        metadata[sid] = versions(raw, source)
    entries = []
    for group in ('checks', 'case_checks'):
        for check in review.get(group, []):
            outcome = check.get('outcome')
            # Missing mandatory coverage is retained in the review, not
            # fabricated into an observed production defect.
            if outcome not in ('failed', 'unverified') or not check.get('evidence'):
                continue
            category = check.get('category')
            if category not in ROUTING or not check.get('comparison'):
                raise ValueError('diagnostic classification/comparison missing')
            references = []
            for evidence in check['evidence']:
                sid = evidence['source']
                pointer(sources[sid], evidence['pointer'])
                references.append({'source': sid, 'pointer': evidence['pointer'],
                                   'path': review['sources'][sid]['path'],
                                   'sha256': review['sources'][sid]['sha256']})
            expected = criteria.get(check['id'])
            if group == 'case_checks':
                scope = review.get('case_scope') or {}
                cases = pointer(sources[scope['source']], scope['pointer'])
                case = next((case for case in cases if case.get('id') == check['id']), None)
                if case is None:
                    raise ValueError('case expectation missing')
                expected = case.get('expected_behavior')
            entries.append({'id': f"{group}:{check['id']}",
                            'kind': 'observed_failure' if outcome == 'failed' else 'evidence_gap',
                            'outcome': outcome, 'category': category, 'expected': expected,
                            'actual_comparison': check['comparison'], 'evidence': references,
                            'candidate_versions': sorted({e['source'] for e in references}),
                            'responsibility': ROUTING[category],
                            'routing_limit': 'Investigation owner only; not proof of root cause or authorization to edit production rules.',
                            'reproduction': {'method': 'offline source/result comparison',
                                             'paid_rerun_authorized': False}})
    seen_events = set()
    for sid, engineering in review.get('engineering', {}).items():
        for finding in engineering.get('findings', []):
            category, location = finding.get('category'), finding.get('evidence')
            if category not in ROUTING or not isinstance(location, str):
                continue
            observed = pointer(sources[sid], location)
            event_id = observed.get('event_id', observed.get('id')) if isinstance(observed, dict) else None
            identity = (sid, category, finding.get('code'), event_id or location)
            if identity in seen_events:
                continue
            seen_events.add(identity)
            references = [{'source': sid, 'pointer': location,
                           'path': review['sources'][sid]['path'], 'sha256': review['sources'][sid]['sha256']}]
            # Protocol and runtime observations are retained even when a retry
            # later succeeds. They are not automatically semantic defects.
            entries.append({'id': f"diagnostic:{sid}:{len(seen_events)}",
                'kind': 'observed_runtime_event', 'outcome': 'observed', 'category': category,
                'expected': 'Preserve the exact structured diagnostic, role and charge; a correct rejection/recovered fault is distinct from semantic failure.',
                'actual_comparison': {key: finding[key] for key in ('code', 'verdict', 'target') if key in finding},
                'evidence': references, 'candidate_versions': [sid], 'responsibility': ROUTING[category],
                'routing_limit': 'Observed event only; inspect the independent semantic comparison before declaring a production defect.',
                'reproduction': {'method': 'offline structured diagnostic inspection', 'paid_rerun_authorized': False}})
    return {'version': 'alia-p01-diagnostic-packet/1', 'created_at': datetime.now(timezone.utc).isoformat(),
            'task': 'ALIA-P01', 'review_path': str(review_path), 'review_sha256': digest(review_path),
            'criteria_sha256': digest(CASES), 'candidate_versions': metadata, 'entries': entries,
            'behavior_pass': review.get('behavior_pass'),
            'case_behavior_pass': review.get('case_behavior_pass'),
            'model_calls': 0, 'scope': 'Existing synthetic acceptance evidence only; no new live reproduction.',
            'limitations': ['Evidence gaps are not semantic failures or behavioral passes.',
                            'A semantic veto may be correct containment; review the independent comparison before changing code.',
                            'Historical unpinned code/config remains explicitly unknown.',
                            'New defects must be rerouted by the sole coordinator if ownership changes.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('review', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = build_packet(args.review)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2, allow_nan=False)
    print(json.dumps({'entries': len(result['entries']), 'output': str(args.output), 'model_calls': 0}))


if __name__ == '__main__':
    main()
