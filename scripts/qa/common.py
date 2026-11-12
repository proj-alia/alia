"""Small shared helpers for frozen-input QA; no credentials or paid calls."""
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import time
import threading
import uuid

LOCK_ROOT = Path('/tmp/alia-project-locks')
LOCK_ENV = 'ALIA_QA_LOCK_FDS'
_ACTIVE = threading.local()


def _lock_key(kind, scope):
    if not re.fullmatch(r'[a-z][a-z0-9-]{0,31}', kind):
        raise ValueError('invalid QA lock kind')
    digest = hashlib.sha256(str(scope).encode('utf-8')).hexdigest()[:24]
    return f'{kind}:{digest}', LOCK_ROOT / f'{kind}-{digest}.lock'


class LockSet:
    def __init__(self, descriptors, opened, queue_seconds):
        self.descriptors = descriptors
        self.opened = opened
        self.queue_seconds = queue_seconds

    @property
    def pass_fds(self):
        return tuple(sorted(set(self.descriptors.values())))

    def child_environment(self, environment=None):
        env = dict(os.environ if environment is None else environment)
        env[LOCK_ENV] = json.dumps(self.descriptors, sort_keys=True)
        # Keep the P01 live-model driver's single-lock handoff compatible.
        model_key, _ = _lock_key('model', 'shared')
        if model_key in self.descriptors:
            env['ALIA_QA_LOCK_FD'] = str(self.descriptors[model_key])
        else:
            env.pop('ALIA_QA_LOCK_FD', None)
        return env

    def close(self):
        for fd in reversed(self.opened):
            os.close(fd)


@contextmanager
def resource_locks(*resources, environment=None):
    """Acquire stable resource-scoped locks in a fixed order.

    Each item is ``(kind, scope)``. Nested QA helpers inherit exact descriptors
    through ALIA_QA_LOCK_FDS; an inherited descriptor must match the expected
    lock inode. Files stay in /tmp after release, and ownership is the live FD.
    """
    incoming = os.environ if environment is None else environment
    inherited = {}
    encoded = incoming.get(LOCK_ENV)
    if encoded:
        try:
            inherited = json.loads(encoded)
        except (TypeError, ValueError):
            raise ValueError('invalid inherited QA lock map') from None
        if not isinstance(inherited, dict) or any(
                not isinstance(k, str) or isinstance(v, bool) or not isinstance(v, int)
                for k, v in inherited.items()):
            raise ValueError('invalid inherited QA lock map')
    # P01's established behavior driver still exports one model-lock FD.
    legacy_fd = incoming.get('ALIA_QA_LOCK_FD')
    if legacy_fd is not None:
        try:
            legacy_fd = int(legacy_fd)
            model_key, _ = _lock_key('model', 'shared')
            inherited.setdefault(model_key, legacy_fd)
        except (TypeError, ValueError):
            raise ValueError('invalid inherited model lock descriptor') from None

    requested = {}
    for item in resources:
        if not isinstance(item, (tuple, list)) or len(item) != 2:
            raise ValueError('QA resources must be (kind, scope) pairs')
        key, path = _lock_key(item[0], item[1])
        requested[key] = path

    descriptors = {}
    opened = []
    queue_seconds = 0.0
    active = getattr(_ACTIVE, 'descriptors', {})
    try:
        for key, path in sorted(requested.items()):
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            if path.parent.stat().st_mode & 0o077:
                raise ValueError('QA lock directory permissions are not private')
            inherited_fd = inherited.get(key)
            if inherited_fd is None:
                inherited_fd = active.get(key)
            expected = path.stat() if path.exists() else None
            if inherited_fd is not None:
                try:
                    actual = os.fstat(inherited_fd)
                except OSError:
                    raise ValueError('invalid inherited QA lock descriptor') from None
                if expected is None or (actual.st_dev, actual.st_ino) != (expected.st_dev, expected.st_ino):
                    raise ValueError('inherited QA lock descriptor identity mismatch')
                try:
                    fcntl.flock(inherited_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except OSError:
                    raise ValueError('inherited QA lock is not held by this task') from None
                descriptors[key] = inherited_fd
                continue

            fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
            opened.append(fd)
            if os.fstat(fd).st_mode & 0o077:
                raise ValueError('QA lock file permissions are not private')
            start = time.monotonic()
            fcntl.flock(fd, fcntl.LOCK_EX)
            queue_seconds += time.monotonic() - start
            descriptors[key] = fd
            if not hasattr(_ACTIVE, 'descriptors'):
                _ACTIVE.descriptors = active
            active[key] = fd
        # Preserve only inherited locks that were actually requested by this
        # child tree; unrelated descriptors are never re-exported.
        yield LockSet(descriptors, opened, queue_seconds)
    finally:
        for fd in reversed(opened):
            for key, value in list(active.items()):
                if value == fd:
                    del active[key]
            try:
                os.close(fd)
            except OSError:
                pass


def utc():
    return datetime.now(timezone.utc).isoformat()


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read_json(path):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('duplicate JSON key')
            result[key] = value
        return result
    return json.loads(Path(path).read_text(), object_pairs_hook=pairs)


def write_new(path, data, private=False):
    path = Path(path)
    temporary = path.with_name('.' + path.name + '.' + uuid.uuid4().hex + '.partial')
    try:
        with os.fdopen(os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                               0o600 if private else 0o644), 'w') as f:
            json.dump(data, f, indent=2, ensure_ascii=False, allow_nan=False)
            f.write('\n')
            f.flush()
            os.fsync(f.fileno())
        # Atomic publish without overwrite. Temporary and target are same filesystem.
        os.link(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        temporary.unlink(missing_ok=True)



def clean_env():
    # Explicitly do not inherit deployment, provider or test credentials.
    keep = {'PATH', 'HOME', 'USER', 'LOGNAME', 'LANG', 'TZ', 'CARGO_HOME',
            'RUSTUP_HOME', 'LD_LIBRARY_PATH', 'SSL_CERT_FILE', 'SSL_CERT_DIR',
            'PLAYWRIGHT_BROWSERS_PATH', 'CHROMIUM_EXECUTABLE'}
    return {k: v for k, v in os.environ.items() if k in keep or k.startswith('LC_')}


def safe_relative(name):
    p = Path(name)
    if not isinstance(name, str) or p.is_absolute() or '..' in p.parts or str(p) != name:
        raise ValueError('unsafe relative path')
    return p


@contextmanager
def heavy_lock():
    """Compatibility alias for the shared model-budget lock."""
    with resource_locks(('model', 'shared')) as locks:
        key, _ = _lock_key('model', 'shared')
        yield locks.descriptors[key], locks.queue_seconds
