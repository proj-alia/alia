#!/usr/bin/env python3
"""Credential routing regressions using fabricated keys; no HTTP or real secrets."""
import unittest
from probe_model import read_go_key


class CredentialRouting(unittest.TestCase):
    def test_other_provider_before_or_after_go_never_changes_selection(self):
        go = '# opencode-go\napikey: fabricated-go\n'
        other = '# openai\napikey: sk-proj-fabricated-openai\n'
        for notes in [go + other, other + go]:
            self.assertEqual(read_go_key(notes), 'fabricated-go')

    def test_duplicate_missing_unlabeled_and_interrupted_sections_are_rejected(self):
        for notes in [
            'apikey: fabricated-go',
            'opencode-go\nopenai\napikey: fabricated-other',
            'opencode-go\napikey: one\napikey: two',
            'opencode-go\napikey: one\nopencode-go\napikey: two',
            'opencode-go\nunknown-provider\napikey: fabricated-other',
        ]:
            with self.subTest(notes=notes), self.assertRaises(ValueError):
                read_go_key(notes)

    def test_mislabeled_openai_empty_or_malformed_values_are_rejected_without_echo(self):
        for value in ['sk-proj-fabricated-secret', '', 'fabricated secret', '"fabricated-secret"']:
            with self.subTest(value=value), self.assertRaises(ValueError) as result:
                read_go_key(f'opencode-go\napikey: {value}')
            if value:
                self.assertNotIn(value, str(result.exception))

    def test_markdown_blank_lines_and_chinese_colon(self):
        self.assertEqual(read_go_key('## opencode-go\n\n```\n# apikey：fabricated-go\n```'), 'fabricated-go')


if __name__ == '__main__':
    unittest.main()
