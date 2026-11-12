"""Free checks of missing materials and premature acceptance claims."""
import copy
import unittest
import social_write_cases_check as check

class CasesTests(unittest.TestCase):
    def setUp(self):
        self.data = check.read_cases()
    def test_frozen_materials_are_complete_and_unverified(self):
        self.assertEqual(check.validate(self.data), [])
        self.assertIsNone(self.data['formal_acceptance'])
    def test_missing_source_or_semantic_review_is_rejected(self):
        for mutation in ('events', 'review', 'program'):
            data = copy.deepcopy(self.data)
            if mutation == 'events': data['cases'][0]['preconditions']['events'] = []
            elif mutation == 'review': data['cases'][0]['semantic_review']['required'] = False
            else: data['cases'][0]['program_assertions'] = []
            self.assertTrue(check.validate(data))
    def test_fictitious_execution_or_semantic_verdict_is_rejected(self):
        for key, value in [('execution_status', 'passed'), ('formal_pass', True)]:
            data = copy.deepcopy(self.data); data['cases'][0][key] = value
            self.assertTrue(check.validate(data))
        data = copy.deepcopy(self.data); data['cases'][0]['semantic_review']['verdict'] = 'pass'
        self.assertTrue(check.validate(data))
    def test_missing_state_or_case_cannot_pass_inventory(self):
        data = copy.deepcopy(self.data); data['cases'].pop()
        self.assertIn('case_count', check.validate(data))
        data = copy.deepcopy(self.data)
        for case in data['cases']: case['state_types'] = ['appraisal']
        self.assertIn('state_coverage', check.validate(data))

if __name__ == '__main__': unittest.main()
