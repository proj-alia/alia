import json
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts/qa'))
from development_usage import summarize

THREAD = '11111111-1111-4111-8111-111111111111'
TURN = '22222222-2222-4222-8222-222222222222'
TURN2 = '33333333-3333-4333-8333-333333333333'


class DevelopmentUsage(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'session.jsonl'
        self.context = {'type': 'turn_context', 'payload': {'turn_id': TURN, 'model': 'gpt-6.1-sol',
                                                           'effort': 'high'}}

    def record(self, rid='response', **usage):
        return {'timestamp': '2026-11-12T00:00:00Z', 'type': 'token_usage_record',
            'payload': {'thread_id': THREAD, 'turn_id': TURN, 'response_id': rid,
                'usage': dict({'input_tokens': 100, 'cached_input_tokens': 80,
                              'output_tokens': 20, 'reasoning_output_tokens': 12}, **usage),
                'thread_token_usage': {'input_tokens': 99999999}}}

    def run_rows(self, rows, mapping=None):
        self.path.write_text('\n'.join(json.dumps(x) for x in [self.context, *rows]))
        return summarize([(THREAD, self.path)], mapping)

    def test_dedup_and_subset_counts_without_cumulative_double_counting(self):
        record = self.record()
        result = self.run_rows([record, record, {'type': 'event_msg', 'payload': {
            'type': 'token_count', 'info': {'total_token_usage': {'input_tokens': 999999}}}}])
        self.assertEqual(result['unique_responses'], 1)
        group = result['groups'][0]
        self.assertEqual(group['known_tokens']['input_tokens'], 100)
        self.assertEqual(group['known_tokens']['output_tokens'], 20)
        self.assertEqual(group['known_tokens']['reasoning_output_tokens'], 12)
        self.assertEqual(group['unknown_records']['cache_write_input_tokens'], 1)
        self.assertIsNone(group['actual_cost'])
        self.assertIsNone(group['task_id_sha256'])
        self.assertIsNone(group['provider_latency_seconds'])

    def test_duplicate_conflict_is_not_silently_accepted(self):
        with self.assertRaisesRegex(ValueError, 'conflicting duplicate'):
            self.run_rows([self.record(), self.record(output_tokens=21)])

    def test_foreign_fork_records_excluded_and_tasks_explicit(self):
        foreign = self.record('old')
        foreign['payload']['thread_id'] = 'parent'
        result = self.run_rows([foreign, self.record()], {TURN: 'TASK-1'})
        self.assertEqual(result['unique_responses'], 1)
        self.assertEqual(result['groups'][0]['task_id_sha256'], hashlib.sha256(b'TASK-1').hexdigest())
        self.assertEqual(result['issues']['foreign_thread_records_excluded'], 1)

    def test_missing_and_invalid_usage_not_zero(self):
        missing = self.record()
        missing['payload']['usage'] = {}
        result = self.run_rows([missing, self.record('bad', cached_input_tokens=101)])
        self.assertEqual(result['groups'][0]['unknown_records']['input_tokens'], 1)
        self.assertEqual(result['issues']['invalid_usage_records'], 1)
        self.assertIsNone(result['actual_cost'])

    def test_model_changes_are_grouped_and_private_content_never_exported(self):
        context2 = {'type': 'turn_context', 'payload': {'turn_id': TURN2, 'model': 'gpt-6-luna', 'effort': 'low'}}
        record2 = self.record('r2')
        record2['payload']['turn_id'] = TURN2
        secret = {'type': 'response_item', 'payload': {'text': 'SECRET-PROMPT-AND-KEY'}}
        result = self.run_rows([self.record(), secret, context2, record2])
        self.assertEqual({g['model'] for g in result['groups']}, {'gpt-6.1-sol', 'gpt-6-luna'})
        self.assertNotIn('SECRET', json.dumps(result))

    def test_truncated_tail_keeps_measured_usage_with_warning(self):
        self.run_rows([self.record()])
        with self.path.open('a') as stream:
            stream.write('\n{"truncated":')
        result = summarize([(THREAD, self.path)])
        self.assertEqual(result['unique_responses'], 1)
        self.assertEqual(result['issues']['malformed_lines'], 1)

    def test_untrusted_identifiers_and_metadata_cannot_echo_private_marker(self):
        marker = 'SECRET-PROMPT-AND-KEY'
        self.context['payload'].update(model=marker, effort=marker)
        result = self.run_rows([self.record()], {TURN: marker})
        self.assertNotIn(marker, json.dumps(result))
        self.assertIsNone(result['groups'][0]['model'])
        self.assertIsNone(result['groups'][0]['effort'])
        record = self.record()
        record['payload']['turn_id'] = marker
        result = self.run_rows([record])
        self.assertEqual(result['unique_responses'], 0)
        with self.assertRaisesRegex(ValueError, 'canonical UUID'):
            summarize([(marker, self.path)])

    def test_timestamp_ranges_compare_actual_instants(self):
        first, last = self.record(), self.record('r2')
        first['timestamp'] = '2026-11-12T10:00:00+02:00'
        last['timestamp'] = '2026-11-12T09:00:00+00:00'
        group = self.run_rows([last, first])['groups'][0]
        self.assertEqual(group['first_record_at'], '2026-11-12T08:00:00+00:00')
        self.assertEqual(group['last_record_at'], '2026-11-12T09:00:00+00:00')


if __name__ == '__main__':
    unittest.main()
