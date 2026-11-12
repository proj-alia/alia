#!/usr/bin/env python3
"""Install the pinned public text tokenizer; no inference, credentials or DB writes."""
import argparse
import hashlib
import os
from pathlib import Path
import tempfile
import urllib.request

REVISION = '46a050bec13002ad29ee795c16e61cf4fa3c4a9c'
SHA256 = '19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d'
MAX_BYTES = 32 * 1024 * 1024
ROOT = Path(__file__).resolve().parents[1]

def verified(data):
    return len(data) <= MAX_BYTES and hashlib.sha256(data).hexdigest() == SHA256

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=Path, help='Use a previously downloaded local public artifact')
    parser.add_argument('--destination', type=Path, default=ROOT/'data/models/glm-5.3-flash/tokenizer.json')
    args = parser.parse_args()
    if args.destination.is_file():
        with args.destination.open('rb') as f:
            if verified(f.read(MAX_BYTES+1)):
                print('Pinned tokenizer already installed and verified')
                return
    if args.source:
        with args.source.open('rb') as f:
            data = f.read(MAX_BYTES+1)
    else:
        url = f'https://huggingface.co/zai-org/GLM-5.3-Flash/resolve/{REVISION}/tokenizer.json'
        with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent':'Alia/0.1'}), timeout=60) as response:
            data = response.read(MAX_BYTES+1)
    if not verified(data):
        raise SystemExit('Model artifact hash/size mismatch; installed file was not changed')
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    name = None
    try:
        with tempfile.NamedTemporaryFile(dir=args.destination.parent, prefix='.tokenizer-', delete=False) as f:
            name = f.name
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
            os.fchmod(f.fileno(), 0o644) # Public model asset, no secrets.
        os.replace(name, args.destination)
        name = None
        parent = os.open(args.destination.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(parent)
        finally:
            os.close(parent)
    finally:
        if name:
            Path(name).unlink(missing_ok=True)
    print(f'Pinned tokenizer installed; SHA256={SHA256}')

if __name__ == '__main__':
    main()
