#!/usr/bin/env python3
"""Version-bound handoff checks. Never executes jobs or changes Goal state."""
import argparse
import hashlib
import json
from pathlib import Path
import re

from common import digest, read_json, safe_relative, write_new

STATES = ('defined', 'ready', 'implementing', 'review_pending', 'engineering_pending', 'accepted')
SUBJECT_FIELDS = ('task_id', 'revision', 'owner', 'reviewers', 'risk', 'baseline',
                  'sources', 'dependencies', 'interfaces', 'scope', 'goal',
                  'acceptance', 'delta')


def subject_digest(card):
    """Bind the exact work specification, source set, and intended reviewers."""
    data = {key: card[key] for key in SUBJECT_FIELDS}
    return hashlib.sha256(json.dumps(data, sort_keys=True, ensure_ascii=False,
                                    separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def version_binding(value):
    return (isinstance(value, dict) and type(value.get('revision')) is int
            and value['revision'] > 0 and isinstance(value.get('subject_sha256'), str)
            and re.fullmatch(r'[a-f0-9]{64}', value['subject_sha256']) is not None)


def files_match(root, files):
    require(isinstance(files, dict) and bool(files), 'empty source/evidence binding')
    root = Path(root).resolve()
    for name, sha in files.items():
        safe_relative(name)
        require(isinstance(sha, str) and re.fullmatch(r'[a-f0-9]{64}', sha), 'invalid digest')
        path = root / name
        require(not any(p.is_symlink() for p in [path, *path.parents] if p != root.parent),
                'symlink evidence/source refused')
        require(path.is_file() and path.resolve().is_relative_to(root), 'missing or escaped source/evidence')
        require(digest(path) == sha, 'changed source/evidence: ' + name)


def validate(card, registry, root, phase='implement'):
    require(phase in ('implement', 'review', 'ready'), 'unknown gate')
    require(card.get('format') == 'alia-task-card/1', 'unsupported task card')
    require(isinstance(card.get('revision'), int) and not isinstance(card['revision'], bool)
            and card['revision'] > 0, 'invalid revision')
    for key in ('task_id', 'owner', 'baseline', 'goal', 'delta'):
        require(isinstance(card.get(key), str) and bool(card[key].strip()), 'missing ' + key)
    require(card.get('risk') in ('simple', 'high'), 'invalid risk')
    for key in ('reviewers', 'scope', 'acceptance'):
        require(isinstance(card.get(key), list) and bool(card[key])
                and all(isinstance(x, str) and x.strip() for x in card[key]), 'missing ' + key)
    require(len(set(card['reviewers'])) == len(card['reviewers'])
            and card['owner'] not in card['reviewers'], 'author cannot review own task')
    for path in card['scope']:
        safe_relative(path)
    require(isinstance(card.get('interfaces'), dict) and isinstance(card.get('dependencies'), dict),
            'missing interfaces/dependencies')
    require(registry.get('format') == 'alia-task-registry/1', 'unsupported registry')
    current = registry.get('tasks', {}).get(card['task_id'], {})
    binding = subject_digest(card)
    require(version_binding(current) and current['revision'] == card['revision']
            and current['subject_sha256'] == binding,
            'stale or unregistered task card')
    require(current.get('state') in STATES and current.get('state') != 'defined', 'task not released')
    require(current.get('released_by') == 'Root', 'Root release required')
    for name, version in card['interfaces'].items():
        require(isinstance(version, str) and registry.get('interfaces', {}).get(name) == version,
                'stale interface: ' + name)
    for name, expected in card['dependencies'].items():
        actual = registry.get('tasks', {}).get(name, {})
        require(version_binding(expected) and version_binding(actual)
                and expected['revision'] == actual['revision']
                and expected['subject_sha256'] == actual['subject_sha256']
                and actual.get('state') == 'accepted', 'unaccepted or stale dependency: ' + name)
    files_match(root, card['sources'])
    require(set(card['sources']) <= set(card['scope']), 'sources outside declared exact file scope')
    if phase == 'ready':
        reviews = card.get('reviews', [])
        require(isinstance(reviews, list), 'missing reviews')
        approved = set()
        for review in reviews:
            require(isinstance(review, dict), 'invalid review')
            reviewer = review.get('reviewer')
            require(reviewer in card['reviewers'] and reviewer != card['owner'], 'unassigned/self review')
            require(reviewer not in approved, 'duplicate review')
            require(review.get('subject_sha256') == binding, 'stale review')
            require(review.get('decision') == 'approved' and type(review.get('open_blockers')) is int
                    and review['open_blockers'] == 0,
                    'review blocks delivery')
            files_match(root, review.get('evidence'))
            approved.add(reviewer)
        require(approved == set(card['reviewers']), 'required independent review missing')
        result = card.get('validation', {})
        require(result.get('subject_sha256') == binding and result.get('status') == 'passed',
                'missing/stale local validation')
        require(isinstance(result.get('commands'), list) and bool(result['commands'])
                and all(isinstance(x, dict) and isinstance(x.get('command'), str)
                        and x['command'].strip() and type(x.get('exit_code')) is int
                        and x['exit_code'] == 0 for x in result['commands']), 'failed or absent commands')
        require(isinstance(result.get('not_run'), list), 'unrun coverage must be explicit')
        files_match(root, result.get('evidence'))
    return {'task_id': card['task_id'], 'revision': card['revision'],
            'subject_sha256': binding, 'gate': phase, 'passed': True,
            'engineering_acceptance': 'not established by this metadata gate',
            'behavior_acceptance': 'not established by this metadata gate'}


def claim_notification(directory, event):
    """Atomic at-most-once dispatch receipt. Crash after claim remains unknown, never auto-retry."""
    require(set(event) == {'task_id', 'revision', 'state', 'subject_sha256', 'evidence_sha256'},
            'invalid notification fields')
    require(isinstance(event['task_id'], str) and event['task_id'].strip()
            and type(event['revision']) is int and event['revision'] > 0
            and event['state'] in STATES
            and isinstance(event['subject_sha256'], str)
            and re.fullmatch('[a-f0-9]{64}', event['subject_sha256'])
            and isinstance(event['evidence_sha256'], str)
            and re.fullmatch('[a-f0-9]{64}', event['evidence_sha256']), 'invalid notification')
    identity = json.dumps([event[k] for k in ('task_id', 'revision', 'state')], ensure_ascii=False)
    slot = Path(directory) / hashlib.sha256(identity.encode()).hexdigest()
    slot.mkdir(parents=True, exist_ok=True)
    binding = {key: event[key] for key in ('task_id', 'revision', 'state', 'subject_sha256')}
    try:
        write_new(slot / 'subject.json', binding, private=True)
    except FileExistsError:
        require(read_json(slot / 'subject.json') == binding,
                'conflicting notification for same revision/state')
    path = slot / (event['evidence_sha256'] + '.json')
    try:
        write_new(path, event, private=True)
        return 'claimed'
    except FileExistsError:
        require(read_json(path) == event, 'conflicting notification for same revision/state')
        return 'already_claimed_outcome_unknown'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--card', type=Path, required=True)
    parser.add_argument('--registry', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--phase', choices=('implement', 'review', 'ready'), default='implement')
    args = parser.parse_args()
    print(json.dumps(validate(read_json(args.card), read_json(args.registry), args.root, args.phase),
                     ensure_ascii=False))


if __name__ == '__main__':
    main()
