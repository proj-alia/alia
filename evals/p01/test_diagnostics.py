"""Independent synthetic files only; no subprocess/provider/DB usage."""
import json
from pathlib import Path
import tempfile
import unittest

import diagnostics
from check_results import CASES, digest


class DiagnosticTests(unittest.TestCase):
    def fixture(self, folder, *, pinned=True, outcome='failed', category='author_error'):
        source = folder / 'source.json'
        source.write_text(json.dumps({'provenance': {'checkpoint_verified': pinned,
            'checkpoint_id': 'fixed' if pinned else None, 'binaries': {'live_individual': 'f' * 64}},
            'reports': [{'phase': 'unknown', 'author_candidates': [{'provider_model': 'fixture-model'}],
                         'replies': []}]}))
        review = folder / 'review.json'
        review.write_text(json.dumps({'version': 'alia-p01-independent-review/1',
            'validation_errors': [], 'cases_sha256': digest(CASES),
            'method': 'independent_source_comparison', 'behavior_pass': False if outcome == 'failed' else None,
            'sources': {'run': {'path': str(source), 'sha256': digest(source)}},
            'checks': [{'id': 'unknown-history', 'outcome': outcome, 'category': category,
                'comparison': 'Current question does not support the appended historical claim.',
                'evidence': [{'source': 'run', 'pointer': '/reports/0/author_candidates/0'}]}]}))
        return source, review

    def test_version_expected_actual_and_root_route_preserved(self):
        with tempfile.TemporaryDirectory() as folder:
            source, review = self.fixture(Path(folder))
            packet = diagnostics.build_packet(review)
            item = packet['entries'][0]
            self.assertEqual(item['kind'], 'observed_failure')
            self.assertEqual(item['responsibility']['owner'], 'main')
            self.assertTrue(item['expected'])
            self.assertEqual(item['evidence'][0]['sha256'], digest(source))
            self.assertEqual(packet['candidate_versions']['run']['checkpoint_id'], 'fixed')
            self.assertEqual(packet['candidate_versions']['run']['observed_provider_models'], ['fixture-model'])
            self.assertFalse(item['reproduction']['paid_rerun_authorized'])

    def test_missing_version_never_becomes_current_checkpoint(self):
        with tempfile.TemporaryDirectory() as folder:
            _, review = self.fixture(Path(folder), pinned=False)
            packet = diagnostics.build_packet(review)
            self.assertFalse(packet['candidate_versions']['run']['checkpoint_verified'])
            self.assertIsNone(packet['candidate_versions']['run']['checkpoint_id'])
            self.assertTrue(packet['candidate_versions']['run']['version_limit'])

    def test_evidence_gap_routes_to_observation_without_failure_upgrade(self):
        with tempfile.TemporaryDirectory() as folder:
            _, review = self.fixture(Path(folder), outcome='unverified', category='unverified')
            packet = diagnostics.build_packet(review)
            self.assertEqual(packet['entries'][0]['kind'], 'evidence_gap')
            self.assertEqual(packet['entries'][0]['responsibility']['owner'], 'ALIA-P01')
            self.assertIsNone(packet['behavior_pass'])

    def test_source_mutation_or_missing_pointer_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            source, review = self.fixture(Path(folder))
            original = source.read_text()
            source.write_text('{}')
            with self.assertRaisesRegex(ValueError, 'source report changed'):
                diagnostics.build_packet(review)
            source.write_text(original)
            data = json.loads(review.read_text())
            data['checks'][0]['evidence'][0]['pointer'] = '/missing'
            review.write_text(json.dumps(data))
            with self.assertRaises(KeyError):
                diagnostics.build_packet(review)

    def test_recovered_protocol_fault_is_retained_without_semantic_failure_claim(self):
        with tempfile.TemporaryDirectory() as folder:
            source, review = self.fixture(Path(folder), outcome='unverified', category='unverified')
            raw = json.loads(source.read_text())
            raw['reports'][0]['review_diagnostics'] = [{'status': 'assessment_failed', 'validation_code': 'assessment_schema'}]
            source.write_text(json.dumps(raw))
            data = json.loads(review.read_text())
            data['sources']['run']['sha256'] = digest(source)
            data['engineering'] = {'run': {'findings': [{'category': 'review_protocol_error',
                'code': 'assessment_schema', 'evidence': '/reports/0/review_diagnostics/0'}]}}
            review.write_text(json.dumps(data))
            packet = diagnostics.build_packet(review)
            event = packet['entries'][-1]
            self.assertEqual(event['kind'], 'observed_runtime_event')
            self.assertEqual(event['outcome'], 'observed')
            self.assertEqual(event['category'], 'review_protocol_error')
            self.assertEqual(event['actual_comparison']['code'], 'assessment_schema')
            self.assertIsNone(packet['behavior_pass'])

    def test_unexecuted_coverage_is_not_fabricated_into_a_bug(self):
        with tempfile.TemporaryDirectory() as folder:
            _, review = self.fixture(Path(folder))
            data = json.loads(review.read_text())
            data['checks'] = [{'id': 'reading', 'outcome': 'unverified', 'category': 'unverified',
                               'comparison': 'No review evidence supplied'}]
            review.write_text(json.dumps(data))
            self.assertEqual(diagnostics.build_packet(review)['entries'], [])


if __name__ == '__main__':
    unittest.main()
