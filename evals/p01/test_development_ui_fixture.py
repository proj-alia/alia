"""Wrapper guard checks; no service, provider or database is started here."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts/qa'))
import development_ui_fixture as fixture
from common import digest


class ProducerGuard(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'source'
        sources = {}
        for name in ['src/lib.rs', 'examples/development_ui_fixture.rs', 'scripts/qa/development_ui_fixture.py']:
            p = self.source / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('synthetic source identity only')
            if name == 'scripts/qa/development_ui_fixture.py':
                p.write_bytes(Path(fixture.__file__).read_bytes())
            sources[name] = digest(p)
        self.base = self.root / 'base.json'
        self.base.write_text(json.dumps({'sources': {'src/lib.rs': sources['src/lib.rs']}}))
        self.manifest = self.root / 'manifest.json'
        self.manifest.write_text(json.dumps({'sources': sources}))
        self.binary = self.root / 'binary'
        self.binary.write_text('not executable; must never be launched by these tests')
        self.request = dict(database_state=str(self.root/'state.json'), database='alia_p02_synthetic',
            individual_id='11111111-1111-4111-8111-111111111111',service_url='http://127.0.0.1:12345/api/v1',
            service_token='SYNTHETIC-PRIVATE-MARKER', source_root=str(self.source),
            source_manifest=str(self.manifest),source_manifest_sha256=digest(self.manifest),
            base_manifest=str(self.base),producer_binary=str(self.binary),producer_binary_sha256=digest(self.binary),
            service_binary=str(self.binary),service_binary_sha256=digest(self.binary),output=str(self.root/'new.json'))
        self.pin = patch.object(fixture,'BASE_MANIFEST',digest(self.base));self.pin.start();self.addCleanup(self.pin.stop)

    def test_bound_request_passes_without_execution(self):
        with patch.object(fixture.subprocess,'run') as run:
            fixture.validate_request(self.request)
            run.assert_not_called()

    def test_production_name_identity_endpoint_and_unknown_fields_rejected(self):
        for field,value in [('database','production'),('individual_id','not-an-id'),
                            ('service_url','https://external.invalid/api/v1'),('extra','private body')]:
            with self.subTest(field=field):
                request=copy.deepcopy(self.request);request[field]=value
                with self.assertRaises(ValueError):fixture.validate_request(request)

    def test_source_and_binary_change_rejected(self):
        self.binary.write_text('different binary')
        with self.assertRaises(ValueError):fixture.validate_request(self.request)
        self.binary.write_text('not executable; must never be launched by these tests')
        (self.source/'src/lib.rs').write_text('new live backend')
        with self.assertRaises(ValueError):fixture.validate_request(self.request)

    def test_output_not_overwritten(self):
        Path(self.request['output']).write_text('old evidence')
        with self.assertRaises(ValueError):fixture.validate_request(self.request)
        self.assertEqual(Path(self.request['output']).read_text(),'old evidence')

    def test_private_request_requires_owner0600(self):
        p=self.root/'private.json';p.write_text(json.dumps(self.request));p.chmod(0o644)
        with self.assertRaises(ValueError):fixture.private_json(p)
        p.chmod(0o600);self.assertEqual(fixture.private_json(p),self.request)

    def test_cli_error_never_echoes_private_input(self):
        p=self.root/'private.json';p.write_text(json.dumps(self.request));p.chmod(0o600)
        with patch.object(sys,'argv',['fixture','--request',str(p)]),patch('sys.stderr') as err:
            self.assertEqual(fixture.main(),1)  # no actual parent state exists
        self.assertNotIn('SYNTHETIC-PRIVATE-MARKER',str(err.write.call_args_list))


if __name__ == '__main__':unittest.main()
