#!/usr/bin/env python3
"""Offline evidence checker. It validates provenance, never guesses semantics."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
CASES = Path(__file__).with_name('cases-v1.json')
CATEGORIES = {'author_error', 'review_protocol_error', 'semantic_veto', 'runtime_rejection', 'driver_fault', 'unverified'}


def load(path):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('duplicate JSON key')
            result[key] = value
        return result
    return json.loads(path.read_text(), object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite')))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pointer(document, path):
    if path == '':
        return document
    if not path.startswith('/'):
        raise ValueError('JSON pointer required')
    value = document
    for part in path[1:].split('/'):
        part = part.replace('~1', '/').replace('~0', '~')
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def classify(report):
    """Keep distinct observed causes; no inference from provider prose/keywords."""
    findings = []
    failure = report.get('failure')
    if isinstance(failure, dict) and failure.get('category') == 'driver_fault':
        findings.append({'category': 'driver_fault', 'code': failure['code'], 'phase': failure.get('phase')})
    units = []
    for rindex, stage in enumerate(report.get('reports', [])):
        prefix = f'/reports/{rindex}'
        units.append((stage, prefix))
        units.extend((case, f'{prefix}/cases/{index}') for index, case in enumerate(stage.get('cases', [])))
    for stage, prefix in units:
        for index, diagnostic in enumerate(stage.get('diagnostics', [])):
            payload = diagnostic.get('payload', {})
            loc = f'{prefix}/diagnostics/{index}'
            kind = diagnostic.get('kind')
            if kind == 'decision.review_retry_scheduled':
                findings.append({'category': 'review_protocol_error', 'evidence': loc,
                                 'code': payload.get('validation_code', payload.get('code', 'review_retry'))})
            if kind == 'decision.reviewed':
                for check in payload.get('assessment', {}).get('checks', []):
                    if check.get('verdict') in ('unsupported', 'uncertain'):
                        findings.append({'category': 'semantic_veto', 'evidence': loc,
                                         'verdict': check['verdict'], 'target': check.get('target')})
            if kind in ('run.retry_scheduled', 'run.stopped'):
                code = payload.get('validation_code') or payload.get('code')
                if code:
                    findings.append({'category': 'runtime_rejection', 'code': code, 'evidence': loc})
        for index, review in enumerate(stage.get('review_diagnostics', [])):
            loc = f'{prefix}/review_diagnostics/{index}'
            if review.get('status') == 'assessment_failed':
                findings.append({'category': 'review_protocol_error',
                                 'code': review.get('validation_code') or 'assessment_failed', 'evidence': loc})
            for check in review.get('assessment', {}).get('checks', []):
                if check.get('verdict') in ('unsupported', 'uncertain'):
                    findings.append({'category': 'semantic_veto', 'evidence': loc,
                                     'verdict': check['verdict'], 'target': check.get('target')})
    return findings


def historical_accounting(report):
    # Recovery has reservation IDs/count and outranks older stage snapshots.
    recovery = report.get('accounting') or report.get('failure_accounting')
    if isinstance(recovery, dict) and recovery.get('recovered'):
        rows = recovery.get('usage', [])
        count = recovery.get('reservation_count')
        complete = type(count) is int and count == len(rows) and not recovery.get('usage_truncated', True)
        charge_key, unknown_key = 'charged_tokens', 'usage_unknown'
        scope = 'final recovered disposable database ledger'
    else:
        rows = None
        for stage in reversed(report.get('reports', [])):
            candidate = stage.get('usage', stage.get('all_usage'))
            if isinstance(candidate, list):
                rows = candidate
                break
        if rows is None:
            return {'charged_tokens': None, 'observed_tokens': None, 'reservations_observed': None,
                    'scope': 'missing', 'complete': False, 'monetary_cost': None}
        charge_key, unknown_key = 'charged', 'unknown'
        # A projection can fail before final JSON: last observed is a lower bound,
        # even if all rows are settled. Never pretend final ledger was recovered.
        complete = False
        scope = 'last cumulative stage snapshot; final-ledger completeness unverified'
    known_rows = all(type(row.get(charge_key)) is int and row.get(unknown_key) is False
                     and row.get('status') == 'settled' for row in rows)
    observed = sum(row[charge_key] for row in rows if type(row.get(charge_key)) is int)
    return {'charged_tokens': observed if complete and known_rows else None,
            'observed_tokens': observed, 'reservations_observed': len(rows),
            'scope': scope, 'complete': complete and known_rows, 'monetary_cost': None}


def evaluate(review, base=ROOT):
    errors, sources, source_meta = [], {}, {}
    case_ids = {case['id'] for case in load(CASES)['criteria']}
    if review.get('cases_sha256') != digest(CASES):
        errors.append('criteria hash mismatch')
    if review.get('method') != 'independent_source_comparison' or not review.get('reviewer'):
        errors.append('independent reviewer/method missing')
    if review.get('provider_self_assessment') is not False:
        errors.append('provider self assessment is insufficient')
    for item in review.get('sources', []):
        path = (base / item['path']).resolve()
        try:
            if digest(path) != item['sha256']:
                raise ValueError('hash mismatch')
            sources[item['id']] = load(path)
            source_meta[item['id']] = {'path': item['path'], 'sha256': item['sha256']}
        except (OSError, ValueError):
            errors.append(f"source unavailable or changed: {item['id']}")
    checks, seen = [], set()
    for item in review.get('checks', []):
        item = dict(item)
        cid, outcome = item.get('id'), item.get('outcome')
        valid = True
        if cid not in case_ids or cid in seen:
            errors.append(f'unknown or duplicate criterion: {cid}')
            valid = False
        seen.add(cid)
        if outcome not in ('passed', 'failed', 'unverified', 'not_run'):
            errors.append(f'invalid outcome: {cid}')
            valid = False
        if outcome in ('passed', 'failed'):
            if not item.get('comparison') or not item.get('evidence'):
                errors.append(f'claim/source comparison missing: {cid}')
                valid = False
            for evidence in item.get('evidence', []):
                try:
                    pointer(sources[evidence['source']], evidence['pointer'])
                except (KeyError, IndexError, ValueError, TypeError):
                    errors.append(f'unresolved evidence: {cid}')
                    valid = False
        if item.get('category') not in CATEGORIES and not (outcome == 'passed' and item.get('category') is None):
            errors.append(f'invalid category: {cid}')
            valid = False
        if not valid:
            item['outcome'] = 'unverified'
        checks.append(item)
    for missing in sorted(case_ids - seen):
        checks.append({'id': missing, 'outcome': 'unverified', 'category': 'unverified', 'comparison': 'No review evidence supplied'})
    # Errors can only reduce confidence; no majority/keyword/provider vote.
    behavior = False if any(c['outcome'] == 'failed' for c in checks) else (
        True if not errors and checks and all(c['outcome'] == 'passed' for c in checks) else None)
    case_checks, case_scope = [], review.get('case_scope')
    case_result = None
    if case_scope is not None:
        try:
            actual_cases = pointer(sources[case_scope['source']], case_scope['pointer'])
            expected = {case['id'] for case in actual_cases}
            if not expected or len(expected) != len(actual_cases):
                raise ValueError('invalid case scope')
        except (KeyError, IndexError, ValueError, TypeError):
            expected = set()
            errors.append('case scope unavailable')
        case_seen = set()
        for item in review.get('case_checks', []):
            item = dict(item)
            valid = True
            cid = item.get('id')
            if cid not in expected or cid in case_seen:
                errors.append(f'unknown or duplicate case: {cid}')
                valid = False
            case_seen.add(cid)
            outcome = item.get('outcome')
            if outcome not in ('passed', 'failed', 'unverified', 'not_run'):
                errors.append(f'invalid case outcome: {cid}')
                valid = False
            if outcome in ('passed', 'failed'):
                if not item.get('comparison') or not item.get('evidence'):
                    errors.append(f'case comparison missing: {cid}')
                    valid = False
                for evidence in item.get('evidence', []):
                    try:
                        pointer(sources[evidence['source']], evidence['pointer'])
                    except (KeyError, IndexError, ValueError, TypeError):
                        errors.append(f'unresolved case evidence: {cid}')
                        valid = False
            if item.get('category') not in CATEGORIES and not (outcome == 'passed' and item.get('category') is None):
                errors.append(f'invalid case category: {cid}')
                valid = False
            if not valid:
                item['outcome'] = 'unverified'
            case_checks.append(item)
        for missing in sorted(expected - case_seen):
            case_checks.append({'id': missing, 'outcome': 'unverified', 'category': 'unverified'})
        case_result = False if any(c['outcome'] == 'failed' for c in case_checks) else (
            True if expected and not errors and case_checks and all(c['outcome'] == 'passed' for c in case_checks) else None)
    if errors and behavior is True:
        behavior = None
    return {'version': 'alia-p01-independent-review/1', 'reviewer': review.get('reviewer'),
            'method': review.get('method'), 'scope': review.get('scope'),
            'cases_sha256': digest(CASES), 'sources': source_meta, 'checks': checks,
            'behavior_pass': behavior, 'validation_errors': errors,
            'case_scope': case_scope, 'case_checks': case_checks, 'case_behavior_pass': case_result,
            'engineering': {sid: {'engine_pass': source.get('engine_pass'),
                                  'stages': [r.get('phase') for r in source.get('reports', [])],
                                  'findings': classify(source),
                                  'accounting': historical_accounting(source)}
                            for sid, source in sources.items()},
            'limitations': review.get('limitations', [])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('review', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = evaluate(load(args.review))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2, allow_nan=False)
    print(json.dumps({'behavior_pass': result['behavior_pass'], 'validation_errors': result['validation_errors'], 'output': str(args.output)}))
    return 2 if result['validation_errors'] else 1 if result['behavior_pass'] is False else 0


if __name__ == '__main__':
    sys.exit(main())
