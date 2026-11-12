#!/usr/bin/env python3
"""Copy an explicit source allowlist, verify before/after, publish once only."""
import argparse
from pathlib import Path
import os
import re
import shutil
import sys
import uuid
from common import digest, read_json, safe_relative, utc, write_new

ROOT = Path(__file__).resolve().parents[2]
ROOT_FILES = {'AGENTS.md', 'README.md', 'Cargo.toml', 'Cargo.lock', 'build.rs',
              'requirements-dev.txt', 'requirements-budget.txt', 'compose.yaml', 'compose.dev.yaml',
              'compose.go-tokenizer.yaml', 'compose.embeddings.yaml',
              'Dockerfile', 'Dockerfile.embeddings', '.gitignore', '.dockerignore'}
SUFFIXES = {'src': {'.rs'}, 'tests': {'.rs'}, 'examples': {'.rs'},
            'migrations': {'.sql'}, 'schemas': {'.json'}, 'config': {'.json'},
            'scripts': {'.py'}, 'docs': {'.md', '.json', '.html', '.css', '.js', '.txt', '.py'},
            'evals': {'.json', '.sha256', '.md', '.py'},
            'web': {'.tsx', '.ts', '.css', '.svg', '.html', '.json', '.mjs'}}
DENIED = {'target', 'node_modules', 'dist', '__pycache__', 'results', 'screens',
          '.git', '.venv', 'data', 'tmp', 'logs'}


def allowed(path):
    parts = path.parts
    if path.as_posix() in ROOT_FILES or path.as_posix() in {'contexts/01.md', 'contexts/02.md'}:
        return True
    if path.name == 'results.json' or any(p in DENIED or p.startswith('.env') or p.endswith('.env') for p in parts):
        return False
    if path.suffix not in {'.py', '.rs', '.ts', '.tsx', '.mjs'} and any(re.search(r'(credential|secret|private|provider.*log)', p, re.I) for p in parts):
        return False
    return len(parts) > 1 and parts[0] in SUFFIXES and path.suffix in SUFFIXES[parts[0]]


def inventory(root):
    found = {}
    # Prune runtime/dependency dirs before walking; do not enumerate the 44GB target.
    for base, dirs, files in os.walk(root, followlinks=False):
        rel = Path(base).relative_to(root)
        dirs[:] = sorted(d for d in dirs if d not in DENIED and not d.startswith('.') and
                         not (rel == Path('evals') and d == 'results'))
        for d in dirs:
            if (Path(base) / d).is_symlink():
                raise ValueError('source directory symlink refused')
        for name in sorted(files):
            p = rel / name
            if allowed(p):
                source = root / p
                if source.is_symlink() or not source.is_file():
                    raise ValueError('source symlink/non-file refused')
                found[p.as_posix()] = digest(source)
    return dict(sorted(found.items()))


def verify(checkpoint):
    checkpoint = Path(checkpoint).resolve()
    manifest = read_json(checkpoint / 'manifest.json')
    if manifest.get('format') != 'alia-p03-checkpoint/1' or not manifest.get('id'):
        raise ValueError('unsupported checkpoint manifest')
    expected = manifest.get('sources')
    if not isinstance(expected, dict) or not expected:
        raise ValueError('missing checkpoint sources')
    for name, sha in expected.items():
        safe_relative(name)
        if not allowed(Path(name)) or not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{64}', sha):
            raise ValueError('invalid checkpoint source entry')
    actual_files = set()
    for base, dirs, files in os.walk(checkpoint / 'source', followlinks=False):
        if any((Path(base) / d).is_symlink() for d in dirs):
            raise ValueError('checkpoint directory symlink refused')
        for name in files:
            file = Path(base) / name
            if file.is_symlink():
                raise ValueError('checkpoint symlink refused')
            actual_files.add(file.relative_to(checkpoint / 'source').as_posix())
    if actual_files != set(expected) or inventory(checkpoint / 'source') != expected:
        raise ValueError('checkpoint source hashes/set mismatch')
    return manifest


def freeze(root, output, after_copy=None):
    root, output = Path(root).resolve(), Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir()  # never overwrite, including abandoned partial freezes
    source = output / 'source'
    source.mkdir()
    try:
        before = inventory(root)
        if not before:
            raise ValueError('empty source allowlist')
        for name in before:
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / name, target)
        if after_copy:
            after_copy()
        if inventory(root) != before or inventory(source) != before:
            raise ValueError('source changed during freeze')
        manifest = {'format': 'alia-p03-checkpoint/1', 'id': output.name,
                    'created_at': utc(), 'sources': before,
                    'contexts': 'Only immutable managed 01/02, copied unchanged for M0 provenance',
                    'exclusions': sorted(DENIED), 'paid_calls': 0}
        write_new(output / 'manifest.json', manifest)
        for base, dirs, files in os.walk(source, topdown=False):
            for name in files:
                (Path(base) / name).chmod(0o444)
            Path(base).chmod(0o555)
        (output / 'manifest.json').chmod(0o444)
        verify(output)
        return manifest
    except Exception as error:
        write_new(output / 'freeze-failure.json', {'at': utc(), 'error': str(error)})
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    m = verify(args.output) if args.verify else freeze(args.root, args.output)
    print(f"Checkpoint {m['id']}: {len(m['sources'])} verified source files")

if __name__ == '__main__':
    main()
