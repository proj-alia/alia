#!/usr/bin/env python3
"""Run the real adapter using local secrets, reporting only non-secret metadata."""
import argparse
import os
from pathlib import Path
import re
import subprocess


def read_go_key(content: str) -> str:
    """Read only the explicitly named Go section; never select by file order."""
    in_go_section = False
    keys = []
    for line in content.splitlines():
        if not line.strip() or line.strip().startswith('```'):
            continue
        entry = re.fullmatch(r'\s*#?\s*apikey\s*[:：]\s*(.*?)\s*', line, re.I)
        if entry:
            if in_go_section:
                value = entry.group(1)
                if not re.fullmatch(r'[^\s\"\'`]+', value) or value.startswith('sk-proj-'):
                    raise ValueError('Invalid OpenCode Go credential entry; contents are not printed.')
                keys.append(value)
            continue
        # Any other heading or note ends the section. Do not associate another
        # provider's key merely because "opencode-go" appears elsewhere.
        in_go_section = line.strip().strip('# :：`*[]').lower() == 'opencode-go'
    if len(keys) != 1:
        raise ValueError('Expected exactly one key in an explicit opencode-go section; contents are not printed.')
    return keys[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--credentials', type=Path, help='Notes file with an explicit opencode-go section and apikey entry')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    environment = os.environ.copy()
    if (root / '.env').exists():
        for line in (root / '.env').read_text().splitlines():
            if '=' in line and not line.lstrip().startswith('#'):
                key, value = line.split('=', 1)
                environment.setdefault(key.strip(), value.strip())
    if args.credentials:
        try:
            key = read_go_key(args.credentials.read_text())
        except ValueError as error:
            raise SystemExit(str(error)) from None
        environment['OPENCODE_API_KEY'] = key
        environment['ALIA_MODEL_PROVIDER'] = 'opencode_go'
        environment['ALIA_MODEL_ENDPOINT'] = 'https://opencode.ai/inference/go/openai/v1'
        if not environment.get('ALIA_MODEL'):
            environment['ALIA_MODEL'] = 'glm-5.3-flash'
    return subprocess.run(['cargo', 'run', '--locked', '--example', 'probe_model'], cwd=root, env=environment).returncode


if __name__ == '__main__':
    raise SystemExit(main())
