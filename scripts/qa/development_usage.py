#!/usr/bin/env python3
"""Read explicit development session files; emit aggregate usage, never prompts/tool outputs."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import uuid

MODELS = {'gpt-6-astra', 'gpt-6.1-sol', 'gpt-6.1-terra', 'gpt-6-sol', 'gpt-6-luna',
          'gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna'}
EFFORTS = {'none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'}


def canonical_uuid(value):
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value
    except (ValueError, AttributeError):
        return False


def summarize(sessions, turn_tasks=None):
    turn_tasks = turn_tasks or {}
    seen, groups, issues = {}, {}, Counter()
    for expected_thread, path in sessions:
        if not canonical_uuid(expected_thread):
            raise ValueError('session thread must be a canonical UUID')
        turns = {}
        with Path(path).open() as stream:
            for line in stream:
                try:
                    row = json.loads(line)
                except (ValueError, UnicodeError):
                    issues['malformed_lines'] += 1
                    continue
                if not isinstance(row, dict) or not isinstance(row.get('payload'), dict):
                    continue
                data = row['payload']
                if row.get('type') == 'turn_context':
                    if canonical_uuid(data.get('turn_id')):
                        model, effort = data.get('model'), data.get('effort')
                        if not isinstance(model, str) or model not in MODELS:
                            model = None
                            issues['unknown_model_metadata'] += 1
                        if not isinstance(effort, str) or effort not in EFFORTS:
                            effort = None
                            issues['unknown_effort_metadata'] += 1
                        turns[data['turn_id']] = (model, effort)
                if row.get('type') != 'token_usage_record':
                    continue  # cumulative token_count and Goal counters are NOT requests
                if data.get('thread_id') != expected_thread:
                    issues['foreign_thread_records_excluded'] += 1
                    continue
                rid, turn, usage = data.get('response_id'), data.get('turn_id'), data.get('usage')
                if not isinstance(rid, str) or not 1 <= len(rid) <= 256 or not isinstance(usage, dict) or not canonical_uuid(turn):
                    issues['invalid_usage_records'] += 1
                    continue
                fields = ('input_tokens', 'cached_input_tokens', 'cache_write_input_tokens',
                          'output_tokens', 'reasoning_output_tokens')
                measured = {k: usage.get(k) for k in fields}
                if any(v is not None and (type(v) is not int or v < 0) for v in measured.values()):
                    issues['invalid_usage_records'] += 1
                    continue
                if any(measured[a] is not None and measured[b] is not None and measured[a] > measured[b]
                       for a, b in [('cached_input_tokens', 'input_tokens'),
                                    ('reasoning_output_tokens', 'output_tokens')]):
                    issues['invalid_usage_records'] += 1
                    continue
                identity = (expected_thread, turn, measured)
                if rid in seen:
                    if seen[rid] != identity:
                        raise ValueError('conflicting duplicate response usage')
                    issues['duplicate_responses_excluded'] += 1
                    continue
                seen[rid] = identity
                model, effort = turns.get(turn, (None, None))
                task = turn_tasks.get(turn)
                if task is not None and (not isinstance(task, str) or not 1 <= len(task) <= 256):
                    raise ValueError('invalid task mapping')
                # Task labels are user supplied, so export a stable digest, never raw text.
                task_ref = hashlib.sha256(task.encode()).hexdigest() if task is not None else None
                key = (expected_thread, turn, model, effort, task_ref)
                group = groups.setdefault(key, {'thread_id': expected_thread, 'turn_id': turn,
                    'task_id_sha256': task_ref, 'model': model, 'effort': effort,
                    'responses': 0, 'known_tokens': dict.fromkeys(fields, 0),
                    'unknown_records': dict.fromkeys(fields, 0), 'first_record_at': None,
                    'last_record_at': None, 'provider_latency_seconds': None, 'actual_cost': None})
                group['responses'] += 1
                for k, v in measured.items():
                    if v is None:
                        group['unknown_records'][k] += 1
                    else:
                        group['known_tokens'][k] += v
                try:
                    stamp = datetime.fromisoformat(row['timestamp'].replace('Z', '+00:00'))
                    if stamp.tzinfo is None:
                        raise ValueError('timezone absent')
                    stamp = stamp.astimezone(timezone.utc).isoformat()
                    group['first_record_at'] = min(filter(None, [group['first_record_at'], stamp]))
                    group['last_record_at'] = max(filter(None, [group['last_record_at'], stamp]))
                except (KeyError, ValueError, TypeError):
                    issues['unknown_timestamps'] += 1
    return {'format': 'alia-development-usage/1', 'billing_scope': 'development_only',
            'unique_responses': len(seen), 'groups': list(groups.values()), 'issues': dict(issues),
            'actual_cost': None, 'provider_latency_seconds': None,
            'limitations': ['Partial local observation, not an account invoice.',
                'Known subtotals exclude unknown records; unknown usage/cost is not zero.',
                'Reasoning is a subset of output; cached input is a subset of input.',
                'Request timestamps are not provider latency. Goal/cumulative counters are excluded.',
                'Task association requires an explicit turn-to-task map; no historical inference.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--session', action='append', required=True, metavar='THREAD_ID=PATH')
    parser.add_argument('--turn-tasks', type=Path)
    args = parser.parse_args()
    mapping = json.loads(args.turn_tasks.read_text()) if args.turn_tasks else None
    if mapping is not None and (not isinstance(mapping, dict) or
                               any(not isinstance(k, str) or not isinstance(v, str) for k, v in mapping.items())):
        parser.error('turn-tasks must map turn IDs to task IDs')
    sessions = []
    for value in args.session:
        thread, separator, path = value.partition('=')
        if not separator or not thread or not path:
            parser.error('session must be THREAD_ID=PATH')
        sessions.append((thread, path))
    print(json.dumps(summarize(sessions, mapping), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
