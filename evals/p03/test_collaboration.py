import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts/qa'))
from collaboration import validate, subject_digest, claim_notification
from common import digest


class Collaboration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'code.py').write_text('version 1')
        (self.root / 'evidence.json').write_text('{"passed":true}')
        self.card = {'format': 'alia-task-card/1', 'task_id': 'T1', 'revision': 1,
            'owner': 'P04', 'reviewers': ['P05'], 'risk': 'high', 'baseline': 'checkpoint-1',
            'sources': {'code.py': digest(self.root / 'code.py')}, 'dependencies': {},
            'interfaces': {'test-api': 'v1'}, 'scope': ['code.py'], 'goal': 'Preserve history',
            'acceptance': ['Old version rejected'], 'delta': 'First slice'}
        sha = subject_digest(self.card)
        self.registry = {'format': 'alia-task-registry/1', 'interfaces': {'test-api': 'v1'},
            'tasks': {'T1': {'revision': 1, 'subject_sha256': sha,
                            'state': 'engineering_pending', 'released_by': 'Root'}}}
        evidence = {'evidence.json': digest(self.root / 'evidence.json')}
        self.card['reviews'] = [{'reviewer': 'P05', 'subject_sha256': sha,
            'decision': 'approved', 'open_blockers': 0, 'evidence': evidence}]
        self.card['validation'] = {'subject_sha256': sha, 'status': 'passed',
            'commands': [{'command': 'fixture-test', 'exit_code': 0}],
            'not_run': ['real behavior'], 'evidence': evidence}

    def test_ready_requires_independent_bound_evidence(self):
        self.assertTrue(validate(self.card, self.registry, self.root, 'ready')['passed'])
        self.card['reviews'] = []
        with self.assertRaisesRegex(ValueError, 'review missing'):
            validate(self.card, self.registry, self.root, 'ready')

    def test_stale_card_or_interface_is_rejected_before_implementation(self):
        self.registry['tasks']['T1']['revision'] = 2
        with self.assertRaisesRegex(ValueError, 'stale'):
            validate(self.card, self.registry, self.root)
        self.registry['tasks']['T1']['revision'] = 1
        self.registry['interfaces']['test-api'] = 'v2'
        with self.assertRaisesRegex(ValueError, 'stale interface'):
            validate(self.card, self.registry, self.root)

    def test_code_changed_after_review_invalidates_ready(self):
        (self.root / 'code.py').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'changed source'):
            validate(self.card, self.registry, self.root, 'ready')
        self.card['sources']['code.py'] = digest(self.root / 'code.py')
        self.registry['tasks']['T1']['subject_sha256'] = subject_digest(self.card)
        with self.assertRaisesRegex(ValueError, 'stale review'):
            validate(self.card, self.registry, self.root, 'ready')

    def test_self_review_and_unreleased_task_rejected(self):
        self.card['reviewers'] = ['P04']
        with self.assertRaisesRegex(ValueError, 'author'):
            validate(self.card, self.registry, self.root)
        self.card['reviewers'] = ['P05']
        self.registry['tasks']['T1']['state'] = 'defined'
        with self.assertRaisesRegex(ValueError, 'not released'):
            validate(self.card, self.registry, self.root)

    def test_dependency_requires_exact_accepted_successor(self):
        dep = {'revision': 2, 'subject_sha256': 'b' * 64}
        self.card['dependencies']['D'] = dep
        self.registry['tasks']['T1']['subject_sha256'] = subject_digest(self.card)
        self.registry['tasks']['D'] = dict(dep, state='review_pending')
        with self.assertRaisesRegex(ValueError, 'dependency'):
            validate(self.card, self.registry, self.root)
        self.registry['tasks']['D']['state'] = 'accepted'
        self.assertTrue(validate(self.card, self.registry, self.root)['passed'])
        self.registry['tasks']['D']['subject_sha256'] = 'c' * 64
        with self.assertRaisesRegex(ValueError, 'dependency'):
            validate(self.card, self.registry, self.root)

    def test_failure_unknown_and_modified_evidence_cannot_pass(self):
        for code in (1, None, False):
            card = copy.deepcopy(self.card)
            card['validation']['commands'][0]['exit_code'] = code
            with self.assertRaisesRegex(ValueError, 'commands'):
                validate(card, self.registry, self.root, 'ready')
        (self.root / 'evidence.json').write_text('{"passed":false}')
        with self.assertRaisesRegex(ValueError, 'changed source/evidence'):
            validate(self.card, self.registry, self.root, 'ready')

    def test_dependencies_require_valid_versions_on_both_sides(self):
        bad_values = [{}, {'revision': None, 'subject_sha256': None},
                      {'revision': True, 'subject_sha256': 'b' * 64},
                      {'revision': 0, 'subject_sha256': 'b' * 64},
                      {'revision': 1, 'subject_sha256': 'invalid'},
                      {'revision': 1, 'subject_sha256': 'B' * 64}]
        for value in bad_values:
            with self.subTest(value=value):
                self.card['dependencies']['D'] = value
                self.registry['tasks']['D'] = dict(value, state='accepted')
                self.registry['tasks']['T1']['subject_sha256'] = subject_digest(self.card)
                with self.assertRaisesRegex(ValueError, 'dependency'):
                    validate(self.card, self.registry, self.root)
        self.card['dependencies'] = {}
        self.registry['tasks']['T1']['subject_sha256'] = subject_digest(self.card)
        self.registry['tasks']['T1']['revision'] = True
        with self.assertRaisesRegex(ValueError, 'stale'):
            validate(self.card, self.registry, self.root)

    def test_concurrent_notification_claim_never_dispatches_twice(self):
        event = {'task_id': 'T1', 'revision': 1, 'state': 'review_pending',
                 'subject_sha256': subject_digest(self.card), 'evidence_sha256': 'e' * 64}
        directory = self.root / 'receipts'
        with ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(lambda _: claim_notification(directory, event), range(8)))
        self.assertEqual(results.count('claimed'), 1)
        self.assertEqual(results.count('already_claimed_outcome_unknown'), 7)
        event['evidence_sha256'] = 'f' * 64
        self.assertEqual(claim_notification(directory, event), 'claimed')
        event['subject_sha256'] = 'd' * 64
        with self.assertRaisesRegex(ValueError, 'conflicting notification'):
            claim_notification(directory, event)
        event['revision'] = 2
        self.assertEqual(claim_notification(directory, event), 'claimed')


if __name__ == '__main__':
    unittest.main()
