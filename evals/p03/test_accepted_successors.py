"""Fixed Root successors; fixtures are not real release/review evidence."""
import json
from pathlib import Path
import unittest

import test_individual_workspaces as fixtures
import management_browser as mg
from common import digest,read_json


class AcceptedSuccessors(unittest.TestCase):
    setUp=fixtures.LayeredInputs.setUp
    tearDown=fixtures.LayeredInputs.tearDown
    add_layer=fixtures.LayeredInputs.add_layer
    all_layers=fixtures.LayeredInputs.all_layers
    validate=fixtures.LayeredInputs.validate

    def successor(self,name,files):
        self.add_layer(name,files)
        p=self.source/f'evals/p03/accepted-slices/{name}.json'
        old=read_json(p)
        p.write_text(json.dumps({'scope':'synthetic fixed Root successor', 'sources':old['sources'],
            'source_manifest':{'path':'evals/results/fixture/source-manifest.json','sha256':'a'*64},
            'report':{'path':'evals/results/fixture/report.json','sha256':'b'*64}}))
        self.optional_pins[name]=digest(p);mg.OPTIONAL_SLICES[name]=digest(p)

    def successors(self):
        self.all_layers()
        self.successor('p02-05',{'web/scripts/test-memory-deletion.mjs':'accepted deletion browser',
            'web/src/features/memory/MemoryInspector.tsx':'deletion inspector',
            'web/scripts/test-memory-revisions.mjs':'deletion-compatible revisions',
            'web/scripts/test-source-inspector.mjs':'deletion-compatible sources'})
        self.successor('p02-07',{'web/scripts/test-sse-privacy.mjs':'accepted SSE browser',
            'web/src/shared/useIndividual.ts':'accepted SSE invalidation'})

    def test_successors_final_order_overwrites_only_explicit_accepted_paths(self):
        self.successors();manifest,bindings=self.validate()
        self.assertEqual(list(bindings)[-2:],['p02-05','p02-07'])
        self.assertEqual(len(bindings),12)
        self.assertEqual((self.checkpoint/'source/web/src/features/memory/MemoryInspector.tsx').read_text(),'deletion inspector')
        self.assertEqual((self.checkpoint/'source/web/src/shared/useIndividual.ts').read_text(),'accepted SSE invalidation')

    def test_each_original_inspector_version_cannot_replace_deletion_successor(self):
        self.successors()
        for index,text in enumerate(('first inspector','final shared inspector','unknown live inspector')):
            with self.subTest(text=text):
                (self.source/'web/src/features/memory/MemoryInspector.tsx').write_text(text)
                self.checkpoint=self.base/('checkpoint-'+str(index))
                with self.assertRaisesRegex(ValueError,'missing or changed accepted source'):
                    self.validate()

    def test_unknown_new_hash_rejected_even_when_known_scripts_are_present(self):
        self.successors();(self.source/'web/src/shared/useIndividual.ts').write_text('unreleased live change')
        with self.assertRaisesRegex(ValueError,'missing or changed accepted source'):self.validate()

    def test_sse_successor_requires_explicit_deletion_predecessor_manifest(self):
        self.successors();(self.source/'evals/p03/accepted-slices/p02-05.json').unlink()
        with self.assertRaisesRegex(ValueError,'manifest binding mismatch: p02-05'):self.validate()

    def test_declared_successor_script_cannot_omit_its_accepted_manifest(self):
        self.successors();(self.source/'evals/p03/accepted-slices/p02-07.json').unlink()
        with self.assertRaisesRegex(ValueError,'manifest binding mismatch: p02-07'):self.validate()

    def test_successor_manifest_byte_change_cannot_be_reinterpreted_as_new_acceptance(self):
        self.successors();p=self.source/'evals/p03/accepted-slices/p02-05.json';p.write_text(p.read_text()+'\n')
        with self.assertRaisesRegex(ValueError,'manifest binding mismatch: p02-05'):self.validate()

    def test_only_known_root_envelope_shape_is_supported(self):
        self.successors();p=self.source/'evals/p03/accepted-slices/p02-07.json';value=read_json(p);value['accepted']=True
        p.write_text(json.dumps(value));mg.OPTIONAL_SLICES['p02-07']=digest(p)
        with self.assertRaisesRegex(ValueError,'invalid accepted manifest: p02-07'):self.validate()

    def test_successor_cannot_escape_evidence_path(self):
        self.successors();p=self.source/'evals/p03/accepted-slices/p02-07.json';value=read_json(p)
        value['source_manifest']['path']='../untrusted.json';p.write_text(json.dumps(value));mg.OPTIONAL_SLICES['p02-07']=digest(p)
        with self.assertRaisesRegex(ValueError,'unsafe relative path'):self.validate()

    def test_successor_cannot_hide_required_browser_in_rebound_manifest(self):
        self.successors();p=self.source/'evals/p03/accepted-slices/p02-07.json';value=read_json(p)
        del value['sources']['web/scripts/test-sse-privacy.mjs']
        p.write_text(json.dumps(value));mg.OPTIONAL_SLICES['p02-07']=digest(p)
        with self.assertRaisesRegex(ValueError,'successor browser script absent'):self.validate()


if __name__=='__main__':unittest.main()
