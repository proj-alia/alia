"""Published build hashes are bound to an immutable checkpoint manifest."""
import os
from pathlib import Path
import re
from common import digest, read_json, safe_relative
from freeze import verify

REQUIRED_BINARIES = {'alia', 'examples/seed_web_inspection', 'examples/live_individual',
    'examples/live_autonomy', 'examples/live_uncertainty', 'examples/live_grounding',
    'examples/live_entities', 'examples/live_person_retrieval', 'examples/verify_tokenizer'}
BELIEF_OBSERVER_SOURCE = 'examples/live_belief_revisions.rs'
BELIEF_OBSERVER_BINARY = 'examples/live_belief_revisions'


def required_binaries(manifest):
    """Fixed runtime names, conditional only on an already verified source map."""
    binaries = set(REQUIRED_BINARIES)
    if BELIEF_OBSERVER_SOURCE in manifest['sources']:
        binaries.add(BELIEF_OBSERVER_BINARY)
    return binaries


def no_symlinks(path):
    path = Path(path).absolute()
    for part in [*path.parents, path]:
        if part.is_symlink():
            raise ValueError('artifact path component is a symlink')
    return path


def verify_artifacts(checkpoint, binary_dir, web_dir=None):
    checkpoint = Path(checkpoint).resolve()
    binary_dir = no_symlinks(binary_dir)
    manifest = verify(checkpoint)
    base = binary_dir.parent
    if binary_dir.name != 'bin':
        raise ValueError('binary directory must be the published bin directory')
    path = base / 'artifacts.json'
    no_symlinks(path)
    artifacts = read_json(path)
    if artifacts.get('format') != 'alia-p03-artifacts/1' or artifacts.get('checkpoint_sha256') != digest(checkpoint / 'manifest.json'):
        raise ValueError('artifact/checkpoint mismatch')
    files = artifacts.get('files')
    if not isinstance(files, dict) or not files:
        raise ValueError('artifact files missing')
    required = {'bin/' + name for name in required_binaries(manifest)}
    if {name for name in files if name.startswith('bin/')} - required:
        raise ValueError('undeclared runtime binary')
    if web_dir is not None:
        if no_symlinks(web_dir) != base / 'web':
            raise ValueError('web directory not bound to artifacts')
        required.add('web/index.html')
    if not required <= files.keys():
        raise ValueError('required runtime artifacts not covered by manifest')
    actual = set()
    for root, dirs, names in os.walk(base, followlinks=False):
        for name in dirs + names:
            no_symlinks(Path(root) / name)
        for name in names:
            relative = (Path(root) / name).relative_to(base).as_posix()
            if relative != 'artifacts.json':
                actual.add(relative)
    if actual != files.keys():
        raise ValueError('artifact file set differs from manifest')
    for name, sha in files.items():
        relative = safe_relative(name)
        if not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{64}', sha):
            raise ValueError('invalid artifact digest')
        file = no_symlinks(base / relative)
        if not file.is_file() or digest(file) != sha:
            raise ValueError('artifact hash mismatch')
    return manifest, artifacts
