#!/usr/bin/env python3
"""Install only pinned public E5 assets, verifying length/hash before rename."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request
ROOT=Path(__file__).resolve().parents[1]

def digest(path):
    sha=hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda:stream.read(1024*1024),b''):sha.update(block)
    return sha.hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--destination',type=Path,default=ROOT/'data/models/multilingual-e5-small')
    args=parser.parse_args()
    manifest=json.loads((ROOT/'config/embedding-assets.json').read_text())
    for entry in manifest['files']:
        relative=Path(entry['path'])
        assert not relative.is_absolute() and '..' not in relative.parts
        target=args.destination/relative
        if target.is_file() and target.stat().st_size==entry['size'] and digest(target)==entry['sha256']:
            print('Verified '+entry['path'],flush=True);continue
        target.parent.mkdir(parents=True,exist_ok=True)
        temp=None
        try:
            url=f"https://huggingface.co/{manifest['model']}/resolve/{manifest['revision']}/{entry['path']}"
            with urllib.request.urlopen(url,timeout=60) as response, tempfile.NamedTemporaryFile(dir=target.parent,delete=False) as output:
                temp=Path(output.name);sha=hashlib.sha256();size=0
                while block:=response.read(1024*1024):
                    size+=len(block)
                    if size>entry['size']:raise ValueError('Unexpected public asset size')
                    output.write(block);sha.update(block)
                if size!=entry['size'] or sha.hexdigest()!=entry['sha256']:raise ValueError('Public asset verification failed')
                output.flush();os.fsync(output.fileno());os.fchmod(output.fileno(),0o644)
            os.replace(temp,target);temp=None
            parent=os.open(target.parent,os.O_RDONLY|os.O_DIRECTORY)
            try:os.fsync(parent)
            finally:os.close(parent)
            print('Installed '+entry['path'],flush=True)
        finally:
            if temp:temp.unlink(missing_ok=True)
if __name__=='__main__':main()
