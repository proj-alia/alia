"""Read-only evidence index integrity; no runtime or semantic verdicts."""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
INDEX = ROOT / 'evals/p01/social_delivery_index.json'

class SocialDeliveryIndexTests(unittest.TestCase):
    def test_index_files_hashes_and_required_evidence(self):
        data = json.loads(INDEX.read_text())
        self.assertEqual(data['format'], 'alia-social-delivery-index/1')
        required = {'social_review_ready','social_review_actual_report','social_fixed_input_manifest','social_v2_original_report','social_review_pg_log','social_review_clippy_log','social_cases','social_cases_checker','social_cases_tests','social_cases_delivery_ready','social_cases_tests_log','original_gates'}
        self.assertEqual(set(data['artifacts']), required)
        for item in data['artifacts'].values():
            path = Path(item['path'])
            if not path.is_absolute(): path = ROOT / path
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), item['sha256'], str(path))
        review = json.loads((ROOT / data['artifacts']['social_review_ready']['path']).read_text())
        self.assertEqual(review['new_actual_PG_tests'], {'passed':3,'failed':0,'ignored':0})
        self.assertEqual(review['strict_clippy_exit'], 101)
        cases = json.loads((ROOT / data['artifacts']['social_cases']['path']).read_text())
        delivery = json.loads(Path(data['artifacts']['social_cases_delivery_ready']['path']).read_text())
        self.assertEqual(len(cases['cases']), 8)
        self.assertEqual(delivery['offline_tests_passed'], 4)
        self.assertIsNone(cases['formal_acceptance'])
        self.assertEqual(data['social_write_cases']['status'], 'not_run')
        self.assertIsNone(data['formal_acceptance'])
        gates = json.loads((ROOT / data['artifacts']['original_gates']['path']).read_text())['formal_gates']
        self.assertEqual((gates['retrieval_queries'], gates['behaviour_denominator'], gates['growth_groups'], gates['minimum_C_reasonable_adjustments']), (100,180,20,16))
        self.assertEqual((gates['recall_at_10_macro_minimum'], gates['behaviour_pass_rate_minimum']), (0.9,0.9))
        self.assertIn('interrupted; not_run', data['protocol_01'])

if __name__ == '__main__': unittest.main()
