"""Source-declared observer publication contracts; temporary byte fixtures only."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/qa'))
from artifacts import verify_artifacts
from common import digest, read_json
from freeze import freeze
from run_engineering import Run


LEGACY_EXAMPLES = {'seed_web_inspection', 'live_individual', 'live_autonomy',
    'live_uncertainty', 'live_grounding', 'live_entities', 'live_person_retrieval',
    'verify_tokenizer'}
OBSERVER = 'live_belief_revisions'


class ObserverArtifacts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='alia-p03-observer-unit-')
        self.base = Path(self.temp.name)
        self.development = self.base / 'development'
        (self.development / 'src').mkdir(parents=True)
        (self.development / 'src/lib.rs').write_text('// frozen synthetic library\n')
        (self.development / 'Cargo.toml').write_text('[package]\nname="fixture"\n')
        (self.development / 'examples').mkdir()
        self.checkpoint = self.base / 'checkpoint'
        self.target = self.base / 'target'

    def tearDown(self):
        for base, dirs, files in os.walk(self.base):
            if not Path(base).is_symlink():
                Path(base).chmod(0o700)
            for name in files:
                p = Path(base) / name
                if not p.is_symlink():
                    p.chmod(0o600)
        self.temp.cleanup()

    def prepare(self, *, declared=False, build_observer=True, other_source=None):
        if declared:
            (self.development / f'examples/{OBSERVER}.rs').write_text('// ninth fixture\n')
        if other_source:
            p = self.development / other_source
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('// lookalike must not declare a runtime\n')
        freeze(self.development, self.checkpoint)
        run = Run(self.checkpoint, self.base / 'run', Path(sys.executable), None)
        names = {'alia'} | {'examples/' + n for n in LEGACY_EXAMPLES}
        if build_observer:
            names.add('examples/' + OBSERVER)
        for name in names:
            p = self.target / 'debug' / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('synthetic compiled artifact: ' + name)
        (run.output / 'web-dist/assets').mkdir(parents=True)
        (run.output / 'web-dist/index.html').write_text('<html>fixture</html>')
        (run.output / 'web-dist/assets/app.js').write_text('// complete static fixture')
        return run

    def publish(self, **kwargs):
        run = self.prepare(**kwargs)
        run.publish_artifacts(self.target, True)
        return run, run.output / 'artifacts'

    def verify(self, base):
        return verify_artifacts(self.checkpoint, base / 'bin', base / 'web')

    def rewrite_manifest(self, base, change):
        path = base / 'artifacts.json'
        value = read_json(path)
        change(value['files'])
        path.chmod(0o600)
        path.write_text(json.dumps(value))

    def test_legacy_round_trip_keeps_all_eight_and_ignores_unbuilt_undeclared_observer(self):
        run, base = self.publish(build_observer=False)
        _, artifact = self.verify(base)
        expected = {'bin/alia'} | {'bin/examples/' + n for n in LEGACY_EXAMPLES}
        self.assertEqual({n for n in artifact['files'] if n.startswith('bin/')}, expected)
        compat = read_json(base / 'p01-manifest.json')
        self.assertEqual(set(compat['binaries']), LEGACY_EXAMPLES)
        self.assertEqual(compat['sources'], run.manifest['sources'])
        self.assertEqual(artifact['files']['p01-manifest.json'], digest(base / 'p01-manifest.json'))

    def test_declared_ninth_round_trip_covers_binary_and_compatibility_hash(self):
        run, base = self.publish(declared=True)
        _, artifact = self.verify(base)
        self.assertEqual(len([n for n in artifact['files'] if n.startswith('bin/')]), 10)
        binary = base / f'bin/examples/{OBSERVER}'
        self.assertEqual(artifact['files'][f'bin/examples/{OBSERVER}'], digest(binary))
        compat = read_json(base / 'p01-manifest.json')
        self.assertEqual(set(compat['binaries']), LEGACY_EXAMPLES | {OBSERVER})
        self.assertEqual(compat['binaries'][OBSERVER], digest(binary))
        self.assertEqual(compat['checkpoint_sha256'], digest(self.checkpoint / 'manifest.json'))
        self.assertEqual(run.report['artifacts_sha256'], digest(base / 'artifacts.json'))

    def test_undeclared_built_ninth_is_not_published_from_target_directory(self):
        _, base = self.publish()
        self.verify(base)
        self.assertTrue((self.target / f'debug/examples/{OBSERVER}').is_file())
        self.assertFalse((base / f'bin/examples/{OBSERVER}').exists())
        self.assertNotIn(OBSERVER, read_json(base / 'p01-manifest.json')['binaries'])

    def test_later_live_observer_source_cannot_change_verified_checkpoint_requirement(self):
        run = self.prepare()
        (self.development / f'examples/{OBSERVER}.rs').write_text('// later live source\n')
        run.publish_artifacts(self.target, True)
        base = run.output / 'artifacts'
        self.verify(base)
        self.assertFalse((base / f'bin/examples/{OBSERVER}').exists())

    def test_lookalike_source_does_not_declare_the_fixed_observer(self):
        _, base = self.publish(other_source='examples/nested/live_belief_revisions.rs')
        self.verify(base)
        self.assertFalse((base / f'bin/examples/{OBSERVER}').exists())

    def test_arbitrary_example_source_does_not_select_an_arbitrary_binary_name(self):
        run = self.prepare(other_source='examples/untrusted_program.rs')
        p = self.target / 'debug/examples/untrusted_program'
        p.write_text('never publish names chosen by source input')
        run.publish_artifacts(self.target, True)
        base = run.output / 'artifacts'
        self.verify(base)
        self.assertFalse((base / 'bin/examples/untrusted_program').exists())

    def test_declared_but_unbuilt_ninth_refuses_publication_without_success_manifest(self):
        run = self.prepare(declared=True, build_observer=False)
        with self.assertRaises(FileNotFoundError):
            run.publish_artifacts(self.target, True)
        self.assertFalse((run.output / 'artifacts/artifacts.json').exists())
        self.assertNotIn('artifacts_sha256', run.report)

    def test_declared_ninth_missing_from_tree_and_manifest_is_rejected(self):
        _, base = self.publish(declared=True)
        (base / f'bin/examples/{OBSERVER}').unlink()
        self.rewrite_manifest(base, lambda files: files.pop(f'bin/examples/{OBSERVER}'))
        with self.assertRaisesRegex(ValueError, 'required runtime'):
            self.verify(base)

    def test_present_ninth_without_hash_coverage_is_rejected(self):
        _, base = self.publish(declared=True)
        self.rewrite_manifest(base, lambda files: files.pop(f'bin/examples/{OBSERVER}'))
        with self.assertRaisesRegex(ValueError, 'required runtime'):
            self.verify(base)

    def test_binary_only_consumer_also_requires_the_declared_observer(self):
        _, base = self.publish(declared=True)
        (base / f'bin/examples/{OBSERVER}').unlink()
        self.rewrite_manifest(base, lambda files: files.pop(f'bin/examples/{OBSERVER}'))
        with self.assertRaisesRegex(ValueError, 'required runtime'):
            verify_artifacts(self.checkpoint, base / 'bin')

    def test_legacy_checkpoint_rejects_hash_covered_but_undeclared_ninth(self):
        _, base = self.publish()
        p = base / f'bin/examples/{OBSERVER}'
        p.write_text('injected unrequested runtime')
        self.rewrite_manifest(base, lambda files: files.update({f'bin/examples/{OBSERVER}': digest(p)}))
        with self.assertRaisesRegex(ValueError, 'undeclared runtime'):
            self.verify(base)

    def test_new_checkpoint_rejects_hash_covered_arbitrary_extra_binary(self):
        _, base = self.publish(declared=True)
        p = base / 'bin/examples/untrusted_program'
        p.write_text('injected arbitrary runtime')
        self.rewrite_manifest(base, lambda files: files.update({'bin/examples/untrusted_program': digest(p)}))
        with self.assertRaisesRegex(ValueError, 'undeclared runtime'):
            self.verify(base)

    def test_unlisted_extra_file_still_fails_exact_tree_check(self):
        _, base = self.publish(declared=True)
        (base / 'web/assets/unlisted.js').write_text('not covered')
        with self.assertRaisesRegex(ValueError, 'file set'):
            self.verify(base)

    def test_corrupted_ninth_binary_fails_digest_check(self):
        _, base = self.publish(declared=True)
        p = base / f'bin/examples/{OBSERVER}'
        p.chmod(0o600)
        p.write_text('corrupted observer')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            self.verify(base)

    def test_corrupted_compatibility_manifest_remains_hash_checked(self):
        _, base = self.publish(declared=True)
        (base / 'p01-manifest.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            self.verify(base)

    def test_ninth_binary_leaf_symlink_is_rejected_even_for_identical_bytes(self):
        _, base = self.publish(declared=True)
        p = base / f'bin/examples/{OBSERVER}'
        outside = self.base / 'outside-binary'
        p.rename(outside)
        p.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.verify(base)

    def test_example_parent_symlink_is_rejected(self):
        _, base = self.publish(declared=True)
        p = base / 'bin/examples'
        outside = self.base / 'outside-examples'
        p.rename(outside)
        p.symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.verify(base)

    def test_checkpoint_observer_source_corruption_is_rejected_before_artifact_acceptance(self):
        _, base = self.publish(declared=True)
        p = self.checkpoint / f'source/examples/{OBSERVER}.rs'
        p.chmod(0o600)
        p.write_text('// changed frozen declaration\n')
        with self.assertRaisesRegex(ValueError, 'checkpoint source'):
            self.verify(base)

    def test_artifact_path_traversal_does_not_expand_runtime_allowlist(self):
        _, base = self.publish(declared=True)
        self.rewrite_manifest(base, lambda files: files.update({'bin/examples/../../escape': '0' * 64}))
        with self.assertRaises(ValueError):
            self.verify(base)

    def test_missing_one_legacy_binary_still_fails_new_checkpoint(self):
        _, base = self.publish(declared=True)
        p = base / 'bin/examples/live_grounding'
        p.unlink()
        self.rewrite_manifest(base, lambda files: files.pop('bin/examples/live_grounding'))
        with self.assertRaisesRegex(ValueError, 'required runtime'):
            self.verify(base)


if __name__ == '__main__':
    unittest.main()
