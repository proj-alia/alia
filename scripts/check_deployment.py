#!/usr/bin/env python3
"""Read-only loopback deployment check. Never prints credentials or private state."""
import hashlib
import json
from pathlib import Path
import re
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def main():
    config = dict(line.split('=', 1) for line in (ROOT / '.env').read_text().splitlines()
                  if line.strip() and not line.startswith('#'))
    port = int(config.get('ALIA_PORT', '3000'))
    assert 1 <= port <= 65535
    base = f'http://127.0.0.1:{port}'

    def read(route, authorized=False):
        headers = {'Authorization': 'Bearer ' + config['ALIA_INSTANCE_TOKEN']} if authorized else {}
        try:
            with urlopen(Request(base + route, headers=headers), timeout=3) as response:
                return response.status, response.read()
        except HTTPError as error:
            return error.code, error.read()

    for _ in range(60):
        try:
            code, body = read('/healthz')
            if code == 200 and json.loads(body) == {'status': 'ready'}:
                break
        except (URLError, TimeoutError):
            pass
        time.sleep(.5)
    else:
        raise RuntimeError('Loopback database readiness check failed')

    code, body = read('/api/v1/state')
    assert code == 401 and json.loads(body)['error']['code'] == 'unauthorized'
    status, body = read('/api/v1/state', authorized=True)
    assert status in (200, 409)
    if status == 409:
        assert json.loads(body)['error']['code'] == 'not_initialized'
    else:
        assert 'individual_id' in json.loads(body)
    code, page = read('/')
    assert code == 200 and page == (ROOT / 'web/dist/index.html').read_bytes()
    assets = re.findall(rb'(?:src|href)="(/assets/[^\"]+)"', page)
    assert len(assets) >= 2
    for asset in assets:
        name = asset.decode()
        assert '..' not in name and '?' not in name
        code, body = read(name)
        assert code == 200
        assert hashlib.sha256(body).digest() == hashlib.sha256((ROOT / ('web/dist' + name)).read_bytes()).digest()
    print(f'Deployment PASS: health=200, unauthorized=401, authenticated_state={status}; HTML/assets match; no writes')


if __name__ == '__main__':
    main()
